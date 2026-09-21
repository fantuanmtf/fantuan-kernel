//! P2 signal delivery (docs/POSIX_PLAN.md): the kernel-side half of the
//! sigaction/sigprocmask/sigreturn trio. The process tables live in
//! `process`; this file owns `deliver_current`, which runs at every return to
//! ring 3 (syscall exit and timer tick) and either invokes the default
//! action or builds the user-stack trampoline:
//!
//! ```text
//! [restorer return address][SignalFrame ...]   <- ctx.rsp (8 mod 16)
//! ```
//!
//! The handler returns through the libc restorer, which issues
//! SYS_SIGRETURN and restores the interrupted context verbatim.

use fantuan_abi::{
    NSIG, SIGKILL, SYS_ERR_INVAL, SYS_ERR_NOSYS, SYS_OK, SignalFrame,
};

use crate::process::{self, FrameOps, UserContext};
use crate::task;
use crate::user;

fn frame_ops() -> FrameOps {
    process::frame_ops()
}

/// Queue a signal on the current task and deliver immediately when possible.
pub fn raise_current(sig: u32) -> u64 {
    process::queue_slot(task::current_slot(), sig);
    deliver_current();
    SYS_OK
}

/// Deliver pending signals at a ring-3 boundary. Returns after one handler
/// is entered (or a default action ran); callers loop on the next boundary.
pub fn deliver_current() {
    let ops = frame_ops();
    let Some(mut ctx) = (ops.get)() else { return };
    let slot = task::current_slot();
    let Some(sig) = process::dequeue(slot) else { return };
    if sig == SIGKILL {
        task::exit_signal(SIGKILL);
    }
    let act = process::action_of(slot, sig);
    if act.handler == fantuan_abi::SIG_IGN {
        return;
    }
    if act.handler == fantuan_abi::SIG_DFL {
        if process::default_ignored(sig) {
            return;
        }
        task::exit_signal(sig);
    }
    if act.restorer == 0 {
        // No trampoline: the handler could never return; treat as fatal.
        task::exit_signal(sig);
    }
    let saved_mask = process::blocked_of(slot);
    let frame = SignalFrame {
        r15: ctx.r15,
        r14: ctx.r14,
        r13: ctx.r13,
        r12: ctx.r12,
        r11: ctx.r11,
        r10: ctx.r10,
        r9: ctx.r9,
        r8: ctx.r8,
        rbp: ctx.rbp,
        rdi: ctx.rdi,
        rsi: ctx.rsi,
        rdx: ctx.rdx,
        rcx: ctx.rcx,
        rbx: ctx.rbx,
        rax: ctx.rax,
        rip: ctx.rip,
        rflags: ctx.rflags,
        rsp: ctx.rsp,
        signum: sig as u64,
        mask: saved_mask as u64,
    };
    let size = core::mem::size_of::<SignalFrame>() as u64;
    let mut sp = ctx.rsp.wrapping_sub(size + 8) & !15u64;
    sp = sp.wrapping_sub(8); // handler entry sees rsp % 16 == 8, like `call`
    let bytes = unsafe {
        core::slice::from_raw_parts(&frame as *const SignalFrame as *const u8, size as usize)
    };
    if user::copy_out(sp + 8, bytes).is_none() {
        task::exit_signal(fantuan_abi::SIGSEGV);
    }
    let restorer = act.restorer.to_le_bytes();
    if user::copy_out(sp, &restorer).is_none() {
        task::exit_signal(fantuan_abi::SIGSEGV);
    }
    ctx.rdi = sig as u64;
    ctx.rip = act.handler;
    ctx.rsp = sp;
    process::block_for_handler(slot, sig, act.mask);
    (ops.set)(&ctx);
}

/// SYS_SIGRETURN: rebuild the interrupted context from the frame the
/// restorer left on the user stack (ctx.rsp points at the SignalFrame).
pub fn sigreturn() -> u64 {
    let ops = frame_ops();
    let Some(ctx) = (ops.get)() else { return SYS_ERR_NOSYS };
    let mut bytes = [0u8; core::mem::size_of::<SignalFrame>()];
    if user::copy_in(&mut bytes, ctx.rsp).is_none() {
        task::exit_signal(fantuan_abi::SIGSEGV);
    }
    let frame: SignalFrame = unsafe { core::ptr::read(bytes.as_ptr() as *const SignalFrame) };
    let slot = task::current_slot();
    process::set_blocked(slot, frame.mask as u32);
    let restored = UserContext {
        r15: frame.r15,
        r14: frame.r14,
        r13: frame.r13,
        r12: frame.r12,
        r11: frame.r11,
        r10: frame.r10,
        r9: frame.r9,
        r8: frame.r8,
        rbp: frame.rbp,
        rdi: frame.rdi,
        rsi: frame.rsi,
        rdx: frame.rdx,
        rcx: frame.rcx,
        rbx: frame.rbx,
        rax: frame.rax,
        rip: frame.rip,
        rflags: frame.rflags,
        rsp: frame.rsp,
    };
    (ops.set)(&restored);
    SYS_OK
}

/// SYS_SIGSUSPEND: atomically swap the mask, sleep until a deliverable
/// signal arrives, then restore. Delivery happens on the return path.
pub fn sigsuspend(mask: u64) -> u64 {
    let slot = task::current_slot();
    if mask >> NSIG != 0 {
        return SYS_ERR_INVAL;
    }
    let old = process::blocked_of(slot);
    process::set_blocked(slot, mask as u32);
    while !process::has_deliverable(slot) {
        task::sleep_ms(1);
        // A stop/continue signal may also arrive; keep waiting otherwise.
    }
    process::set_blocked(slot, old);
    SYS_OK
}

/// SYS_SIGACTION: copy in/out the shared payload.
pub fn sigaction(signum: u64, act: u64, oldact: u64) -> u64 {
    let sig = signum as u32;
    if sig == 0 || sig >= NSIG {
        return SYS_ERR_INVAL;
    }
    let slot = task::current_slot();
    if oldact != 0 {
        let Some(mut old) = process::sigaction_get(slot, sig) else {
            return SYS_ERR_INVAL;
        };
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(
                &mut old as *mut fantuan_abi::SigAction as *mut u8,
                core::mem::size_of::<fantuan_abi::SigAction>(),
            )
        };
        if user::copy_out(oldact, bytes).is_none() {
            return fantuan_abi::SYS_ERR_FAULT;
        }
    }
    if act != 0 {
        let mut a = fantuan_abi::SigAction::default();
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(
                &mut a as *mut fantuan_abi::SigAction as *mut u8,
                core::mem::size_of::<fantuan_abi::SigAction>(),
            )
        };
        if user::copy_in(bytes, act).is_none() {
            return fantuan_abi::SYS_ERR_FAULT;
        }
        return process::sigaction_set(slot, sig, &a);
    }
    SYS_OK
}

/// SYS_SIGPROCMASK: how + set copy-in/out.
pub fn sigprocmask(how: u64, set: u64, oldset: u64) -> u64 {
    let slot = task::current_slot();
    let old = process::blocked_of(slot);
    if oldset != 0 {
        let bytes = (old as u64).to_le_bytes();
        if user::copy_out(oldset, &bytes).is_none() {
            return fantuan_abi::SYS_ERR_FAULT;
        }
    }
    if set != 0 {
        let mut raw = [0u8; 8];
        if user::copy_in(&mut raw, set).is_none() {
            return fantuan_abi::SYS_ERR_FAULT;
        }
        let mask = u64::from_le_bytes(raw) as u32;
        return process::sigprocmask(slot, how, mask);
    }
    SYS_OK
}
