//! x86_64 task glue (M9.2c): the scheduler lives in kernel-core::task; this
//! module installs the arch ops, provides the kernel-entry trampolines, the
//! ELF user-task spawner (ring 3 iretq frame) and the P2 fork/exec image
//! builders (docs/POSIX_PLAN.md).

use kernel_core::elf;
use kernel_core::process::UserContext;
use crate::gdt;
use crate::mm::frame;
use crate::mm::paging::{self, phys_to_virt};
use crate::mm::user;
use fantuan_abi::{USER_CS_SEL, USER_DS_SEL, USER_HEAP_BASE, USER_STACK_TOP};

pub use kernel_core::task::{current_id, exit, init, schedule, spawn, SWITCHES};

const USER_STACK_PAGES: u64 = 4;

extern "C" {
    fn switch_context(old_rsp: *mut u64, new_rsp: u64, new_cr3: u64);
    fn user_iret_entry();
}

fn arch_switch(old: *mut u64, new_rsp: u64, new_vm: u64) {
    unsafe { switch_context(old, new_rsp, new_vm) }
}

fn arch_set_kernel_stack(top: u64, _is_user: bool) {
    gdt::set_rsp0(top);
}

/// Initial kernel frame: [r15..rbx zeros][rflags][task_entry] <- rsp.
fn arch_init_kernel_stack(stack_top: u64, _body: fn() -> !) -> u64 {
    let sp = (stack_top - 8 * 8) as *mut u64;
    unsafe {
        for i in 0..6 {
            *sp.add(i) = 0;
        }
        *sp.add(6) = 0x202; // IF set: kernel tasks run with interrupts on
        *sp.add(7) = task_entry as *const () as u64;
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

fn arch_user_map(root: u64, va: u64, pa: u64, prot: kernel_core::user::Prot) {
    let mut flags = user::P_PRESENT | user::P_USER;
    if prot.write() {
        flags |= user::P_WRITABLE;
    }
    if !prot.exec() {
        flags |= user::P_NX;
    }
    user::map_page(root, va, pa, flags);
}

fn arch_clone_root(src: u64) -> Option<u64> {
    user::clone_user_pml4(src)
}

fn arch_unmap(root: u64, va: u64) {
    user::unmap_page(root, va);
}

fn arch_protect(root: u64, va: u64, prot: kernel_core::user::Prot) {
    user::protect_page(root, va, prot);
}

fn arch_log(s: &str) {
    crate::serial::line(s);
}

fn arch_on_reap(tid: u64) {
    use core::fmt::Write;
    let mut s = crate::serial::Serial::new(crate::serial::COM1);
    let _ = writeln!(s, "sched: reaped tid {} (kernel stack + user pages freed)", tid);
}

// --- P2 fork/exec stack machinery ------------------------------------------

/// Build a kernel stack that restores the full user context from CTX and
/// iretqs to ring 3 (used by fork and spawn). Layout:
/// `[switch: r15..rbx][rflags][user_iret_entry][GP block: r15..rax][rip][cs]
///  [rflags][rsp][ss]` — user_iret_entry pops the GP block then iretqs.
fn build_user_kernel_frame(stack_top: u64, ctx: &UserContext) -> u64 {
    let sp = (stack_top - (7 + 1 + 20) * 8) as *mut u64;
    let g = [
        ctx.r15, ctx.r14, ctx.r13, ctx.r12, ctx.r11, ctx.r10, ctx.r9, ctx.r8, ctx.rbp,
        ctx.rdi, ctx.rsi, ctx.rdx, ctx.rcx, ctx.rbx, ctx.rax,
    ];
    unsafe {
        for i in 0..7 {
            *sp.add(i) = 0;
        }
        *sp.add(6) = 0x202;
        *sp.add(7) = user_iret_entry as *const () as u64;
        let gp = sp.add(8);
        for (i, v) in g.iter().enumerate() {
            *gp.add(i) = *v;
        }
        *gp.add(15) = ctx.rip;
        *gp.add(16) = USER_CS_SEL as u64;
        *gp.add(17) = ctx.rflags;
        *gp.add(18) = ctx.rsp;
        *gp.add(19) = USER_DS_SEL as u64;
    }
    sp as u64
}

/// Fork child: same code, RAX = 0, same user stack contents (eager copy).
fn arch_init_fork_stack(ctx: &UserContext) -> Option<(u64, u64, u64)> {
    let (stack_phys, stack_top) = kernel_core::task::alloc_kernel_stack()?;
    let rsp = build_user_kernel_frame(stack_top, ctx);
    Some((stack_phys, stack_top, rsp))
}

/// Map the SysV user stack and write `[argc][argv..][NULL][envp..][NULL]`.
/// Returns (stack_phys, initial RSP).
fn map_user_stack(cr3: u64, argv: &[&[u8]], envp: &[&[u8]]) -> Option<(u64, u64)> {
    let ustack_phys = frame::get().alloc_contiguous(USER_STACK_PAGES as usize)?;
    let ustack_base = USER_STACK_TOP - USER_STACK_PAGES * frame::FRAME_SIZE;
    for i in 0..USER_STACK_PAGES {
        let f = ustack_phys + i * frame::FRAME_SIZE;
        user::map_page(
            cr3,
            ustack_base + i * frame::FRAME_SIZE,
            f,
            user::P_PRESENT | user::P_WRITABLE | user::P_USER | user::P_NX,
        );
        let dst = phys_to_virt(f) as *mut u8;
        unsafe { core::ptr::write_bytes(dst, 0, 4096) };
    }
    let base = ustack_base;
    let mut pos = (USER_STACK_PAGES * frame::FRAME_SIZE) as usize;
    let mut ptrs = [0u64; 40];
    let mut n = 0;
    for s in argv.iter().chain(envp.iter()) {
        if n >= ptrs.len() {
            break;
        }
        pos -= s.len() + 1;
        stack_write(ustack_phys, pos, s);
        stack_write(ustack_phys, pos + s.len(), &[0]);
        ptrs[n] = base + pos as u64;
        n += 1;
    }
    let argc = argv.len().min(ptrs.len());
    let envc = n - argc;
    pos &= !15;
    pos -= 8;
    stack_write(ustack_phys, pos, &0u64.to_le_bytes()); // envp NULL
    for i in (argc..argc + envc).rev() {
        pos -= 8;
        stack_write(ustack_phys, pos, &ptrs[i].to_le_bytes());
    }
    pos -= 8;
    stack_write(ustack_phys, pos, &0u64.to_le_bytes()); // argv NULL
    for i in (0..argc).rev() {
        pos -= 8;
        stack_write(ustack_phys, pos, &ptrs[i].to_le_bytes());
    }
    pos -= 8;
    stack_write(ustack_phys, pos, &(argc as u64).to_le_bytes());
    Some((ustack_phys, base + pos as u64))
}

fn stack_write(ustack_phys: u64, off: usize, bytes: &[u8]) {
    let p = phys_to_virt(ustack_phys) as *mut u8;
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), p.add(off), bytes.len()) };
}

