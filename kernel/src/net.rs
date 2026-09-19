//! x86_64 wiring for the kernel-net rump adaptation layer (M11 R2). The
//! adapter owns the NetBSD slice; this module only supplies the host hooks
//! (kernel log, frame allocator, PIT ticks, cooperative sleep) and starts
//! the deferred-softint and self-test tasks.
//!
//! Behaviour with `CONFIG_DEBUG_SELFTEST=n`: the adapter is brought up and
//! the softint drainer runs, but no self-test task is spawned and no `rump:`
//! markers are printed.

use core::sync::atomic::Ordering;

use fantuan_abi::PHYS_OFFSET;

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
    crate::pit::beep_n(4, crate::pit::BeepLen::Short);
    crate::bootlog::halt_forever()
}

extern "C" fn pages_alloc(frames: usize) -> *mut u8 {
    match crate::mm::frame::get().alloc_contiguous(frames) {
        Some(phys) => crate::mm::paging::phys_to_virt(phys) as *mut u8,
        None => core::ptr::null_mut(),
    }
}

extern "C" fn pages_free(ptr: *mut u8, frames: usize) {
    if ptr.is_null() {
        return;
    }
    let phys = (ptr as u64).wrapping_sub(PHYS_OFFSET);
    for i in 0..frames as u64 {
        crate::mm::frame::get().free(phys + i * crate::mm::frame::FRAME_SIZE);
    }
}

extern "C" fn ticks() -> u64 {
    crate::timer::ticks()
}

extern "C" fn physmem_pages() -> u64 {
    crate::mm::frame::get().usable_mib() * 256
}

extern "C" fn switch_count() -> u64 {
    crate::task::SWITCHES.load(Ordering::Relaxed)
}

extern "C" fn sleep_ms(ms: u64) {
    kernel_core::task::sleep_ms(ms);
}

extern "C" fn exit() -> ! {
    kernel_core::task::exit(0)
}

extern "C" fn pci_read(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    crate::pci::read32(bus, dev, func, off)
}

extern "C" fn pci_write(bus: u8, dev: u8, func: u8, off: u8, val: u32) {
    crate::pci::write32(bus, dev, func, off, val);
}

extern "C" fn mmio_map(phys: u64, len: u64) -> *mut u8 {
    // map_mmio() rounds down to a 2 MiB page; the hook contract is the
    // virtual address of `phys` itself, so cancel that rounding.
    const HUGE: u64 = 2 * 1024 * 1024;
    match crate::mm::paging::map_mmio(crate::mm::frame::get(), phys, len) {
        Some(base) => (base + (phys & (HUGE - 1))) as *mut u8,
        None => core::ptr::null_mut(),
    }
}

extern "C" fn virt_to_phys(ptr: *const u8) -> u64 {
    (ptr as u64).wrapping_sub(PHYS_OFFSET)
}

/// Called from IRQ0 by the timer; feeds `callout_hardclock`.
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
    crate::task::spawn(kernel_net::softintd);
    crate::task::spawn(kernel_net::loopback_task);
    #[cfg(kconfig_debug_selftest)]
    crate::task::spawn(kernel_net::selftest_task);
}
