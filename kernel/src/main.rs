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

// The first userland program, embedded at build time (tools/build.sh builds
// the user crate first; kernel/build.rs bakes the ELF in).
include!(concat!(env!("OUT_DIR"), "/user_program.rs"));

mod console;
mod consts;
mod cpu;
mod elf;
mod exceptions;
mod font;
mod gdt;
mod idt;
mod interrupts;
mod mm;
mod pic;
mod pit;
mod port;
mod serial;
mod syscall;
mod task;
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
    let _ = writeln!(s, "fantuan-kernel v0.1 (M3)");
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

    cpu::sti();
    let _ = writeln!(s, "interrupts: enabled");

    // --- M2: memory management --------------------------------------------
    // Guard: the kernel must actually run at its linked higher-half address.
    if (kmain as *const () as u64) < fantuan_abi::PHYS_OFFSET {
        let _ = writeln!(s, "fatal: kernel not running at PHYS_OFFSET (link/paging mismatch)");
        halt_forever();
    }

    mm::frame::init(bi);
    let alloc = mm::frame::get();
    let _ = writeln!(
        s,
        "mm: frame allocator ready: {} MiB usable (bitmap {} KiB)",
        alloc.usable_mib(),
        mm::frame::BITMAP_BYTES / 1024
    );
    let new_pml4 = mm::paging::init(alloc);
    let _ = writeln!(
        s,
        "paging: kernel tables @ phys {:#x}, kmain @ {:#x}",
        new_pml4, kmain as *const () as usize
    );

    // The bootloader's tables are unreferenced after the switch: reclaim them.
    for i in 0..bi.boot_tables_pages {
        alloc.free(bi.boot_pml4 + i * mm::frame::FRAME_SIZE);
    }
    let _ = writeln!(s, "mm: reclaimed {} bootloader table pages", bi.boot_tables_pages);

    // Self-test: a fresh frame must be readable/writable through the alias.
    if let Some(f) = alloc.alloc() {
        let probe = mm::paging::phys_to_virt(f) as *mut u64;
        unsafe { probe.write_volatile(0xDEAD_BEEF_CAFE_F00D); }
        let ok = unsafe { probe.read_volatile() } == 0xDEAD_BEEF_CAFE_F00D;
        alloc.free(f);
        let _ = writeln!(s, "mm: frame self-test {} (frame {:#x} via PHYS_OFFSET alias)", if ok { "ok" } else { "FAILED" }, f);
        if !ok {
            halt_forever();
        }
    } else {
        let _ = writeln!(s, "mm: frame self-test FAILED (allocator returned no frames)");
        halt_forever();
    }

    // Exception demos: both paths (with/without error code) must work.
    unsafe {
        core::arch::asm!("int3", options(nomem, nostack));
    }
    let _ = writeln!(s, "demo: #BP handled (no error code path)");
    unsafe {
        core::arch::asm!("int 0", options(nomem, nostack));
    }
    let _ = writeln!(s, "demo: #DE handled (no error code path)");

    // --- M3: kernel tasks + syscall ABI ------------------------------------
    let abi = syscall::syscall(syscall::SYS_VERSION, 0, 0, 0, 0, 0);
    let _ = writeln!(s, "syscall: ABI v{} (int 0x60, versioned dispatch)", abi);
    task::init(bi.stack_top);
    task::spawn(demo_1);
    task::spawn(demo_2);
    task::spawn(demo_3);
    let _ = writeln!(s, "sched: 3 kernel demo tasks spawned (quantum 100 ms)");

    // --- M4: user mode ------------------------------------------------------
    let u1 = task::spawn_user(USER_ELF);
    let u2 = task::spawn_user(USER_ELF);
    let _ = writeln!(
        s,
        "user: ELF {} bytes -> tids {} {} (ring 3, per-task page tables)",
        USER_ELF.len(),
        u1.unwrap_or(0),
        u2.unwrap_or(0)
    );

    // Boot-complete signal: one LONG beep — distinct from the short-beep
    // diagnostic codes (DESIGN.md §6.2).
    pit::beep_long();
    let _ = writeln!(s, "beep: boot ok (1 long)");
    let _ = writeln!(s, "kernel: idle (hlt; IRQ0 ticks wake it)");

    // Idle (task 0): hlt wakes on every 10 ms tick; the scheduler hands the
    // CPU to the demo tasks.
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}

// --- M3 demo tasks ---------------------------------------------------------

fn push_str(buf: &mut [u8], mut off: usize, s: &str) -> usize {
    for &b in s.as_bytes() {
        if off < buf.len() {
            buf[off] = b;
            off += 1;
        }
    }
    off
}

fn push_u64(buf: &mut [u8], mut off: usize, mut v: u64) -> usize {
    if v == 0 {
        buf[off] = b'0';
        return off + 1;
    }
    let mut tmp = [0u8; 20];
    let mut n = 0;
    while v > 0 {
        tmp[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
    }
    while n > 0 {
        n -= 1;
        buf[off] = tmp[n];
        off += 1;
    }
    off
}

/// Every task: report its identity through the syscall layer, then sleep
/// (woken by the scheduler's deadline check).
fn demo_task(tag: u64) -> ! {
    let mut n = 0u64;
    loop {
        let tid = syscall::syscall(syscall::SYS_GET_TID, 0, 0, 0, 0, 0);
        let mut buf = [0u8; 96];
        let mut off = 0;
        off = push_str(&mut buf, off, "task ");
        off = push_u64(&mut buf, off, tag);
        off = push_str(&mut buf, off, " (tid ");
        off = push_u64(&mut buf, off, tid);
        off = push_str(&mut buf, off, "): hello ");
        off = push_u64(&mut buf, off, n);
        buf[off] = b'\n';
        off += 1;
        syscall::syscall(syscall::SYS_WRITE, buf.as_ptr() as u64, off as u64, 0, 0, 0);
        n += 1;
        // Different periods per task; the scheduler's deadline check wakes us.
        syscall::syscall(syscall::SYS_SLEEP_MS, 250 + tag * 150, 0, 0, 0, 0);
    }
}

fn demo_1() -> ! {
    demo_task(1)
}
fn demo_2() -> ! {
    demo_task(2)
}
fn demo_3() -> ! {
    demo_task(3)
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // No allocation, no console dependency: raw bytes to COM1, then beeps.
    let mut s = serial();
    let _ = writeln!(s, "\nPANIC: {}", info);
    pit::beep_n(4, pit::BeepLen::Short);
    halt_forever()
}
