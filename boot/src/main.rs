//! fantuan-boot — self-written UEFI bootloader (M0).
//!
//! Hand-rolled against the UEFI 2.x spec; zero external crates. efi_main only
//! orchestrates; the work lives in the modules:
//!
//!   uefi/       types, GUIDs, SystemTable/BootServices, protocol layouts
//!   console     ConOut text output        serial   post-exit debug channel
//!   memory      memory-map snapshot       loader   kernel image load
//!   bootinfo    BOOT_INFO + exit + jump
//!
//! Flow: GOP -> RSDP -> memory map -> stack -> kernel image ->
//! ExitBootServices -> BootInfo -> jump to the kernel at 16 MiB.

#![no_std]
#![no_main]

mod bootinfo;
mod console;
mod loader;
mod memory;
mod paging;
mod serial;
mod uefi;

use core::panic::PanicInfo;

use uefi::protocol;
use uefi::table::{self, SystemTable};
use uefi::{
    Handle, Status, EFI_BOOT_SERVICES_SIGNATURE, EFI_LOAD_ERROR, EFI_SYSTEM_TABLE_SIGNATURE,
};

#[allow(private_interfaces)]
#[no_mangle]
pub extern "efiapi" fn efi_main(image_handle: Handle, system_table: *mut SystemTable) -> Status {
    let st = unsafe { &*system_table };
    if st.hdr.signature != EFI_SYSTEM_TABLE_SIGNATURE {
        return EFI_LOAD_ERROR;
    }
    let con = st.con_out;
    unsafe { ((*con).clear_screen)(con); }

    console::println(con, "fantuan-boot v0.1 (M0)");
    console::println(con, "self-written UEFI bootloader");

    let bs = unsafe { &*st.boot_services };
    if bs.hdr.signature != EFI_BOOT_SERVICES_SIGNATURE {
        console::println(con, "ERROR: bad boot services signature");
        return EFI_LOAD_ERROR;
    }

    // 1. Display + ACPI (pure queries; reporting lives here)
    let fb = match protocol::locate_gop(bs) {
        Some(fb) => {
            console::println_hex(con, "gop: base=", fb.base);
            let mut buf = [0u16; 256];
            let mut off = 0;
            console::write_ascii(&mut buf, &mut off, "gop: ");
            console::write_dec(&mut buf, &mut off, fb.width as u64);
            console::write_ascii(&mut buf, &mut off, "x");
            console::write_dec(&mut buf, &mut off, fb.height as u64);
            console::write_ascii(&mut buf, &mut off, " stride=");
            console::write_dec(&mut buf, &mut off, fb.stride as u64);
            console::write_ascii(&mut buf, &mut off, " format=");
            console::write_dec(&mut buf, &mut off, fb.format as u64);
            console::output_line(con, &mut buf, off);
            fb
        }
        None => {
            console::println(con, "gop: not available (headless?) — serial-only console");
            protocol::FrameBufferInfo::none()
        }
    };
    let rsdp = table::locate_rsdp(st);
    console::println_hex(con, "rsdp: ", rsdp);

    // 2. Memory-map snapshot (before any allocation changes the key)
    let Some(mut map) = memory::snapshot(bs, con) else {
        return EFI_LOAD_ERROR;
    };

    // 3. Initial kernel stack, allocated below the kernel image
    let Some(stack_top) = memory::allocate_stack(bs, con, loader::KERNEL_ADDR) else {
        return EFI_LOAD_ERROR;
    };

    // 4. Kernel image from the ESP
    if loader::load_kernel(bs, con, &map).is_err() {
        return EFI_LOAD_ERROR;
    }

    // 4.5 Initial page tables (must be allocated BEFORE ExitBootServices,
    // and below the kernel image like the stack)
    let Some(tables) = paging::allocate_tables(bs, con, loader::KERNEL_ADDR) else {
        return EFI_LOAD_ERROR;
    };

    // 4.6 Runtime services survive the exit; the kernel drives its own
    // NVRAM self-test through them (M7.5).
    let runtime_services = st.runtime_services as u64;

    // 5. Exit boot services, enable paging, hand over, jump — never returns
    bootinfo::exit_and_jump(
        bs,
        con,
        image_handle,
        fb,
        rsdp,
        stack_top,
        loader::KERNEL_ADDR,
        &mut map,
        &tables,
        runtime_services,
    );
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // ConOut may already be gone; just halt.
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)) }
    }
}
