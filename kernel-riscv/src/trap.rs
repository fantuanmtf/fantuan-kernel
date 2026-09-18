//! S-mode trap handling (M9.2b; U-mode traps M9.3c): stvec entry with the
//! sscratch swap, full register frame and dispatch. Interrupts (SBI timer)
//! are acknowledged in `timer::tick`; ecall from U-mode goes to the shared
//! syscall dispatcher; user faults (and bad user pointers caught during a
//! syscall copy) kill the task, kernel faults park after the report.
//!
//! Address-space policy: the kernel is linked at 0x80200000 and executed via
//! the PHYS_OFFSET alias, but absolute pointers (switch jump tables, hook
//! function pointers) hold the link address. Kernel code therefore always
//! runs with satp = kernel root; a per-task root is active only in U-mode.

use core::arch::{asm, global_asm};

use crate::{park, put_dec, put_hex, puts};

/// Register frame saved by `trap_entry`; layout must match the assembly.
/// x0 has no slot; x1..x31 occupy offsets 8..248, then sepc/sstatus. Slot 16
/// holds the interrupted sp (the user sp when the trap came from U-mode).
/// x0's slot is scratch: the exit path stores the return-mode flag there.
#[repr(C)]
pub struct TrapFrame {
    pub regs: [u64; 32],
    pub sepc: u64,
    pub sstatus: u64,
}

const SSTATUS_SPP: u64 = 1 << 8; // 1 = came from S-mode

global_asm!(
    ".section .text",
    ".global trap_entry",
    ".align 2",
    "trap_entry:",
    // While in U-mode sscratch holds the kernel stack top; in S-mode it is 0.
    "    csrrw   t0, sscratch, sp",
    "    beqz    t0, 1f",
    "    mv      sp, t0",            // from U: continue on the kernel stack
    "    j       2f",
    "1:",
    "    csrw    sscratch, zero",    // from S: restore the invariant
    "2:",
    "    addi    sp, sp, -272",
    "    sd      x1, 8(sp)",
    "    sd      x3, 24(sp)",
    "    sd      x4, 32(sp)",
    "    sd      x5, 40(sp)",
    "    sd      x6, 48(sp)",
    "    sd      x7, 56(sp)",
    "    sd      x8, 64(sp)",
    "    sd      x9, 72(sp)",
    "    sd      x10, 80(sp)",
    "    sd      x11, 88(sp)",
    "    sd      x12, 96(sp)",
    "    sd      x13, 104(sp)",
    "    sd      x14, 112(sp)",
    "    sd      x15, 120(sp)",
    "    sd      x16, 128(sp)",
    "    sd      x17, 136(sp)",
    "    sd      x18, 144(sp)",
    "    sd      x19, 152(sp)",
    "    sd      x20, 160(sp)",
    "    sd      x21, 168(sp)",
    "    sd      x22, 176(sp)",
    "    sd      x23, 184(sp)",
    "    sd      x24, 192(sp)",
    "    sd      x25, 200(sp)",
    "    sd      x26, 208(sp)",
    "    sd      x27, 216(sp)",
    "    sd      x28, 224(sp)",
    "    sd      x29, 232(sp)",
    "    sd      x30, 240(sp)",
    "    sd      x31, 248(sp)",
    // Interrupted sp: the user sp is in sscratch, the S-mode sp is sp+272.
    "    csrr    t0, sscratch",
    "    bnez    t0, 3f",
    "    addi    t0, sp, 272",
    "3:",
    "    sd      t0, 16(sp)",
    "    csrr    t0, sepc",
    "    sd      t0, 256(sp)",
    "    csrr    t0, sstatus",
    "    sd      t0, 264(sp)",
    // trap_dispatch(frame, scause, stval)
    "    mv      a0, sp",
    "    csrr    a1, scause",
    "    csrr    a2, stval",
    "    call    trap_dispatch",
    // Return: restore sepc/sstatus from the frame (sret takes SPP/SPIE from
    // sstatus, which is a global CSR and may have been left by another task).
    "    ld      t0, 256(sp)",
    "    csrw    sepc, t0",
    "    ld      t0, 264(sp)",
    "    csrw    sstatus, t0",
    "    csrci   sstatus, 2",         // sret re-enables interrupts via SPIE
    "    ld      t0, 16(sp)",
    "    csrw    sscratch, t0",       // stash the interrupted sp
    "    csrr    t0, sstatus",
    "    andi    t0, t0, 0x100",      // SPP: from S-mode?
    "    sd      t0, 0(sp)",          // x0's slot doubles as the exit flag
    "    ld      x1, 8(sp)",
    "    ld      x3, 24(sp)",
    "    ld      x4, 32(sp)",
    "    ld      x5, 40(sp)",
    "    ld      x6, 48(sp)",
    "    ld      x7, 56(sp)",
    "    ld      x8, 64(sp)",
    "    ld      x9, 72(sp)",
    "    ld      x10, 80(sp)",
    "    ld      x11, 88(sp)",
    "    ld      x12, 96(sp)",
    "    ld      x13, 104(sp)",
    "    ld      x14, 112(sp)",
    "    ld      x15, 120(sp)",
    "    ld      x16, 128(sp)",
    "    ld      x17, 136(sp)",
    "    ld      x18, 144(sp)",
    "    ld      x19, 152(sp)",
    "    ld      x20, 160(sp)",
    "    ld      x21, 168(sp)",
    "    ld      x22, 176(sp)",
    "    ld      x23, 184(sp)",
    "    ld      x24, 192(sp)",
    "    ld      x25, 200(sp)",
    "    ld      x26, 208(sp)",
    "    ld      x27, 216(sp)",
    "    ld      x28, 224(sp)",
    "    ld      x29, 232(sp)",
    "    ld      x30, 240(sp)",
    "    ld      x31, 248(sp)",
    "    addi    sp, sp, 272",
    "    ld      t0, -272(sp)",       // exit flag (x0 slot)
    "    beqz    t0, 6f",
    // Back to S-mode: sp is already the interrupted stack.
    "    ld      t0, -232(sp)",       // restore user t0 (x5)
    "    csrw    sscratch, zero",
    "    sret",
    "6:",
    // Back to U-mode: sp currently holds the kernel stack top.
    "    ld      t0, -232(sp)",       // restore user t0 while still on it
    "    csrrw   sp, sscratch, sp",   // sp = user sp, sscratch = kernel top
    "    sret",
);

