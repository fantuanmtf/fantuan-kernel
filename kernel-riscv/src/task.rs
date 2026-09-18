//! riscv64 task glue (M9.2c): context switch on the callee-saved integer
//! registers and the TaskOps the shared scheduler needs. Per-task address
//! spaces (satp) arrive with user mode in M9.3.

use core::arch::{asm, global_asm};

use crate::{paging, put_dec, puts};

/// Context bytes pushed on the task stack: ra + s0..s11 (12 slots) plus
/// alignment. Keep in sync with the assembly.
const CTX_BYTES: u64 = 112;

global_asm!(
    ".section .text",
    ".global riscv_switch",
    ".type riscv_switch, @function",
    "riscv_switch:", // a0 = &old_sp, a1 = new_sp
    // The context lives on the task's own stack, like the x86 switch: push
    // the callee-saved registers below sp, publish the new sp, restore from
    // the incoming stack.
    "    addi    sp, sp, -112",
    "    sd      ra, 0(sp)",
    "    sd      s0, 8(sp)",
    "    sd      s1, 16(sp)",
    "    sd      s2, 24(sp)",
    "    sd      s3, 32(sp)",
    "    sd      s4, 40(sp)",
    "    sd      s5, 48(sp)",
    "    sd      s6, 56(sp)",
    "    sd      s7, 64(sp)",
    "    sd      s8, 72(sp)",
    "    sd      s9, 80(sp)",
    "    sd      s10, 88(sp)",
    "    sd      s11, 96(sp)",
    "    sd      sp, 0(a0)",
    "    mv      sp, a1",
    "    ld      ra, 0(sp)",
    "    ld      s0, 8(sp)",
    "    ld      s1, 16(sp)",
    "    ld      s2, 24(sp)",
    "    ld      s3, 32(sp)",
    "    ld      s4, 40(sp)",
    "    ld      s5, 48(sp)",
    "    ld      s6, 56(sp)",
    "    ld      s7, 64(sp)",
    "    ld      s8, 72(sp)",
    "    ld      s9, 80(sp)",
    "    ld      s10, 88(sp)",
    "    ld      s11, 96(sp)",
    "    addi    sp, sp, 112",
    "    ret",
);

extern "C" {
    fn riscv_switch(old: *mut u64, new: *const u64);
}

fn arch_switch(old: *mut u64, new_sp: u64, _new_vm: u64) {
    unsafe { riscv_switch(old, new_sp as *const u64) }
}

fn arch_set_kernel_stack(top: u64) {
    // Kernel-entry stack for future U-mode traps (M9.3) and for symmetry
    // with the x86 TSS.rsp0 handling.
    unsafe { asm!("csrw sscratch, {}", in(reg) top, options(nostack)) };
}

fn arch_init_kernel_stack(stack_top: u64, _body: fn() -> !) -> u64 {
    let frame = (stack_top - CTX_BYTES) as *mut u64;
    unsafe {
        for i in 0..(CTX_BYTES / 8) as usize {
            *frame.add(i) = 0;
        }
        *frame.add(0) = riscv_task_entry as *const () as u64; // ra
    }
    frame as u64
}

extern "C" fn riscv_task_entry() -> ! {
    (kernel_core::task::current_body())()
}

fn arch_kernel_vm_root() -> u64 {
    0 // riscv tasks share the kernel address space until M9.3
}

fn arch_free_user_vm(_vm: u64) {
    // No per-task address spaces yet (M9.3 adds the Sv39 walk).
}

fn arch_phys_to_virt(p: u64) -> u64 {
    paging::phys_to_virt(p)
}

fn arch_now_ticks() -> u64 {
    crate::timer::ticks()
}

fn arch_on_reap(tid: u64) {
    puts("sched: reaped tid ");
    put_dec(tid);
    puts(" (kernel stack freed)\n");
}

/// Install the ops; call before kernel_core::task::init.
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
