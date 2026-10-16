//! fantuan-kernel — boot entry + core bring-up (M0-M8).
//!
//! Boot chain: UEFI firmware -> fantuan-boot (Rust EFI app) -> boot/entry.S ->
//! kmain. The tree currently carries: handshake/serial/GOP console (M0),
//! interrupts + PIT/TSC (M1), higher-half kernel + frame allocator (M2),
//! tasks/scheduler + syscall ABI (M3), ring-3 + ELF loader (M4), the C driver
//! boundary + AHCI (M4.5), diagnostics (M5/M5.5), the VFS (M6), boot repair
//! (M7.x), the built-in shell (§10), and the storage ops registry + NVMe (M8).
//!
//! `kmain` below is a table of contents, not an implementation: the phases
//! live in `boot.rs` (power the machine on) and `wiring.rs` (start tasks and
//! hand the console to the shell). **The call order is the correctness
//! argument** — each phase's doc comment names what it must follow. Both files
//! were split out of this one in 2026-10, when it reached 418 lines against
//! the 300-line rule.

#![no_std]
#![no_main]

use fantuan_abi::BootInfo;

// The first userland program, embedded at build time (tools/build.sh builds
// the user crate first; kernel/build.rs bakes the ELF in). This include must
// stay at the crate root: every phase reads the ELFs as `crate::USER_ELF` and
// friends, so moving it into a module would move all of those with it.
include!(concat!(env!("OUT_DIR"), "/user_program.rs"));

#[cfg(any(kconfig_virt, kconfig_graphics))]
mod acpi;
mod arch;
mod boot;
#[cfg(kconfig_rescue_repair)]
mod bootrepair;
mod bootlog;
#[cfg(kconfig_graphics)]
mod console;
mod consts;
mod crypto;
mod demo;
mod diag;
mod drivers;
#[cfg(kconfig_graphics)]
mod font;
#[cfg(kconfig_graphics)]
mod gfx_demo;
#[cfg(kconfig_graphics)]
mod kms_demo;
mod input;
mod kbd;
mod mm;
#[cfg(kconfig_graphics)]
mod mouse;
#[cfg(kconfig_net)]
mod net;
mod panic;
mod shell;
#[cfg(kconfig_smbios)]
mod smbios;
mod task;
mod timer;
mod wiring;
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

    // Bring the machine up, in an order that does not survive reordering.
    let mut s = boot::phase_core(bi);
    boot::phase_interrupts(&mut s);
    boot::phase_memory(bi, &mut s);
    boot::phase_traps_and_crypto(&mut s);

    // Start the kernel and user tasks.
    wiring::phase_processes(bi, &mut s);

    // NX must be live before the ELF loader maps anything with P_NX.
    boot::phase_hardening(&mut s);
    wiring::phase_userland(&mut s);

    // Read-only hardware reporting, then the storage stack it needs.
    boot::report::phase_diagnostics(bi);
    let mounted = boot::report::phase_storage(bi, &mut s);

    // Never returns.
    wiring::enter_shell(bi, mounted)
}
