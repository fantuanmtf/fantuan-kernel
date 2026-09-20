//! EL1 exception vectors + dispatch (M11 R9a). `vector_table` is the 16-entry
//! VBAR_EL1 table; every entry routes to a save-frame stub that passes the
//! frame and a kind (0 sync, 1 IRQ, 2 FIQ, 3 SError) to `trap_dispatch`.
//! ELR_EL1/SPSR_EL1 are restored from the per-task frame right before `eret`
//! (they are global registers a context switch may have left stale).
//!
//! The deliberate demo is `brk #0` from EL1: EC=0x3C, resume at ELR+4, the
//! same evidence shape as riscv's `ebreak`. Kernel faults park after a
//! report; there is no user mode yet, so no kill path.

use core::arch::{asm, global_asm};

use crate::{park, put_dec, put_hex, puts};

/// Register frame saved by the stubs; layout must match the assembly.
#[repr(C)]
pub struct TrapFrame {
    pub x: [u64; 31], // x0..x30
    pub elr: u64,
    pub spsr: u64,
    pub esr: u64,
    pub far: u64,
}

global_asm!(
    ".section .text",
    ".macro save_frame",
    "    sub sp, sp, #288",
    "    stp x0, x1, [sp, #0]",
    "    stp x2, x3, [sp, #16]",
    "    stp x4, x5, [sp, #32]",
    "    stp x6, x7, [sp, #48]",
    "    stp x8, x9, [sp, #64]",
    "    stp x10, x11, [sp, #80]",
    "    stp x12, x13, [sp, #96]",
    "    stp x14, x15, [sp, #112]",
    "    stp x16, x17, [sp, #128]",
    "    stp x18, x19, [sp, #144]",
    "    stp x20, x21, [sp, #160]",
    "    stp x22, x23, [sp, #176]",
    "    stp x24, x25, [sp, #192]",
    "    stp x26, x27, [sp, #208]",
    "    stp x28, x29, [sp, #224]",
    "    str x30, [sp, #240]",
    "    mrs x0, elr_el1",
    "    str x0, [sp, #248]",
    "    mrs x0, spsr_el1",
    "    str x0, [sp, #256]",
    "    mrs x0, esr_el1",
    "    str x0, [sp, #264]",
    "    mrs x0, far_el1",
    "    str x0, [sp, #272]",
    ".endm",
    ".macro entry kind",
    "    save_frame",
    "    mov x0, sp",
    "    mov x1, #\\kind",
    "    bl trap_dispatch",
    "    b trap_exit",
    ".endm",
    ".align 11",
    ".global vector_table",
    "vector_table:",
    // Current EL with SP_EL0 (0x000..0x180)
    ".align 7", "b trap_sync",
    ".align 7", "b trap_irq",
    ".align 7", "b trap_fiq",
    ".align 7", "b trap_serr",
    // Current EL with SP_ELx (0x200..0x380)
    ".align 7", "b trap_sync",
    ".align 7", "b trap_irq",
    ".align 7", "b trap_fiq",
    ".align 7", "b trap_serr",
    // Lower EL, AArch64 (0x400..0x580)
    ".align 7", "b trap_sync",
    ".align 7", "b trap_irq",
    ".align 7", "b trap_fiq",
    ".align 7", "b trap_serr",
    // Lower EL, AArch32 (0x600..0x780)
    ".align 7", "b trap_sync",
    ".align 7", "b trap_irq",
    ".align 7", "b trap_fiq",
    ".align 7", "b trap_serr",
    "trap_sync:",
    "    entry 0",
    "trap_irq:",
    "    entry 1",
    "trap_fiq:",
    "    entry 2",
    "trap_serr:",
    "    entry 3",
    "trap_exit:",
    "    ldr x0, [sp, #248]",
    "    msr elr_el1, x0",
    "    ldr x0, [sp, #256]",
    "    msr spsr_el1, x0",
    "    ldp x0, x1, [sp, #0]",
    "    ldp x2, x3, [sp, #16]",
    "    ldp x4, x5, [sp, #32]",
    "    ldp x6, x7, [sp, #48]",
    "    ldp x8, x9, [sp, #64]",
    "    ldp x10, x11, [sp, #80]",
    "    ldp x12, x13, [sp, #96]",
    "    ldp x14, x15, [sp, #112]",
    "    ldp x16, x17, [sp, #128]",
    "    ldp x18, x19, [sp, #144]",
    "    ldp x20, x21, [sp, #160]",
    "    ldp x22, x23, [sp, #176]",
    "    ldp x24, x25, [sp, #192]",
    "    ldp x26, x27, [sp, #208]",
    "    ldp x28, x29, [sp, #224]",
    "    ldr x30, [sp, #240]",
    "    add sp, sp, #288",
    "    eret",
);

extern "C" {
    static vector_table: u8;
}

/// Point VBAR_EL1 at the table (2 KiB aligned, as the architecture requires).
pub fn init() {
    let addr = core::ptr::addr_of!(vector_table) as u64;
    unsafe {
        asm!("msr vbar_el1, {}", in(reg) addr, options(nostack));
        asm!("isb", options(nostack));
    }
}

fn fatal(what: &str) -> ! {
    puts("trap: unexpected ");
    puts(what);
    puts(" - parking\n");
    park()
}

fn irq() {
    let id = crate::gic::ack();
    if id >= 1020 {
        return; // spurious
    }
    crate::gic::eoi(id);
    if id == crate::gic::TIMER_IRQ {
        crate::timer::tick();
    } else {
        puts("trap: unexpected irq ");
        put_dec(id as u64);
        puts(" - parking\n");
        park();
    }
}

fn sync(tf: &mut TrapFrame) {
    let ec = (tf.esr >> 26) & 0x3F;
    match ec {
        0x3C => {
            // BRK: resume after the 32-bit instruction.
            puts("trap: brk handled\n");
            tf.elr += 4;
        }
        0x15 => fatal("svc (no user mode yet)"),
        _ => {
            puts("trap: unexpected sync ec=");
            put_hex(ec);
            puts(" elr=");
            put_hex(tf.elr);
            puts(" far=");
            put_hex(tf.far);
            puts(" esr=");
            put_hex(tf.esr);
            puts("\n");
            park();
        }
    }
}

#[no_mangle]
pub extern "C" fn trap_dispatch(tf: *mut TrapFrame, kind: u64) {
    let tf = unsafe { &mut *tf };
    match kind {
        0 => sync(tf),
        1 => irq(),
        2 => fatal("fiq"),
        _ => fatal("serror"),
    }
}
