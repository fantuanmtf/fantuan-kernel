//! Kernel & user tasks + round-robin scheduler (M3/M4, DESIGN.md §4.6).
//!
//! Each task owns a frame-allocated 16 KiB kernel stack; the PIT quantum calls
//! schedule() from the IRQ0 handler; switch_context saves/restores the
//! callee-saved registers and the page tables on the task stacks. User tasks
//! (M4) get their own PML4 and enter ring 3 through a pre-built iretq frame.
//! M4 scope notes: static task table; exited tasks are reaped by
//! task/reap.rs (M8.3b). User pages are RWX until the M8.3c hardening pass.

use core::ptr;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::cpu;
use crate::elf;
use crate::gdt;
use crate::mm::frame;
use crate::mm::paging::{self, phys_to_virt};
use crate::mm::user;
use crate::timer;

mod reap;
use fantuan_abi::{USER_CS_SEL, USER_DS_SEL, USER_STACK_TOP};

pub const MAX_TASKS: usize = 16;
const STACK_PAGES: u64 = 4; // 16 KiB per task
const USER_STACK_PAGES: u64 = 4;

#[derive(Clone, Copy, PartialEq)]
pub enum State {
    Unused,
    Ready,
    Sleeping { wake_tick: u64 },
    Exited,
}

pub struct Task {
    pub state: State,
    /// Where switch_context left the saved callee-saved registers.
    pub rsp: u64,
    /// Physical PML4 this task runs with (kernel tasks share the kernel one).
    pub cr3: u64,
    /// Kernel stack top: also the TSS rsp0 while this task is current.
    pub kernel_stack_top: u64,
    /// Whether the task executes in ring 3 (fault classification uses the
    /// interrupt frame's CS; reaping uses this to free the address space).
    pub is_user: bool,
    /// Kernel stack frame base (physical); reap_exited frees STACK_PAGES here.
    pub stack_phys: u64,
    pub body: fn() -> !,
    pub id: u64,
    pub exit_code: u64,
}

fn dead_body() -> ! {
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)) }
    }
}

const EMPTY_TASK: Task = Task {
    state: State::Unused,
    rsp: 0,
    cr3: 0,
    kernel_stack_top: 0,
    is_user: false,
    stack_phys: 0,
    body: dead_body,
    id: 0,
    exit_code: 0,
};

static mut TASKS: [Task; MAX_TASKS] = [EMPTY_TASK; MAX_TASKS];
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static NEXT_ID: AtomicU64 = AtomicU64::new(0);
pub static SWITCHES: AtomicU64 = AtomicU64::new(0);

extern "C" {
    fn switch_context(old_rsp: *mut u64, new_rsp: u64, new_cr3: u64);
    fn user_entry();
}

/// Make the boot context (kmain) task 0.
pub fn init(boot_stack_top: u64) {
    unsafe {
        TASKS[0] = Task {
            state: State::Ready,
            rsp: 0,
            cr3: paging::kernel_pml4(),
            kernel_stack_top: boot_stack_top,
            is_user: false,
            stack_phys: 0,
            body: dead_body,
            id: 0,
            exit_code: 0,
        };
    }
    gdt::set_rsp0(boot_stack_top);
}

fn find_slot() -> Option<usize> {
    (0..MAX_TASKS).find(|&i| unsafe { (*ptr::addr_of!(TASKS[i])).state == State::Unused })
}

/// Spawn a kernel task. Interrupt-safe: fully set up before the task becomes
/// visible to the scheduler.
pub fn spawn(body: fn() -> !) -> u64 {
    let flags = cpu::irq_save();
    let slot = find_slot().expect("task table full");
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;

    let stack_phys = frame::get()
        .alloc_contiguous(STACK_PAGES as usize)
        .expect("no contiguous frames for task stack");
    let stack_top = phys_to_virt(stack_phys + STACK_PAGES * frame::FRAME_SIZE);
    // [r15..rbx zeros][task_entry] <- rsp
    let sp = (stack_top - 7 * 8) as *mut u64;
    unsafe {
        for i in 0..6 {
            *sp.add(i) = 0;
        }
        *sp.add(6) = task_entry as *const () as u64;
        *ptr::addr_of_mut!(TASKS[slot]) = Task {
            state: State::Ready,
            rsp: sp as u64,
            cr3: paging::kernel_pml4(),
            kernel_stack_top: stack_top,
            is_user: false,
            stack_phys,
            body,
            id,
            exit_code: 0,
        };
    }

    cpu::irq_restore(flags);
    id
}

