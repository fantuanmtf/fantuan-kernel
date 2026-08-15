//! Exception vectors, names, and handlers (DESIGN.md §2: the exception path
//! is core Rust — interrupts.S only pushes the frame).

use core::fmt::Write;

use crate::interrupts::{cpu_frame, InterruptFrame};
use crate::pit;
use crate::serial::{self, Serial};

pub const EXC_DIVIDE: usize = 0;
pub const EXC_DEBUG: usize = 1;
pub const EXC_BREAKPOINT: usize = 3;
pub const EXC_OVERFLOW: usize = 4;
pub const EXC_BOUND: usize = 5;
pub const EXC_INVALID_OPCODE: usize = 6;
pub const EXC_DEVICE_NA: usize = 7;
pub const EXC_DOUBLE_FAULT: usize = 8;
pub const EXC_INVALID_TSS: usize = 10;
pub const EXC_SEGMENT_NP: usize = 11;
pub const EXC_STACK: usize = 12;
pub const EXC_GENERAL_PROTECTION: usize = 13;
pub const EXC_PAGE_FAULT: usize = 14;
pub const EXC_X87: usize = 16;
pub const EXC_ALIGNMENT: usize = 17;
pub const EXC_MACHINE_CHECK: usize = 18;
pub const EXC_SIMD: usize = 19;
pub const EXC_VIRT: usize = 20;
pub const EXC_CONTROL_PROTECTION: usize = 21;
pub const EXC_VMM: usize = 28;
pub const EXC_SECURITY: usize = 30;

pub fn handle(f: &mut InterruptFrame) {
    let v = f.vector as usize;
    let rip = cpu_frame(f).rip;
    let mut s = Serial::new(serial::COM1);
    let _ = writeln!(s, "exc {} ({}) err={:#x} rip={:#x}", v, name(v), f.error_code, rip);

    if v == EXC_PAGE_FAULT {
        let cr2: u64;
        unsafe {
            core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags));
        }
        let _ = writeln!(s, "  page fault at {:#x}", cr2);
    }

    // Recoverable (software-triggered) exceptions return to the faulting
    // instruction stream; everything else is fatal for now.
    match v {
        EXC_BREAKPOINT | EXC_DIVIDE | EXC_OVERFLOW => {}
        _ => halt(),
    }
}

fn name(v: usize) -> &'static str {
    match v {
        EXC_DIVIDE => "#DE divide error",
        EXC_DEBUG => "#DB debug",
        EXC_BREAKPOINT => "#BP breakpoint",
        EXC_OVERFLOW => "#OF overflow",
        EXC_BOUND => "#BR bound range",
        EXC_INVALID_OPCODE => "#UD invalid opcode",
        EXC_DEVICE_NA => "#NM device not available",
        EXC_DOUBLE_FAULT => "#DF double fault",
        EXC_INVALID_TSS => "#TS invalid TSS",
        EXC_SEGMENT_NP => "#NP segment not present",
        EXC_STACK => "#SS stack fault",
        EXC_GENERAL_PROTECTION => "#GP general protection",
        EXC_PAGE_FAULT => "#PF page fault",
        EXC_X87 => "#MF x87 FPU",
        EXC_ALIGNMENT => "#AC alignment check",
        EXC_MACHINE_CHECK => "#MC machine check",
        EXC_SIMD => "#XM SIMD",
        EXC_VIRT => "#VE virtualization",
        EXC_CONTROL_PROTECTION => "#CP control protection",
        EXC_VMM => "#VC VMM communication",
        EXC_SECURITY => "#SX security",
        _ => "reserved",
    }
}

fn halt() -> ! {
    pit::beep_n(4, pit::BeepLen::Short);
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)) }
    }
}
