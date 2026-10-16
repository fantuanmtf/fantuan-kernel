//! The x86_64 bring-up phases, in the order `kmain` calls them.
//!
//! Split out of `main.rs` in 2026-10: that file was 418 lines against the
//! 300-line rule and its own budget entry said "split into boot + wiring".
//! This half is "power the machine on in an order that cannot be reordered";
//! `wiring` is "start tasks and hand the console to the shell"; `report` is
//! "look at what is attached and mount it". `main.rs` keeps the entry point
//! and calls these in sequence, because **the order is the correctness
//! argument** — everything below marked "must precede" is a real dependency,
//! not a preference.

use core::fmt::Write;

use fantuan_abi::{BootInfo, BOOT_MAGIC, BOOT_VERSION};

use crate::bootlog::{halt_forever, print_memmap_summary, serial};
use crate::serial::Serial;

pub mod report;

// Kernel image end (link.ld); the shared frame allocator needs it to punch
// the kernel hole in its bitmap.
extern "C" {
    static __bss_end: u8;
}

/// Serial, the shared log sink, the arch hooks, the handshake check, the
/// framebuffer console and the pure-logic self-tests.
///
/// Returns the serial handle the later phases write to. The `Console` value
/// stays local to this function: the phases that come after only need to know
/// whether one exists, which they ask `console::up()`.
pub fn phase_core(bi: &BootInfo) -> Serial {
    // Output channel 1: serial (16550 COM1), 115200 8N1.
    let mut s = serial();
    s.init();
    // Shared (kernel-core) diagnostics log through the serial sink.
    kernel_core::log::set_sink(crate::serial::log_bytes);

    // M9.4-2: install the x86 hooks the shared modules need. All of them are
    // safe to install here; the ones with boot-time dependencies resolve
    // lazily (e.g. the clock is 0 until tsc::calibrate).
    crate::drivers::install_hooks();

    // Handshake: validate what the bootloader handed over (DESIGN.md §4).
    if bi.magic != BOOT_MAGIC || bi.version != BOOT_VERSION {
        let _ = writeln!(s, "fatal: bad handshake magic={:#x} version={}", bi.magic, bi.version);
        crate::pit::beep_n(4, crate::pit::BeepLen::Short);
        halt_forever();
    }

    let _ = writeln!(s);
    let _ = writeln!(s, "fantuan v0.0.4");
    let _ = writeln!(
        s,
        "handshake ok: magic={:#x} version={} rsdp={:#x}",
        bi.magic, bi.version, bi.rsdp
    );
    let _ = writeln!(s, "kernel_base={:#x} stack_top={:#x}", bi.kernel_base, bi.stack_top);
    print_memmap_summary(&mut s, bi);

    // Output channel 2: GOP framebuffer console (skipped when headless).
    let fb = &bi.fb;
    let _ = writeln!(
        s,
        "framebuffer: base={:#x} size={:#x} {}x{} stride={} format={}",
        fb.base, fb.size, fb.width, fb.height, fb.stride, fb.format
    );
    #[cfg(kconfig_graphics)]
    let mut con = crate::console::Console::new(fb);
    #[cfg(kconfig_graphics)]
    if let Some(c) = con.as_mut() {
        let _ = writeln!(c, "fantuan v0.0.4");
        let _ = writeln!(c, "handshake ok: magic={:#x} version={}", bi.magic, bi.version);
        let _ = writeln!(c, "console: GOP framebuffer {}x{}", fb.width, fb.height);
    } else {
        let _ = writeln!(s, "console: none (serial-only; GOP unavailable)");
    }
    #[cfg(not(kconfig_graphics))]
    let _ = writeln!(s, "console: none (serial-only; GOP unavailable)");

    // M13-1: the framebuffer core self-test (pure logic, no drawing).
    #[cfg(kconfig_graphics)]
    if kernel_core::graphics::selftest() {
        let _ = writeln!(s, "graphics: damage self-test ok");
    } else {
        let _ = writeln!(s, "graphics: damage self-test FAILED");
    }

    // M13-3: the input event ring self-test (order + drop policy, no input).
    #[cfg(kconfig_graphics)]
    if kernel_core::input_ring::selftest() {
        let _ = writeln!(s, "input: ring self-test ok");
    } else {
        let _ = writeln!(s, "input: ring self-test FAILED");
    }

    // M13-4: the graphics event ring self-test (ABI size + order + drop policy).
    #[cfg(kconfig_graphics)]
    if kernel_core::graphics::event::selftest() {
        let _ = writeln!(s, "gfx: event ring self-test ok");
    } else {
        let _ = writeln!(s, "gfx: event ring self-test FAILED");
    }

    s
}

