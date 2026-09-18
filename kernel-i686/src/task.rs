//! i686 task glue (M10-4b2b): wired and verified. The earlier "stack leak"
//! was not a target-ABI problem: the shared ISR stub restored registers
//! from the wrong stack offset (`add esp, 8` before `popad`), corrupting
//! every GPR on exception/IRQ return. With that fixed the 32-bit scheduler
//! rotates the demo tasks next to M10-4b2a.
//!
//! The 32-bit context switch and the TaskOps the shared scheduler needs.
//! Per-task address spaces and the TSS arrive with ring 3 (M10-4b3); tasks
//! share the stage2 page directory, so vm_root stays 0.

use crate::serial::{put_dec, puts};
use crate::{demo, pit};

extern "C" {
    fn context_switch(old_rsp: *mut u64, new_sp: u64, new_cr3: u64);
}

/// Kernel-core keeps addresses in u64; the 32-bit context switch works with
/// the low 32 bits (all kernel memory is below 4 GiB here).
fn arch_switch(old: *mut u64, new_sp: u64, _new_vm: u64) {
    unsafe { context_switch(old, new_sp, 0) }
}

fn arch_set_kernel_stack(_top: u64, _is_user: bool) {
    // No TSS yet: ring-3 entry (M10-4b3) installs one for the kernel stack.
}

/// Initial frame: [edi][esi][ebx][ebp][task_entry] <- saved esp.
fn arch_init_kernel_stack(stack_top: u64, _body: fn() -> !) -> u64 {
    let sp = (stack_top - 5 * 4) as *mut u32;
    unsafe {
        for i in 0..4 {
            *sp.add(i) = 0;
        }
        *sp.add(4) = task_entry as *const () as u32;
    }
    sp as u64
}

extern "C" fn task_entry() -> ! {
    (kernel_core::task::current_body())()
}

fn arch_kernel_vm_root() -> u64 {
    0 // shared address space (stage2 page directory)
}

fn arch_free_user_vm(_vm: u64) {}

fn arch_now_ticks() -> u64 {
    pit::ticks()
}

fn arch_on_reap(tid: u64) {
    puts("sched: reaped tid ");
    put_dec(tid);
    puts("\\n");
}

/// Install the ops; call before kernel_core::task::init.
pub fn init_arch() {
    kernel_core::task::set_ops(kernel_core::task::TaskOps {
        switch: arch_switch,
        set_kernel_stack: arch_set_kernel_stack,
        init_kernel_stack: arch_init_kernel_stack,
        kernel_vm_root: arch_kernel_vm_root,
        free_user_vm: arch_free_user_vm,
        phys_to_virt: crate::phys_to_virt,
        now_ticks: arch_now_ticks,
        on_reap: arch_on_reap,
    });
}

/// Spawn the two scheduler demo tasks (they print a little, then sleep).
pub fn spawn_demos() {
    kernel_core::task::spawn(demo::demo_1);
    kernel_core::task::spawn(demo::demo_2);
}
