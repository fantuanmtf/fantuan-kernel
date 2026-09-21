//! riscv64 task glue (M9.2c; per-task Sv39 roots and U-mode entry M9.3c):
//! context switch on the callee-saved integer registers, satp switching and
//! the TaskOps/UserOps the shared scheduler and ELF loader need.

use core::arch::{asm, global_asm};

use fantuan_abi::USER_STACK_TOP;
use kernel_core::elf;
use kernel_core::user::Prot;

use crate::{cpu, paging, put_dec, puts};

/// Context bytes pushed on the task stack: ra + s0..s11 (12 slots) plus
/// alignment. Keep in sync with the assembly.
const CTX_BYTES: u64 = 112;
const USER_STACK_PAGES: u64 = 4;

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
    // First entry of a user task: s0 = entry, s1 = user stack top, s2 = satp
    // value for the task root. The kernel stack top sits in sscratch and sp;
    // enter the user root, then sret drops to U-mode.
    ".global riscv_user_entry",
    ".type riscv_user_entry, @function",
    "riscv_user_entry:",
    "    csrw    sepc, s0",
    "    mv      sp, s1",
    "    csrw    satp, s2",
    "    sfence.vma",
    "    li      t0, 0x20", // sstatus: SPIE=1, SPP=0
    "    csrw    sstatus, t0",
    "    sret",
);

extern "C" {
    fn riscv_switch(old: *mut u64, new: *const u64);
    fn riscv_user_entry();
}

fn arch_switch(old: *mut u64, new_sp: u64, _new_vm: u64) {
    // satp is not switched here: a context switch always happens in kernel
    // code (kernel root); the per-task root is entered only on the way out
    // to U-mode (riscv_user_entry / trap exit).
    unsafe { riscv_switch(old, new_sp as *const u64) }
}

fn arch_set_kernel_stack(top: u64, is_user: bool) {
    // U-mode traps swap sp with sscratch, so user tasks keep the kernel stack
    // top there; kernel tasks keep 0 (the swap is skipped).
    let value = if is_user { top } else { 0 };
    unsafe { asm!("csrw sscratch, {}", in(reg) value, options(nostack)) };
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
    paging::kernel_root()
}

fn arch_free_user_vm(vm: u64) {
    paging::free_user_root(vm);
}

fn arch_phys_to_virt(p: u64) -> u64 {
    paging::phys_to_virt(p)
}

fn arch_now_ticks() -> u64 {
    crate::timer::ticks()
}

fn arch_user_map(root: u64, va: u64, pa: u64, prot: Prot) {
    paging::map_user_page(root, va, pa, prot);
}

fn arch_log(s: &str) {
    puts(s);
    puts("\n");
}

fn arch_on_reap(tid: u64) {
    puts("sched: reaped tid ");
    put_dec(tid);
    puts(" (kernel stack + user pages freed)\n");
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

fn unset_clone_root(_src: u64) -> Option<u64> {
    None
}

fn unset_unmap_page(_root: u64, _va: u64) {}

fn unset_protect_page(_root: u64, _va: u64, _prot: kernel_core::user::Prot) {}

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
    kernel_core::user::set_ops(kernel_core::user::UserOps {
        machine: 0xF3, // riscv
        new_root: paging::new_user_root,
        map: arch_user_map,
        free_root: paging::free_user_root,
        phys_to_virt: arch_phys_to_virt,
        log: arch_log,
        clone_root: unset_clone_root,
        unmap: unset_unmap_page,
        protect: unset_protect_page,
    });
}

/// Spawn a user task from a static ELF image (M9.3c).
pub fn spawn_user(elf_image: &[u8]) -> Option<u64> {
    use kernel_core::task::{alloc_kernel_stack, dead_body, has_free_slot, register, State, Task};

    if !has_free_slot() {
        puts("user: no free task slot\n");
        return None;
    }
    let flags = cpu::irq_save();
    let Some((entry, root)) = elf::load(elf_image) else {
        cpu::irq_restore(flags);
        return None;
    };
    // User stack: contiguous frames mapped at USER_STACK_TOP - 16 KiB, RW.
    let Some(ustack_phys) = kernel_core::frame::get().alloc_contiguous(USER_STACK_PAGES as usize)
    else {
        paging::free_user_root(root);
        cpu::irq_restore(flags);
        return None;
    };
    let ustack_base = USER_STACK_TOP - USER_STACK_PAGES * 4096;
    for i in 0..USER_STACK_PAGES {
        paging::map_user_page(
            root,
            ustack_base + i * 4096,
            ustack_phys + i * 4096,
            Prot::Rw,
        );
    }

    let Some((stack_phys, stack_top)) = alloc_kernel_stack() else {
        for i in 0..USER_STACK_PAGES {
            kernel_core::frame::get().free(ustack_phys + i * 4096);
        }
        paging::free_user_root(root);
        cpu::irq_restore(flags);
        return None;
    };
    // Initial kernel frame: ra = riscv_user_entry, s0 = entry, s1 = user sp.
    let frame = (stack_top - CTX_BYTES) as *mut u64;
    unsafe {
        for i in 0..(CTX_BYTES / 8) as usize {
            *frame.add(i) = 0;
        }
        *frame.add(0) = riscv_user_entry as *const () as u64;
        *frame.add(1) = entry;
        *frame.add(2) = USER_STACK_TOP;
        *frame.add(3) = paging::satp_for(root); // s2
    }
    let id = register(Task {
        state: State::Ready,
        rsp: frame as u64,
        vm_root: root,
        kernel_stack_top: stack_top,
        is_user: true,
        stack_phys,
        body: dead_body,
        id: 0,
        exit_code: 0,
        heap_base: 0,
    });
    cpu::irq_restore(flags);
    id
}
