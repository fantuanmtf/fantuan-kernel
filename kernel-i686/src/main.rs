//! kernel-i686 (M10): 32-bit x86 bring-up from the self-written BIOS chain.
//! Entry contract (stage2): flat protected mode, paging on (identity + the
//! 0xC0000000 alias), `_start` called with the BootInfo physical pointer on
//! the stack, esp = 0x80000. Paging, interrupts and scheduling arrive in the
//! following M10-4 steps; this brings the handoff, memmap and the shared
//! frame allocator up on 32-bit.

#![no_std]
#![no_main]

use core::arch::asm;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::addr_of_mut;

use fantuan_abi::{BootInfo, BOOT_MAGIC, BOOT_VERSION, PHYS_OFFSET};

mod ata;
mod cpu;
mod fb;
mod idt;
mod pic;
mod pit;
mod demo;
mod gdt;
mod serial;
mod task;
mod user;

core::arch::global_asm!(
    ".section .text.entry",
    ".global _start",
    "_start:",
    "  cld",                // firmware may leave DF=1 for compiler rep movsb
    "  mov dx, 0x3F8",
    "  mov al, 0x58",       // 'X': entry reached
    "  out dx, al",
    "  jmp rust_entry",
);

extern "C" {
    static mut __bss_start: u8;
    static mut __bss_end: u8;
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    serial::puts("PANIC: kernel-i686\n");
    cpu::halt()
}

#[no_mangle]
#[link_section = ".text.start"]
pub extern "C" fn rust_entry(bi: *const BootInfo) -> ! {
    // objcopy -O binary drops NOBITS: zero .bss before any Rust static is used.
    unsafe {
        let s = addr_of_mut!(__bss_start) as usize;
        let e = addr_of_mut!(__bss_end) as usize;
        let mut p = s;
        while p < e {
            *(p as *mut u8) = 0;
            p += 1;
        }
    }
    serial::init();
    kernel_core::log::set_sink(serial::log_bytes);
    kmain(bi)
}

fn phys_to_virt(p: u64) -> u64 {
    PHYS_OFFSET + p
}

fn kmain(bi: *const BootInfo) -> ! {
    let bi = unsafe { &*bi };
    // M10-5: bring the VBE console up first and mirror serial to it, so the
    // banner and every milestone below are visible on both channels.
    if fb::init(bi) {
        serial::set_mirror(fb::putc);
    }
    serial::puts("\nfantuan v0.0.2 (i686) - BIOS handoff\n");

    if bi.magic != BOOT_MAGIC || bi.version != BOOT_VERSION {
        serial::puts("fatal: bad handshake\n");
        cpu::halt();
    }
    serial::puts("handshake ok: arch=");
    serial::put_dec(bi.arch as u64);
    serial::puts(" kernel_base=");
    serial::put_hex(bi.kernel_base);
    serial::puts(" stack_top=");
    serial::put_hex(bi.stack_top);
    serial::puts("\n");

    serial::puts("memmap: ");
    serial::put_dec(bi.memmap.count as u64);
    serial::puts(" descriptors\n");
    for i in 0..bi.memmap.count {
        let d = unsafe { &*bi.memmap.ptr.add(i) };
        serial::puts("  base=");
        serial::put_hex(d.physical_start);
        serial::puts(" pages=");
        serial::put_dec(d.number_of_pages);
        serial::puts(" type=");
        serial::put_dec(d.type_ as u64);
        serial::puts("\n");
    }

    kernel_core::arch::set_irq_ops(cpu::irq_save, cpu::irq_restore);
    kernel_core::mem::set_phys_to_virt(phys_to_virt);
    let kernel_end_phys = addr_of_mut!(__bss_end) as u64 - PHYS_OFFSET;
    kernel_core::frame::init(bi, kernel_end_phys, &[]);
    serial::puts("mm: frame allocator ready: ");
    serial::put_dec(kernel_core::frame::get().usable_mib());
    serial::puts(" MiB usable\n");

    if let Some(f) = kernel_core::frame::get().alloc() {
        let probe = phys_to_virt(f) as *mut u64;
        unsafe { probe.write_volatile(0x0BAD_F00D) };
        let ok = unsafe { probe.read_volatile() } == 0x0BAD_F00D;
        kernel_core::frame::get().free(f);
        serial::puts(if ok {
            "mm: frame self-test ok (via PHYS_OFFSET alias)\n"
        } else {
            "mm: frame self-test FAILED\n"
        });
    }

    // --- M10-4c: PIO ATA + the shared VFS on the test disk (read-only) ------
    if ata::init() {
        ata::install_hooks();
        let mut s = kernel_core::log::Log::new();
        let _ = writeln!(s, "ata: primary master ready, {} sectors", ata::sectors());
        match kernel_core::vfs::init() {
            Some(vfs) => {
                let _ = writeln!(s, "vfs: ok");
                let stage2: [kernel_core::diag::Check; 1] = [kernel_core::diag::Check {
                    name: "storage",
                    run: kernel_core::diag::storage::check,
                }];
                kernel_core::diag::run_stage("2 storage", &stage2);
                kernel_core::bootrepair::diagnose(&mut s, &vfs, bi.runtime_services);
            }
            None => {
                let _ = writeln!(s, "vfs: unavailable (boot continues)");
            }
        }
    } else {
        serial::puts("ata: no primary master (VFS skipped)\n");
    }

    // --- M10-4b3: GDT/TSS (needed before the ring-3 syscall gate) -----------
    gdt::init();
    serial::puts("gdt: ring0/ring3 + TSS loaded\n");

    // --- M10-4b2a: interrupts ------------------------------------------------
    idt::init();
    serial::puts("idt: 48 vectors + int 0x80\n");
    pic::remap();
    serial::puts("pic: remapped 0x20/0x28, IRQ0 unmasked\n");
    pit::init(100);
    serial::puts("timer: PIT 100 Hz\n");

    // Recoverable exception demo (the handler resumes after int3), then enable
    // interrupts for the PIT heartbeat.
    unsafe { asm!("int3") };
    serial::puts("demo: #BP handled and resumed\n");
    task::init_arch();
    kernel_core::task::init(0x80000);
    task::spawn_demos();
    match user::spawn_user(user::USER_ELF) {
        Some(tid) => {
            serial::puts("user: elf32 userland spawned as tid ");
            serial::put_dec(tid);
            serial::puts("\n");
        }
        None => serial::puts("user: ELF32 userland load failed\n"),
    }
    match user::spawn_fault_stub() {
        Some(tid) => {
            serial::puts("user: fault stub spawned as tid ");
            serial::put_dec(tid);
            serial::puts("\n");
        }
        None => serial::puts("user: fault stub spawn failed\n"),
    }

    cpu::sti();
    serial::puts("interrupts: enabled\n");

    // Let the PIT tick for ~5 s; hlt keeps the CPU idle between interrupts.
    while pit::ticks() < 500 {
        unsafe { asm!("hlt", options(nomem, nostack)) };
    }
    serial::puts("timer: 500 ticks, exceptions=");
    serial::put_dec(idt::exception_count());
    serial::puts("\n");
    serial::puts("i686: M10-4b2a interrupts complete\n");
    cpu::halt()
}
