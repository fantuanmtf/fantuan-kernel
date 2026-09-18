//! fantuan-kernel — boot entry + core bring-up (M0-M8).
//!
//! Boot chain: UEFI firmware -> fantuan-boot (Rust EFI app) -> boot/entry.S ->
//! kmain. The tree currently carries: handshake/serial/GOP console (M0),
//! interrupts + PIT/TSC (M1), higher-half kernel + frame allocator (M2),
//! tasks/scheduler + syscall ABI (M3), ring-3 + ELF loader (M4), the C driver
//! boundary + AHCI (M4.5), diagnostics (M5/M5.5), the VFS (M6), boot repair
//! (M7.x), the built-in shell (§10), and the storage ops registry + NVMe (M8).

#![no_std]
#![no_main]

use core::fmt::Write;

use fantuan_abi::{BootInfo, BOOT_MAGIC, BOOT_VERSION};

use bootlog::{halt_forever, print_memmap_summary, serial};

// The first userland program, embedded at build time (tools/build.sh builds
// the user crate first; kernel/build.rs bakes the ELF in).
include!(concat!(env!("OUT_DIR"), "/user_program.rs"));

// Kernel image end (link.ld); the shared frame allocator needs it to punch
// the kernel hole in its bitmap.
extern "C" {
    static __bss_end: u8;
}

mod bootrepair;
mod arch;
mod bootlog;
mod console;
mod consts;
mod crypto;
mod demo;
mod diag;
mod drivers;
mod font;
mod input;
mod kbd;
mod mm;
mod panic;
mod shell;
mod smbios;
mod task;
mod timer;
pub use kernel_core::runtime;
pub use kernel_core::vfs;

// The arch layer keeps the historical crate::<module> paths for the
// generic core (M9.0); riscv64 gets its own implementations in M9.1.
pub use arch::x86_64::{
    cpu, exceptions, gdt, idt, interrupts, pci, pic, pit, port, serial, syscall, tsc,
};