/// execve: load ELF, fresh stack with argv/envp, derived heap base.
fn arch_exec_image(
    image: &[u8],
    argv: &[&[u8]],
    envp: &[&[u8]],
) -> Option<(u64, u64, u64, u64)> {
    let (entry, root, image_end) = elf::load_full(image)?;
    let heap_base = image_end.max(USER_HEAP_BASE);
    let Some((_stack, user_rsp)) = map_user_stack(root, argv, envp) else {
        user::free_user_pml4(root);
        return None;
    };
    Some((entry, user_rsp, root, heap_base))
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
        init_fork_stack: arch_init_fork_stack,
        exec_image: arch_exec_image,
    });
    kernel_core::user::set_ops(kernel_core::user::UserOps {
        machine: 0x3E, // x86_64
        new_root: user::new_user_pml4,
        map: arch_user_map,
        free_root: user::free_user_pml4,
        phys_to_virt,
        log: arch_log,
        clone_root: arch_clone_root,
        unmap: arch_unmap,
        protect: arch_protect,
    });
}

/// Spawn a user task from a static ELF image (M4).
pub fn spawn_user(elf_image: &[u8]) -> Option<u64> {
    spawn_user_args(elf_image, &[])
}

/// Spawn a user task with argv (P1); the stack layout is the SysV one.
pub fn spawn_user_args(elf_image: &[u8], args: &[&[u8]]) -> Option<u64> {
    spawn_user_env(elf_image, args, &[])
}

/// P2 spawn with an environment (the kernel shell's `sh` command).
pub fn spawn_user_env(elf_image: &[u8], args: &[&[u8]], env: &[&[u8]]) -> Option<u64> {
    use kernel_core::task::{alloc_kernel_stack, dead_body, has_free_slot, register, State, Task};

    if !has_free_slot() {
        crate::serial::line("user: no free task slot");
        return None;
    }
    let flags = crate::cpu::irq_save();

    let Some((entry, cr3, image_end)) = elf::load_full(elf_image) else {
        crate::cpu::irq_restore(flags);
        return None;
    };
    let Some((ustack_phys, user_rsp)) = map_user_stack(cr3, args, env) else {
        user::free_user_pml4(cr3);
        crate::cpu::irq_restore(flags);
        return None;
    };

    let Some((stack_phys, stack_top)) = alloc_kernel_stack() else {
        for i in 0..USER_STACK_PAGES {
            frame::get().free(ustack_phys + i * frame::FRAME_SIZE);
        }
        crate::cpu::irq_restore(flags);
        return None;
    };
    let ctx = UserContext { rip: entry, rsp: user_rsp, rflags: 0x202, ..Default::default() };
    let rsp = build_user_kernel_frame(stack_top, &ctx);
    let id = register(Task {
        state: State::Ready,
        rsp,
        vm_root: cr3,
        kernel_stack_top: stack_top,
        is_user: true,
        stack_phys,
        body: dead_body,
        id: 0,
        exit_code: 0,
        heap_base: image_end.max(USER_HEAP_BASE),
    });
    crate::cpu::irq_restore(flags);
    id
}

/// Kernel shell helper: wait for a child without being a user task.
pub fn wait_for(child: u64) -> u64 {
    kernel_core::process::wait4(child as i64, 0).map(|(_, st)| st).unwrap_or(u64::MAX)
}
