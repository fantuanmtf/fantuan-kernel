//! x86_64 task glue (M9.2c): the scheduler lives in kernel-core::task; this
//! module installs the arch ops, provides the kernel-entry trampoline and
//! the ELF user-task spawner (ring 3 iretq frame).

use crate::elf;
use crate::gdt;
use crate::mm::frame;
use crate::mm::paging::{self, phys_to_virt};
use crate::mm::user;
use fantuan_abi::{USER_CS_SEL, USER_DS_SEL, USER_STACK_TOP};

pub use kernel_core::task::{current_id, exit, init, schedule, sleep_ms, spawn, SWITCHES};

const USER_STACK_PAGES: u64 = 4;

extern "C" {
    fn switch_context(old_rsp: *mut u64, new_rsp: u64, new_cr3: u64);
    fn user_entry();
}

fn arch_switch(old: *mut u64, new_rsp: u64, new_vm: u64) {
    unsafe { switch_context(old, new_rsp, new_vm) }
}

fn arch_set_kernel_stack(top: u64) {
    gdt::set_rsp0(top);
}

/// Initial kernel frame: [r15..rbx zeros][task_entry] <- rsp.
fn arch_init_kernel_stack(stack_top: u64, _body: fn() -> !) -> u64 {
    let sp = (stack_top - 7 * 8) as *mut u64;
    unsafe {
        for i in 0..6 {
            *sp.add(i) = 0;
        }
        *sp.add(6) = task_entry as *const () as u64;
    }
    sp as u64
}

extern "C" fn task_entry() -> ! {
    (kernel_core::task::current_body())()
}

fn arch_kernel_vm_root() -> u64 {
    paging::kernel_pml4()
}

fn arch_free_user_vm(vm: u64) {
    user::free_user_pml4(vm);
}

fn arch_phys_to_virt(p: u64) -> u64 {
    phys_to_virt(p)
}

fn arch_now_ticks() -> u64 {
    crate::timer::ticks()
}

fn arch_on_reap(tid: u64) {
    use core::fmt::Write;
    let mut s = crate::serial::Serial::new(crate::serial::COM1);
    let _ = writeln!(s, "sched: reaped tid {} (kernel stack + user pages freed)", tid);
}

/// Install the arch ops; call once before task::init.
pub fn init_arch() {
    kernel_core::task::set_ops(kernel_core::task::TaskOps {
        switch: arch_switch,
        set_kernel_stack: arch_set_kernel_stack,
        init_kernel_stack: arch_init_kernel_stack,
        kernel_vm_root: arch_kernel_vm_root,
        free_user_vm: arch_free_user_vm,
        phys_to_virt: arch_phys_to_virt,
        now_ticks: arch_now_ticks,
        on_reap: arch_on_reap,
    });
}

/// Spawn a user task from a static ELF image (M4).
pub fn spawn_user(elf_image: &[u8]) -> Option<u64> {
    use kernel_core::task::{alloc_kernel_stack, dead_body, has_free_slot, register, State, Task};

    if !has_free_slot() {
        crate::serial::line("user: no free task slot");
        return None;
    }
    let flags = crate::cpu::irq_save();

    let Some((entry, cr3)) = elf::load(elf_image) else {
        crate::cpu::irq_restore(flags);
        return None;
    };

    // User stack: contiguous frames mapped at USER_STACK_TOP - 16 KiB.
    let Some(ustack_phys) = frame::get().alloc_contiguous(USER_STACK_PAGES as usize) else {
        crate::cpu::irq_restore(flags);
        return None;
    };
    let ustack_base = USER_STACK_TOP - USER_STACK_PAGES * frame::FRAME_SIZE;
    for i in 0..USER_STACK_PAGES {
        user::map_page(
            cr3,
            ustack_base + i * frame::FRAME_SIZE,
            ustack_phys + i * frame::FRAME_SIZE,
            user::P_PRESENT | user::P_WRITABLE | user::P_USER | user::P_NX,
        );
    }

    // Kernel stack + the ring-3 iretq frame: six saved-register zeros, then
    // user_entry as the ret target, then [rip][cs][rflags][rsp][ss].
    let Some((stack_phys, stack_top)) = alloc_kernel_stack() else {
        for i in 0..USER_STACK_PAGES {
            frame::get().free(ustack_phys + i * frame::FRAME_SIZE);
        }
        crate::cpu::irq_restore(flags);
        return None;
    };
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
    }
    let id = register(Task {
        state: State::Ready,
        rsp: sp as u64,
        vm_root: cr3,
        kernel_stack_top: stack_top,
        is_user: true,
        stack_phys,
        body: dead_body,
        id: 0,
        exit_code: 0,
    });
    crate::cpu::irq_restore(flags);
    id
}
