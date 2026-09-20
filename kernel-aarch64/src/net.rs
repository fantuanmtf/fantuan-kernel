//! aarch64 wiring for the kernel-net rump adaptation layer (M11 R9b). The
//! adapter owns the NetBSD slice; this module supplies the host hooks (kernel
//! log, frame allocator through the direct map, generic-timer ticks,
//! cooperative sleep) and starts the softint/net tasks. `mmio_map` maps the
//! MMIO window identity (TTBR0) on demand, which is where the polled
//! virtio-net driver finds the QEMU virt slots at 0x0a000000; PCI hooks are
//! stubs because the direct-FDT machine has no PCI.
//!
//! Behaviour with `CONFIG_DEBUG_SELFTEST=n`: the adapter is brought up and
//! the softint drainer runs, but no self-test task is spawned and no `rump:`
//! markers are printed.

use core::sync::atomic::Ordering;

use crate::{mmu, park};

extern "C" fn log(ptr: *const u8, len: usize) {
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    kernel_core::log::put(bytes);
    kernel_core::log::put(b"\n");
}

extern "C" fn panic(ptr: *const u8, len: usize) -> ! {
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    kernel_core::log::put(b"PANIC: ");
    kernel_core::log::put(bytes);
    kernel_core::log::put(b"\n");
    park()
}

extern "C" fn pages_alloc(frames: usize) -> *mut u8 {
    match kernel_core::frame::get().alloc_contiguous(frames) {
        Some(phys) => mmu::phys_to_virt(phys) as *mut u8,
        None => core::ptr::null_mut(),
    }
}

extern "C" fn pages_free(ptr: *mut u8, frames: usize) {
    if ptr.is_null() {
        return;
    }
    let phys = (ptr as u64).wrapping_sub(mmu::PHYS_OFFSET);
    for i in 0..frames as u64 {
        kernel_core::frame::get().free(phys + i * kernel_core::frame::FRAME_SIZE);
    }
}

extern "C" fn ticks() -> u64 {
    crate::timer::ticks()
}

extern "C" fn physmem_pages() -> u64 {
    kernel_core::frame::get().usable_mib() * 256
}

extern "C" fn switch_count() -> u64 {
    kernel_core::task::SWITCHES.load(Ordering::Relaxed)
}

extern "C" fn sleep_ms(ms: u64) {
    kernel_core::task::sleep_ms(ms);
}

extern "C" fn exit() -> ! {
    kernel_core::task::exit(0)
}

extern "C" fn pci_read(_bus: u8, _dev: u8, _func: u8, _off: u8) -> u32 {
    0xFFFF_FFFF
}

extern "C" fn pci_write(_bus: u8, _dev: u8, _func: u8, _off: u8, _val: u32) {}

/// Map the 2 MiB block containing PHYS identity and return the physical
/// address (the kernel runs identity-mapped, so it is directly usable).
extern "C" fn mmio_map(phys: u64, _len: u64) -> *mut u8 {
    mmu::map_mmio(phys);
    mmu::flush_tlb();
    phys as *mut u8
}

extern "C" fn virt_to_phys(ptr: *const u8) -> u64 {
    (ptr as u64).wrapping_sub(mmu::PHYS_OFFSET)
}

/// Called from the generic-timer IRQ; feeds `callout_hardclock`.
pub fn tick() {
    kernel_net::tick();
}

/// Bring up the adapter and start its tasks; called after task::init.
pub fn init() {
    kernel_net::init(kernel_net::Env {
        log,
        panic,
        pages_alloc,
        pages_free,
        ticks,
        physmem_pages,
        switch_count,
        sleep_ms,
        exit,
        pci_read,
        pci_write,
        mmio_map,
        virt_to_phys,
    });
    kernel_core::task::spawn(kernel_net::softintd);
    kernel_core::task::spawn(kernel_net::loopback_task);
    #[cfg(kconfig_debug_selftest)]
    kernel_core::task::spawn(kernel_net::selftest_task);
}
