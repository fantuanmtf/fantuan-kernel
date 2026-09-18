//! S-mode trap handling (M9.2b): stvec entry, full register frame and
//! dispatch. Interrupts (SBI timer) are acknowledged in `timer::tick`;
//! exceptions are reported with scause/stval/sepc. User-mode faults are
//! classified later (M9.3); kernel faults park after the report.

use core::arch::{asm, global_asm};

use crate::{park, put_dec, put_hex, puts};

/// Register frame saved by `trap_entry`; layout must match the assembly.
/// x0 has no slot; x1..x31 occupy offsets 8..248, then sepc/sstatus.
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
    // Original sp (pre-decrement) goes in x2's slot.
    "    addi    t0, sp, 272",
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
    "    ld      t0, 256(sp)",
    "    csrw    sepc, t0",
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

#[no_mangle]
pub extern "C" fn trap_dispatch(tf: *mut TrapFrame, scause: u64, stval: u64) {
    let tf = unsafe { &mut *tf };
    let interrupt = scause >> 63 != 0;
    let code = scause & 0xFF;

    if interrupt {
        if code == 5 {
            crate::timer::tick();
        } else {
            puts("trap: unexpected interrupt code=");
            put_dec(code);
            puts("\n");
        }
        return;
    }

    match code {
        3 => {
            // Breakpoint: resume after the instruction (2 or 4 bytes).
            let instr = unsafe { core::ptr::read_volatile(tf.sepc as *const u16) };
            tf.sepc += if instr & 3 == 3 { 4 } else { 2 };
            puts("trap: ebreak handled\n");
        }
        _ => {
            puts("trap: exception scause=");
            put_hex(scause);
            puts(" stval=");
            put_hex(stval);
            puts(" sepc=");
            put_hex(tf.sepc);
            puts(if tf.sstatus & SSTATUS_SPP != 0 { " [kernel]\n" } else { " [user]\n" });
            park();
        }
    }
}