extern "C" {
    fn trap_entry();
}

/// Point stvec at the entry (direct mode; 4-byte aligned).
pub fn init() {
    let addr = trap_entry as *const () as u64;
    unsafe { asm!("csrw stvec, {}", in(reg) addr, options(nostack)) };
}

/// Back to U-mode needs the task's root active again.
fn reenter_user_root(from_user: bool) {
    if from_user {
        crate::paging::set_root(kernel_core::task::current_vm_root());
    }
}

#[no_mangle]
pub extern "C" fn trap_dispatch(tf: *mut TrapFrame, scause: u64, stval: u64) {
    let tf = unsafe { &mut *tf };
    let interrupt = scause >> 63 != 0;
    let code = scause & 0xFF;
    let from_user = tf.sstatus & SSTATUS_SPP == 0;
    if from_user {
        // Kernel code may jump through absolute link addresses: run it on
        // the kernel root and re-enter the user root just before sret.
        crate::paging::set_root(crate::paging::kernel_root());
    }

    if interrupt {
        if code == 5 {
            crate::timer::tick();
        } else {
            puts("trap: unexpected interrupt code=");
            put_dec(code);
            puts("\n");
        }
        reenter_user_root(from_user);
        return;
    }

    match code {
        8 if from_user => {
            // ecall from U-mode: a7 = number, a0..a4 = args, result in a0.
            let n = tf.regs[17];
            let args = [tf.regs[10], tf.regs[11], tf.regs[12], tf.regs[13], tf.regs[14]];
            tf.regs[10] = kernel_core::syscall::dispatch(crate::syscall::write, n, &args);
            tf.sepc += 4;
        }
        3 if !from_user => {
            // Breakpoint: resume after the instruction (2 or 4 bytes).
            let instr = unsafe { core::ptr::read_volatile(tf.sepc as *const u16) };
            tf.sepc += if instr & 3 == 3 { 4 } else { 2 };
            puts("trap: ebreak handled\n");
        }
        _ if from_user || crate::syscall::in_user_copy() => {
            crate::syscall::clear_user_copy();
            puts("exc ");
            put_dec(code);
            puts(" [user] scause=");
            put_hex(scause);
            puts(" stval=");
            put_hex(stval);
            puts(" sepc=");
            put_hex(tf.sepc);
            puts("\n  killing user task ");
            put_dec(kernel_core::task::current_id());
            puts("\n");
            // exit() schedules; its hook pointers are link addresses.
            crate::paging::set_root(crate::paging::kernel_root());
            kernel_core::task::exit(1);
        }
        _ => {
            puts("trap: exception scause=");
            put_hex(scause);
            puts(" stval=");
            put_hex(stval);
            puts(" sepc=");
            put_hex(tf.sepc);
            puts(" [kernel]\n");
            park();
        }
    }
    reenter_user_root(from_user);
}
