//! fantuan-kernel — M1: interrupts & exceptions.
//!
//! Boot chain: UEFI firmware -> fantuan-boot (Rust EFI app) -> boot/entry.S ->
//! kmain. On top of M0 (handshake, serial, GOP console, beeper), M1 adds:
//! authoritative GDT + TSS (IST1 for #DF), IDT with 256 stubs, PIC remap,
//! 100 Hz PIT timer on IRQ0, TSC-calibrated sleep, exception handlers.

#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;

use fantuan_abi::{BootInfo, BOOT_MAGIC, BOOT_VERSION};

mod console;
mod consts;
mod exceptions;
mod font;
mod gdt;
mod idt;
mod interrupts;
mod pic;
mod pit;
mod port;
mod serial;
mod timer;
mod tsc;

fn serial() -> serial::Serial {
    serial::Serial::new(serial::COM1)
}

fn print_memmap_summary(s: &mut serial::Serial, bi: &BootInfo) {
    let n = bi.memmap.count;
    let mut conv_pages: u64 = 0;
    let mut other_pages: u64 = 0;
    unsafe {
        let mut ptr = bi.memmap.ptr;
        for _ in 0..n {
            let d = &*ptr;
            if d.type_ == fantuan_abi::MEMORY_TYPE_CONVENTIONAL {
                conv_pages += d.number_of_pages;
            } else {
                other_pages += d.number_of_pages;
            }
            ptr = (ptr as *const u8).add(bi.memmap.desc_size) as *const _;
        }
    }
    let _ = writeln!(
        s,
        "memory map: {} descriptors, conventional {} MiB, other {} MiB",
        n,
        conv_pages * 4 / 1024,
        other_pages * 4 / 1024
    );
}

fn halt_forever() -> ! {
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}

#[no_mangle]
pub extern "sysv64" fn kmain(boot_info: *const BootInfo) -> ! {
    let bi = unsafe { &*boot_info };

    // Output channel 1: serial (16550 COM1), 115200 8N1.
    let mut s = serial();
    s.init();

    // Handshake: validate what the bootloader handed over (DESIGN.md §4).
    if bi.magic != BOOT_MAGIC || bi.version != BOOT_VERSION {
        let _ = writeln!(
            s,
            "fatal: bad handshake magic={:#x} version={}",
            bi.magic, bi.version
        );
        pit::beep_n(4, pit::BeepLen::Short);
        halt_forever();
    }

    let _ = writeln!(s);
    let _ = writeln!(s, "fantuan-kernel v0.1 (M1)");
    let _ = writeln!(
        s,
        "handshake ok: magic={:#x} version={} rsdp={:#x}",
        bi.magic, bi.version, bi.rsdp
    );
    let _ = writeln!(
        s,
        "kernel_base={:#x} stack_top={:#x}",
        bi.kernel_base, bi.stack_top
    );
    print_memmap_summary(&mut s, bi);

    // Output channel 2: GOP framebuffer console (skipped when headless).
    let fb = &bi.fb;
    let _ = writeln!(
        s,
        "framebuffer: base={:#x} size={:#x} {}x{} stride={} format={}",
        fb.base, fb.size, fb.width, fb.height, fb.stride, fb.format
    );
    let mut con = console::Console::new(fb);
    if let Some(c) = con.as_mut() {
        let _ = writeln!(c, "fantuan-kernel v0.1 (M1)");
        let _ = writeln!(c, "handshake ok: magic={:#x} version={}", bi.magic, bi.version);
        let _ = writeln!(c, "console: GOP framebuffer {}x{}", fb.width, fb.height);
    } else {
        let _ = writeln!(s, "console: none (serial-only; GOP unavailable)");
    }

    // --- M1: interrupt machinery -----------------------------------------
    gdt::init();
    let _ = writeln!(s, "gdt: authoritative GDT + TSS (IST1 = #DF stack)");
    idt::init();
    let _ = writeln!(s, "idt: 256 entries, #DF on IST1");
    pic::init();
    let _ = writeln!(s, "pic: remapped 0x20/0x28, only IRQ0 unmasked");

    // TSC calibration MUST precede the PIT going periodic (channel 0).
    tsc::calibrate();
    pit::init_timer(100);
    let _ = writeln!(s, "timer: PIT 100 Hz on IRQ0, TSC {} MHz", tsc::hz() / 1_000_000);

    unsafe {
        core::arch::asm!("sti", options(nomem, nostack));
    }
    let _ = writeln!(s, "interrupts: enabled");

    // Exception demos: both paths (with/without error code) must work.
    unsafe {
        core::arch::asm!("int3", options(nomem, nostack));
    }
    let _ = writeln!(s, "demo: #BP handled (no error code path)");
    unsafe {
        core::arch::asm!("int 0", options(nomem, nostack));
    }
    let _ = writeln!(s, "demo: #DE handled (no error code path)");

    // Boot-complete signal: one LONG beep — distinct from the short-beep
    // diagnostic codes (DESIGN.md §6.2).
    pit::beep_long();
    let _ = writeln!(s, "beep: boot ok (1 long)");
    let _ = writeln!(s, "kernel: idle (hlt; IRQ0 ticks wake it)");

    // Idle: hlt wakes on every 10 ms tick.
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // No allocation, no console dependency: raw bytes to COM1, then beeps.
    let mut s = serial();
    let _ = writeln!(s, "\nPANIC: {}", info);
    pit::beep_n(4, pit::BeepLen::Short);
    halt_forever()
}
