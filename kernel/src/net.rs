//! x86_64 wiring for the kernel-net rump adaptation layer (M11 R2). The
//! adapter owns the NetBSD slice; this module only supplies the host hooks
//! (kernel log, frame allocator, PIT ticks, cooperative sleep) and starts
//! the deferred-softint and self-test tasks.
//!
//! Behaviour without the `rump-selftest` feature: the adapter is brought up
//! and the softint drainer runs, but no self-test task is spawned and no
//! `rump:` markers are printed.

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
    });
    crate::task::spawn(kernel_net::softintd);
    crate::task::spawn(kernel_net::loopback_task);
    #[cfg(feature = "rump-selftest")]
    crate::task::spawn(kernel_net::selftest_task);
}
