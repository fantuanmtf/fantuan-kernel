//! aarch64 kernel-task glue (M11 R9a): the context switch on the full
//! callee-saved set (x19..x30 plus the low 128 bits of v8..v15, which the
//! optimized integer code may use through NEON) and DAIF, so a task carries
//! its own interrupt mask across switches (the aarch64 counterpart of the
//! x86 RFLAGS fix from R7). There is no user mode in R9a, so UserOps is
//! unused and the address space never changes: the switch ignores vm_root.

use core::arch::global_asm;

use crate::{mmu, put_dec, puts};

/// Context bytes pushed on the task stack: v8..v15 (8 x 16) + x19..x30
/// (12 x 8) + DAIF = 232, padded to 240. Keep in sync with the assembly.
const CTX_BYTES: u64 = 240;
/// u64 index of x30 (the switch return address) in the saved frame.
const CTX_LR_IDX: usize = 27;
/// u64 index of the saved DAIF value.
const CTX_DAIF_IDX: usize = 28;

global_asm!(
    ".section .text",
    ".global aarch64_switch",
    ".type aarch64_switch, @function",
    "aarch64_switch:", // x0 = &old_sp, x1 = new_sp
    "    sub sp, sp, #240",
    "    stp q8, q9, [sp, #0]",
    "    stp q10, q11, [sp, #32]",
    "    stp q12, q13, [sp, #64]",
    "    stp q14, q15, [sp, #96]",
    "    stp x19, x20, [sp, #128]",
    "    stp x21, x22, [sp, #144]",
    "    stp x23, x24, [sp, #160]",
    "    stp x25, x26, [sp, #176]",
    "    stp x27, x28, [sp, #192]",
    "    stp x29, x30, [sp, #208]",
    "    mrs x2, daif",
    "    str x2, [sp, #224]",
    "    mov x2, sp",
    "    str x2, [x0]",
    "    mov sp, x1",
    "    ldp q8, q9, [sp, #0]",
    "    ldp q10, q11, [sp, #32]",
    "    ldp q12, q13, [sp, #64]",
    "    ldp q14, q15, [sp, #96]",
    "    ldp x19, x20, [sp, #128]",
    "    ldp x21, x22, [sp, #144]",
    "    ldp x23, x24, [sp, #160]",
    "    ldp x25, x26, [sp, #176]",
    "    ldp x27, x28, [sp, #192]",
    "    ldp x29, x30, [sp, #208]",
    "    ldr x2, [sp, #224]",
    "    add sp, sp, #240",
    "    msr daif, x2",
    "    ret",
);

extern "C" {
    fn aarch64_switch(old: *mut u64, new_sp: u64);
}

fn arch_switch(old: *mut u64, new_sp: u64, _new_vm: u64) {
    // One address space (MMU roots are global): no TTBR switch needed.
    unsafe { aarch64_switch(old, new_sp) }
}

fn arch_set_kernel_stack(_top: u64, _is_user: bool) {
    // Exceptions from EL1 keep using SP_EL1, so the kernel-entry stack is
    // simply the task's own SP; nothing to install until EL0 exists (R9b).
}

fn arch_init_kernel_stack(stack_top: u64, _body: fn() -> !) -> u64 {
    let frame = (stack_top - CTX_BYTES) as *mut u64;
    unsafe {
        for i in 0..(CTX_BYTES / 8) as usize {
            *frame.add(i) = 0;
        }
        *frame.add(CTX_LR_IDX) = task_entry as *const () as u64;
        // DAIF = 0: a fresh task runs with interrupts enabled (the x86
        // initial frame's IF=1 equivalent).
        *frame.add(CTX_DAIF_IDX) = 0;
    }
    frame as u64
}

extern "C" fn task_entry() -> ! {
    (kernel_core::task::current_body())()
}

fn arch_kernel_vm_root() -> u64 {
    mmu::kernel_root()
}

fn arch_free_user_vm(_vm: u64) {
    // No user address spaces in R9a.
}

fn arch_phys_to_virt(p: u64) -> u64 {
    mmu::phys_to_virt(p)
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
fn unset_fork_stack(_ctx: &kernel_core::process::UserContext) -> Option<(u64, u64, u64)> {
    None
}

fn unset_exec_image(
    _e: &[u8],
    _a: &[&[u8]],
    _v: &[&[u8]],
) -> Option<(u64, u64, u64, u64)> {
    None
}

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
        init_fork_stack: unset_fork_stack,
        exec_image: unset_exec_image,
    });
}
