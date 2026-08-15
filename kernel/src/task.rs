//! Kernel tasks + round-robin scheduler (M3, DESIGN.md §4.6).
//!
//! Each task owns a frame-allocated kernel stack; the PIT quantum calls
//! schedule() from the IRQ0 handler; switch_context saves/restores the
//! callee-saved registers on the task stacks. M3 scope notes: kernel-mode
//! tasks only (no ring 3 yet), static task table, exited tasks leak their
//! stacks (reaping comes with M4).

use core::ptr;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::cpu;
use crate::mm::frame::FrameAllocator;
use crate::mm::paging::phys_to_virt;
use crate::timer;

pub const MAX_TASKS: usize = 16;
const STACK_PAGES: u64 = 4; // 16 KiB per task

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
    /// Stack frame base (physical; for future reaping).
    #[allow(dead_code)]
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
    stack_phys: 0,
    body: dead_body,
    id: 0,
    exit_code: 0,
};

static mut TASKS: [Task; MAX_TASKS] = [EMPTY_TASK; MAX_TASKS];
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static NEXT_ID: AtomicU64 = AtomicU64::new(0);
pub static SWITCHES: AtomicU64 = AtomicU64::new(0);

// Borrowed from kmain at init; kmain never returns, so the local it points at
// lives forever (M4 will turn the allocator into a proper global).
static mut FRAME_ALLOC: *mut FrameAllocator = ptr::null_mut();

extern "C" {
    fn switch_context(old_rsp: *mut u64, new_rsp: u64);
}

/// Register the allocator and make the boot context (kmain) task 0.
pub fn init(alloc: &mut FrameAllocator) {
    unsafe {
        FRAME_ALLOC = alloc;
        TASKS[0] = Task {
            state: State::Ready,
            rsp: 0,
            stack_phys: 0,
            body: dead_body,
            id: 0,
            exit_code: 0,
        };
    }
}

/// Spawn a kernel task. Interrupt-safe: the slot and stack are fully set up
/// before the task becomes visible to the scheduler.
pub fn spawn(body: fn() -> !) -> u64 {
    let flags = cpu::irq_save();

    let slot = (0..MAX_TASKS)
        .find(|&i| unsafe { (*ptr::addr_of!(TASKS[i])).state == State::Unused })
        .expect("task table full");
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;

    let stack_phys = unsafe { &mut *FRAME_ALLOC }
        .alloc()
        .expect("no frames for task stack");
    // Initial stack: [r15][r14][r13][r12][rbp][rbx][task_entry] <- rsp, so the
    // first switch_context into this task pops six zeros and returns into
    // task_entry (the ret target sits ABOVE the six saved registers).
    let stack_top = phys_to_virt(stack_phys + STACK_PAGES * crate::mm::frame::FRAME_SIZE);
    let sp = (stack_top - 7 * 8) as *mut u64;
    unsafe {
        for i in 0..6 {
            *sp.add(i) = 0;
        }
        *sp.add(6) = task_entry as *const () as u64;
        *ptr::addr_of_mut!(TASKS[slot]) = Task {
            state: State::Ready,
            rsp: sp as u64,
            stack_phys,
            body,
            id,
            exit_code: 0,
        };
    }

    cpu::irq_restore(flags);
    id
}

/// First entry into a freshly spawned task.
extern "C" fn task_entry() -> ! {
    let body = unsafe { (*ptr::addr_of!(TASKS[CURRENT.load(Ordering::Relaxed)])).body };
    body();
}

/// Round-robin: wake due sleepers, pick the next Ready task, switch.
/// Runs in the IRQ0 handler; the switch unwinds inside the next task's own
/// interrupt frame, so the ISR epilogue iretqs back into the right task.
pub fn schedule() {
    let now = timer::ticks();
    let cur = CURRENT.load(Ordering::Relaxed);
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
        let old_rsp = &raw mut TASKS[cur].rsp;
        let new_rsp = TASKS[n].rsp;
        switch_context(old_rsp, new_rsp);
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
