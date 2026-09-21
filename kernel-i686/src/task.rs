//! i686 task glue (M10-4b2b): wired and verified. The earlier "stack leak"
//! was not a target-ABI problem: the shared ISR stub restored registers
//! from the wrong stack offset (`add esp, 8` before `popad`), corrupting
//! every GPR on exception/IRQ return. With that fixed the 32-bit scheduler
//! rotates the demo tasks next to M10-4b2a.
//!
//! The 32-bit context switch and the TaskOps the shared scheduler needs.
//! Kernel tasks carry the stage2 page directory as their vm_root; user tasks
//! carry a per-task PD (M10-4b3b), and the context switch reloads CR3.

use crate::serial::{put_dec, puts};
use crate::{demo, pit};

extern "C" {
    // u32 args: on i386 cdecl a u64 arg occupies two stack slots, which would
    // shift the 32-bit offsets context.S reads (a silent "keep CR3").
    fn context_switch(old_rsp: *mut u64, new_sp: u32, new_cr3: u32);
}

/// The stage2 page directory CR3 pointed at on entry: the kernel's own
/// address space, used for kernel tasks and as the clone source for user PDs.
static mut KERNEL_CR3: u32 = 0;

/// Kernel-core keeps addresses in u64; all i686 kernel and user addresses are
/// below 4 GiB, so the low 32 bits are the value. Zero means "keep the current
/// CR3" in context.S, so kernel tasks carry KERNEL_CR3 instead.
fn arch_switch(old: *mut u64, new_sp: u64, new_vm: u64) {
    unsafe { context_switch(old, new_sp as u32, new_vm as u32) }
}

fn arch_set_kernel_stack(top: u64, _is_user: bool) {
    // TSS.esp0: the stack the CPU switches to on ring-3 -> ring-0 traps.
    crate::gdt::set_kernel_stack(top);
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
    unsafe { KERNEL_CR3 as u64 }
}

fn arch_free_user_vm(vm: u64) {
    crate::user::free_root(vm);
}

fn arch_now_ticks() -> u64 {
    pit::ticks()
}

fn arch_on_reap(tid: u64) {
    puts("sched: reaped tid ");
    put_dec(tid);
    puts("\n");
}

fn arch_log(s: &str) {
    puts(s);
    puts("\n");
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
    let cr3: u32;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack)) };
    unsafe { KERNEL_CR3 = cr3 };
    kernel_core::task::set_ops(kernel_core::task::TaskOps {
        switch: arch_switch,
        set_kernel_stack: arch_set_kernel_stack,
        init_kernel_stack: arch_init_kernel_stack,
        kernel_vm_root: arch_kernel_vm_root,
        free_user_vm: arch_free_user_vm,
        phys_to_virt: crate::phys_to_virt,
        now_ticks: arch_now_ticks,
        on_reap: arch_on_reap,
        init_fork_stack: unset_fork_stack,
        exec_image: unset_exec_image,
    });
    kernel_core::user::set_ops(kernel_core::user::UserOps {
        machine: 0x03, // EM_386: ELFCLASS32 user images
        new_root: crate::user::new_root,
        map: crate::user::map,
        free_root: crate::user::free_root,
        phys_to_virt: crate::phys_to_virt,
        log: arch_log,
        clone_root: unset_clone_root,
        unmap: unset_unmap_page,
        protect: unset_protect_page,
    });
}

/// Spawn the two scheduler demo tasks (they print a little, then sleep).
pub fn spawn_demos() {
    kernel_core::task::spawn(demo::demo_1);
    kernel_core::task::spawn(demo::demo_2);
}