#[no_mangle]
pub extern "sysv64" fn kmain(boot_info: *const BootInfo) -> ! {
    let bi = unsafe { &*boot_info };

    // Output channel 1: serial (16550 COM1), 115200 8N1.
    let mut s = serial();
    s.init();
    // Shared (kernel-core) diagnostics log through the serial sink.
    kernel_core::log::set_sink(crate::serial::log_bytes);

    // M9.4-2: install the x86 hooks the shared modules need. All of them are
    // safe to install here; the ones with boot-time dependencies resolve
    // lazily (e.g. the clock is 0 until tsc::calibrate).
    drivers::install_hooks();

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
    let _ = writeln!(s, "fantuan v0.0.1");
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
        let _ = writeln!(c, "fantuan v0.0.1");
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

    // M8.5a: PS/2 keyboard on IRQ1. Absent hardware degrades to serial-only.
    if kbd::init() {
        pic::unmask(consts::IRQ_KEYBOARD);
        let _ = writeln!(s, "kbd: IRQ1 unmasked (serial + keyboard input)");
    } else {
        let _ = writeln!(s, "kbd: no PS/2 keyboard (serial-only input)");
    }

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

    // M9.2a: install the arch IRQ hooks for the shared allocator, then
    // initialize it from the memory map (kernel end from the linker script).
    kernel_core::arch::set_irq_ops(cpu::irq_save, cpu::irq_restore);
    let kernel_end_phys = core::ptr::addr_of!(__bss_end) as u64 - fantuan_abi::PHYS_OFFSET;
    mm::frame::init(bi, kernel_end_phys, &[]);
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
    // reclaim() (not free()) because these reserved frames were never handed
    // out by the allocator.
    for i in 0..bi.boot_tables_pages {
        alloc.reclaim(bi.boot_pml4 + i * mm::frame::FRAME_SIZE);
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

    // --- M8.1: crypto known-answer tests (SHA-256 + RSA-2048) --------------
    // Failure does not halt the boot: crypto is only needed to apply
    // operator-provided .auth bundles (the shell can re-run the tests).
    if crypto::selftest(&mut s, false) {
        let _ = writeln!(s, "crypto: KATs ok (sha256 + rsa-2048)");
    } else {
        let _ = writeln!(s, "crypto: KATs FAILED — authenticated variable updates unavailable");
    }

    // --- M3: kernel tasks + syscall ABI ------------------------------------
    let abi = syscall::syscall(syscall::SYS_VERSION, 0, 0, 0, 0, 0);
    let _ = writeln!(s, "syscall: ABI v{} (int 0x60, versioned dispatch)", abi);
    task::init_arch();
    task::init(bi.stack_top);
    task::spawn(demo::demo_1);
    task::spawn(demo::demo_2);
    task::spawn(demo::demo_3);
    let _ = writeln!(s, "sched: 3 kernel demo tasks spawned (quantum 100 ms)");

    // --- M8.3c: memory hardening (NX + SMEP/SMAP) --------------------------
    // NX must be live before the ELF loader sets P_NX entries. Missing
    // features are reported and skipped (the default qemu64 CPU lacks
    // SMEP/SMAP; tools/run.sh passes -cpu max so smoke exercises them).
    let nx = cpu::enable_nxe();
    let smep = cpu::enable_smep();
    let smap = cpu::enable_smap();
    let _ = writeln!(s, "mmu: nx {} smep {} smap {} (unsupported features skipped)", nx, smep, smap);

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

    // --- M5: diagnostics stage 1 (pure Rust core, DESIGN.md §6) ------------
    // M5.5: SMBIOS tables feed the CPU/GPU diagnostics (BIOS/system identity,
    // memory devices, slot list). The entry point lives in the F-segment on
    // legacy firmware; a missing anchor degrades gracefully.
    unsafe {
        smbios::init_from_entry(bi.smbios_table);
        smbios::init(0xF_0000, 0x1_0000);
    }

    let stage1: [diag::Check; 3] = [
        diag::Check { name: "cpu", run: diag::cpu::check },
        diag::Check { name: "gpu", run: diag::gpu::check },
        diag::Check { name: "ram", run: diag::ram::check },
    ];
    diag::run_stage("1 hardware", &stage1);

    // --- M4.5: C driver layer -----------------------------------------------
    let mut mounted: Option<vfs::Vfs> = None;
    if drivers::init() {
        let _ = writeln!(s, "drivers: C layer + AHCI read-only verified");

        // --- M6: VFS + partition table + FAT32 read-only ------------------
        // Mount first: stage 2 reports per-partition filesystem types from
        // the probe table (M5.5) and the ESP bootloaders it found.
        mounted = vfs::init();
        if mounted.is_some() {
            let _ = writeln!(s, "vfs: ok");
        } else {
            let _ = writeln!(s, "vfs: unavailable (boot continues)");
        }

        // --- M5: diagnostics stage 2 (needs the C storage driver) ----------
        let stage2: [diag::Check; 1] = [
            diag::Check { name: "storage", run: diag::storage::check },
        ];
        diag::run_stage("2 storage", &stage2);

        // --- M7: boot repair v1 (read-only diagnosis + repair actions) ----
        // The boot path is READ-ONLY: diagnosis only. Repairs run solely from
        // the shell's confirmation-gated `grub-fix repair` (iron rule).
        if let Some(v) = mounted {
            // Shared boot repair logs through the kernel-core Log sink.
            bootrepair::diagnose(&mut kernel_core::log::Log::new(), &v, bi.runtime_services);
        }
    } else {
        let _ = writeln!(s, "drivers: AHCI unavailable (boot continues)");
    }

    // Boot-complete signal: one LONG beep — distinct from the short-beep
    // diagnostic codes (DESIGN.md §6.2).
    pit::beep_long();
    let _ = writeln!(s, "beep: boot ok (1 long)");

    // --- §10 Minimal shell -------------------------------------------------
    // M8.5b: mirror serial output onto the GOP so the shell is usable on
    // machines without a serial port. Enabled here, after the boot logs.
    if con.is_some() {
        serial::enable_mirror();
        let _ = writeln!(s, "console: serial output mirrored to GOP (keyboard + display)");
    }
    // The interactive loop replaces the idle spin: it halts between polls, so
    // the scheduler keeps running the other tasks exactly as before.
    shell::enter(mounted, bi, bi.runtime_services);
}