/// GDT, IDT, PIC, keyboard, the timers and the PS/2 mouse, then interrupts on.
pub fn phase_interrupts(s: &mut Serial) {
    // --- M1: interrupt machinery -----------------------------------------
    crate::gdt::init();
    let _ = writeln!(s, "gdt: authoritative GDT + TSS (IST1 = #DF stack)");
    crate::idt::init();
    let _ = writeln!(s, "idt: 256 entries, #DF on IST1");
    crate::pic::init();
    let _ = writeln!(s, "pic: remapped 0x20/0x28, only IRQ0 unmasked");

    // M8.5a: PS/2 keyboard on IRQ1. Absent hardware degrades to serial-only.
    if crate::kbd::init() {
        crate::pic::unmask(crate::consts::IRQ_KEYBOARD);
        let _ = writeln!(s, "kbd: IRQ1 unmasked (serial + keyboard input)");
    } else {
        let _ = writeln!(s, "kbd: no PS/2 keyboard (serial-only input)");
    }

    // TSC calibration MUST precede the PIT going periodic (channel 0).
    crate::tsc::calibrate();
    crate::pit::init_timer(100);
    let _ = writeln!(s, "timer: PIT 100 Hz on IRQ0, TSC {} MHz", crate::tsc::hz() / 1_000_000);

    // M13-3: PS/2 mouse on the 8042 aux port (IRQ12). After the timer so the
    // bounded ACK waits can sleep; before interrupts so the reset bytes are
    // polled, not routed through the (masked) IRQ12.
    #[cfg(kconfig_graphics)]
    if crate::mouse::init() {
        let _ = writeln!(s, "mouse: PS/2 aux enabled (IRQ12, 3-byte packets)");
    } else {
        let _ = writeln!(s, "mouse: PS/2 aux unavailable (no pointer input)");
    }

    crate::cpu::sti();
    let _ = writeln!(s, "interrupts: enabled");
}

/// The higher-half guard, the frame allocator, the kernel page tables and the
/// double-buffer upgrade. Requires `phase_interrupts` (the allocator takes the
/// arch IRQ hooks, and the self-test needs working exceptions).
pub fn phase_memory(bi: &BootInfo, s: &mut Serial) {
    // --- M2: memory management --------------------------------------------
    // Guard: the kernel must actually run at its linked higher-half address.
    if (crate::kmain as *const () as u64) < fantuan_abi::PHYS_OFFSET {
        let _ = writeln!(s, "fatal: kernel not running at PHYS_OFFSET (link/paging mismatch)");
        halt_forever();
    }

    // M9.2a: install the arch IRQ hooks for the shared allocator, then
    // initialize it from the memory map (kernel end from the linker script).
    kernel_core::arch::set_irq_ops(crate::cpu::irq_save, crate::cpu::irq_restore);
    kernel_core::arch::set_irq_enable(crate::cpu::sti);
    let kernel_end_phys = core::ptr::addr_of!(__bss_end) as u64 - fantuan_abi::PHYS_OFFSET;
    crate::mm::frame::init(bi, kernel_end_phys, &[]);
    let alloc = crate::mm::frame::get();
    let _ = writeln!(s, "mm: frame allocator ready: {} MiB usable (bitmap {} KiB)",
        alloc.usable_mib(), crate::mm::frame::BITMAP_BYTES / 1024);
    let new_pml4 = crate::mm::paging::init(alloc);
    let _ = writeln!(s, "paging: kernel tables @ phys {:#x}, kmain @ {:#x}",
        new_pml4, crate::kmain as *const () as usize);

    // The bootloader's tables are unreferenced after the switch: reclaim them.
    // reclaim() (not free()) because these reserved frames were never handed
    // out by the allocator.
    for i in 0..bi.boot_tables_pages {
        alloc.reclaim(bi.boot_pml4 + i * crate::mm::frame::FRAME_SIZE);
    }
    let _ = writeln!(s, "mm: reclaimed {} bootloader table pages", bi.boot_tables_pages);

    // Self-test: a fresh frame must be readable/writable through the alias.
    if let Some(f) = alloc.alloc() {
        let probe = crate::mm::paging::phys_to_virt(f) as *mut u64;
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

    // M13-2: swap the console to a frame-allocator back buffer (double buffer).
    #[cfg(kconfig_graphics)]
    if crate::console::up() {
        if crate::console::upgrade_buffered() {
            let _ = writeln!(s, "console: double-buffered (frame-allocator back buffer)");
        } else {
            let _ = writeln!(s, "console: direct present (back buffer unavailable)");
        }
    }
}

/// The two exception demos and the crypto known-answer tests.
pub fn phase_traps_and_crypto(s: &mut Serial) {
    // Exception demos: both paths (with/without error code) must work.
    unsafe {
        core::arch::asm!("int3", options(nomem, nostack));
    }
    let _ = writeln!(s, "demo: #BP handled (no error code path)");
    unsafe {
        core::arch::asm!("int 0", options(nomem, nostack));
    }
    let _ = writeln!(s, "demo: #DE handled (no error code path)");

    // --- M8.1: crypto known-answer tests (SHA-256 + RSA-2048) --------------
    // Failure does not halt the boot: crypto is only needed to apply
    // operator-provided .auth bundles (the shell can re-run the tests).
    if crate::crypto::selftest(s, false) {
        let _ = writeln!(s, "crypto: KATs ok (sha256 + rsa-2048)");
    } else {
        let _ = writeln!(s, "crypto: KATs FAILED — authenticated variable updates unavailable");
    }
}

/// The net stack bring-up and the memory hardening (NX + SMEP/SMAP).
/// **Must follow** `wiring::phase_processes` in `kmain`: NX must be live
/// before the ELF loader sets P_NX entries, and the demos are already running.
pub fn phase_hardening(s: &mut Serial) {
    // M11 R2: NetBSD rump adaptation layer (pools, callouts, mbufs).
    #[cfg(kconfig_net)]
    crate::net::init();

    // --- M8.3c: memory hardening (NX + SMEP/SMAP) --------------------------
    // Missing features are reported and skipped (the default qemu64 CPU lacks
    // SMEP/SMAP; tools/run.sh passes -cpu max so smoke exercises them).
    let nx = crate::cpu::enable_nxe();
    let smep = crate::cpu::enable_smep();
    let smap = crate::cpu::enable_smap();
    let _ = writeln!(s, "mmu: nx {} smep {} smap {} (unsupported features skipped)", nx, smep, smap);
}