/// Spawn a user task from a static ELF image (M4).
pub fn spawn_user(elf_image: &[u8]) -> Option<u64> {
    let flags = cpu::irq_save();
    let Some(slot) = find_slot() else {
        crate::serial::line("user: no free task slot");
        cpu::irq_restore(flags);
        return None;
    };
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;

    let Some((entry, cr3)) = elf::load(elf_image) else {
        cpu::irq_restore(flags);
        return None;
    };

    // User stack: contiguous frames mapped at USER_STACK_TOP - 16 KiB. Every
    // early exit must restore the interrupt state saved above, or an OOM would
    // leave IRQs disabled for the rest of the boot.
    let Some(ustack_phys) = frame::get().alloc_contiguous(USER_STACK_PAGES as usize) else {
        cpu::irq_restore(flags);
        return None;
    };
    let ustack_base = USER_STACK_TOP - USER_STACK_PAGES * frame::FRAME_SIZE;
    for i in 0..USER_STACK_PAGES {
        user::map_page(
            cr3,
            ustack_base + i * frame::FRAME_SIZE,
            ustack_phys + i * frame::FRAME_SIZE,
            user::P_PRESENT | user::P_WRITABLE | user::P_USER,
        );
    }

    // Kernel stack + the initial frame: six saved-register zeros, then
    // user_entry as the ret target, then the ring-3 iretq frame
    // [rip][cs][rflags][rsp][ss].
    let Some(stack_phys) = frame::get().alloc_contiguous(STACK_PAGES as usize) else {
        for i in 0..USER_STACK_PAGES {
            frame::get().free(ustack_phys + i * frame::FRAME_SIZE);
        }
        cpu::irq_restore(flags);
        return None;
    };
    let stack_top = phys_to_virt(stack_phys + STACK_PAGES * frame::FRAME_SIZE);
    let sp = (stack_top - 12 * 8) as *mut u64;
    unsafe {
        for i in 0..6 {
            *sp.add(i) = 0;
        }
        *sp.add(6) = user_entry as *const () as u64;
        *sp.add(7) = entry;
        *sp.add(8) = USER_CS_SEL as u64;
        *sp.add(9) = 0x202; // IF set: user code runs with interrupts on
        *sp.add(10) = USER_STACK_TOP;
        *sp.add(11) = USER_DS_SEL as u64;
        *ptr::addr_of_mut!(TASKS[slot]) = Task {
            state: State::Ready,
            rsp: sp as u64,
            cr3,
            kernel_stack_top: stack_top,
            is_user: true,
            stack_phys,
            body: dead_body,
            id,
            exit_code: 0,
        };
    }

    cpu::irq_restore(flags);
    Some(id)
}

/// First entry into a freshly spawned kernel task.
extern "C" fn task_entry() -> ! {
    let body = unsafe { (*ptr::addr_of!(TASKS[CURRENT.load(Ordering::Relaxed)])).body };
    body();
}

/// Round-robin: wake due sleepers, pick the next Ready task, switch.
/// Runs in the IRQ0 handler; the switch unwinds inside the next task's own
/// interrupt frame, so the ISR epilogue iretqs back into the right task.
pub fn schedule() {
    let cur = CURRENT.load(Ordering::Relaxed);
    // Reap finished tasks before choosing the next one: the current slot is
    // excluded, and the frame allocator is interrupt-safe (mm::lock).
    reap::reap_exited(cur);
    let now = timer::ticks();
    unsafe {
        for i in 0..MAX_TASKS {
            let t = &mut *ptr::addr_of_mut!(TASKS[i]);
            if let State::Sleeping { wake_tick } = t.state {
                if wake_tick <= now {
                    t.state = State::Ready;
                }
            }
        }
        let mut next = None;
        for off in 1..=MAX_TASKS {
            let i = (cur + off) % MAX_TASKS;
            if (*ptr::addr_of!(TASKS[i])).state == State::Ready {
                next = Some(i);
                break;
            }
        }
        let Some(n) = next else { return };
        if n == cur {
            return;
        }
        SWITCHES.fetch_add(1, Ordering::Relaxed);
        CURRENT.store(n, Ordering::Relaxed);
        // Point the ring-0 interrupt stack at the NEXT task's kernel stack
        // top BEFORE the switch: a fresh user task iretqs straight into ring 3
        // (its first activation never resumes schedule), so its very first
        // syscall needs rsp0 to already be correct.
        gdt::set_rsp0(TASKS[n].kernel_stack_top);
        let old_rsp = &raw mut TASKS[cur].rsp;
        let new_rsp = TASKS[n].rsp;
        let new_cr3 = TASKS[n].cr3;
        switch_context(old_rsp, new_rsp, new_cr3);
    }
}

pub fn sleep_ms(ms: u64) {
    let wake = timer::ticks() + (ms * 100 / 1000).max(1);
    unsafe {
        (*ptr::addr_of_mut!(TASKS[CURRENT.load(Ordering::Relaxed)])).state = State::Sleeping { wake_tick: wake };
    }
    schedule();
}

/// Mark the current task exited and never run it again.
pub fn exit(code: u64) -> ! {
    unsafe {
        let t = &mut *ptr::addr_of_mut!(TASKS[CURRENT.load(Ordering::Relaxed)]);
        t.state = State::Exited;
        t.exit_code = code;
    }
    loop {
        schedule();
    }
}

pub fn current_id() -> u64 {
    unsafe { (*ptr::addr_of!(TASKS[CURRENT.load(Ordering::Relaxed)])).id }
}
