//! kernel-net - the fantuan adaptation layer for the NetBSD rump slice
//! (M11 R2-R4). The imported NetBSD sources plus the C shim live in this
//! crate's build archive; this module owns the Rust<->C hook surface and
//! the kernel tasks that drive softints, the IPv4 boot tests and the
//! self-test.
//!
//! No kernel-core dependency: the kernel installs an `Env` of callbacks
//! (log, frame pages, PIT ticks, cooperative sleep) before `init()`.

#![no_std]

use core::sync::atomic::{AtomicBool, Ordering};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Env {
    pub log: extern "C" fn(*const u8, usize),
    pub panic: extern "C" fn(*const u8, usize) -> !,
    pub pages_alloc: extern "C" fn(usize) -> *mut u8,
    pub pages_free: extern "C" fn(*mut u8, usize),
    pub ticks: extern "C" fn() -> u64,
    pub physmem_pages: extern "C" fn() -> u64,
    pub switch_count: extern "C" fn() -> u64,
    pub sleep_ms: extern "C" fn(u64),
    pub exit: extern "C" fn() -> !,
    /// PCI config space (type-1 CF8/CFC) for the e1000 probe.
    pub pci_read: extern "C" fn(u8, u8, u8, u8) -> u32,
    pub pci_write: extern "C" fn(u8, u8, u8, u8, u32),
    /// Map an MMIO range (cache-disabled) and return its virtual base.
    pub mmio_map: extern "C" fn(u64, u64) -> *mut u8,
    /// Physical address behind a frame-allocator virtual pointer.
    pub virt_to_phys: extern "C" fn(*const u8) -> u64,
}

extern "C" fn stub_log(_: *const u8, _: usize) {}
extern "C" fn stub_panic(_: *const u8, _: usize) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
extern "C" fn stub_pages_alloc(_: usize) -> *mut u8 {
    core::ptr::null_mut()
}
extern "C" fn stub_pages_free(_: *mut u8, _: usize) {}
extern "C" fn stub_ticks() -> u64 {
    0
}
extern "C" fn stub_physmem_pages() -> u64 {
    0
}
extern "C" fn stub_switch_count() -> u64 {
    0
}
extern "C" fn stub_sleep_ms(_: u64) {}
extern "C" fn stub_exit() -> ! {
    loop {
        core::hint::spin_loop();
    }
}
extern "C" fn stub_pci_read(_: u8, _: u8, _: u8, _: u8) -> u32 {
    0xFFFF_FFFF
}
extern "C" fn stub_pci_write(_: u8, _: u8, _: u8, _: u8, _: u32) {}
extern "C" fn stub_mmio_map(_: u64, _: u64) -> *mut u8 {
    core::ptr::null_mut()
}
extern "C" fn stub_virt_to_phys(_: *const u8) -> u64 {
    0
}

static mut ENV: Env = Env {
    log: stub_log,
    panic: stub_panic,
    pages_alloc: stub_pages_alloc,
    pages_free: stub_pages_free,
    ticks: stub_ticks,
    physmem_pages: stub_physmem_pages,
    switch_count: stub_switch_count,
    sleep_ms: stub_sleep_ms,
    exit: stub_exit,
    pci_read: stub_pci_read,
    pci_write: stub_pci_write,
    mmio_map: stub_mmio_map,
    virt_to_phys: stub_virt_to_phys,
};

static READY: AtomicBool = AtomicBool::new(false);

fn env() -> Env {
    unsafe { core::ptr::addr_of!(ENV).read() }
}

extern "C" {
    fn rump_shim_init();
    fn rump_shim_tick();
    fn rump_softint_dispatch();
    fn rump_net_poll() -> i32;
    fn rump_selftest_poll() -> i32;
}

/// Bring up the imported slice; call once, after the frame allocator and
/// the scheduler are running.
pub fn init(env: Env) {
    unsafe { core::ptr::addr_of_mut!(ENV).write(env) };
    unsafe { rump_shim_init() };
    READY.store(true, Ordering::Release);
}

/// Drive the callout wheel from the timer interrupt; no-op before init.
pub fn tick() {
    if !READY.load(Ordering::Acquire) {
        return;
    }
    unsafe { rump_shim_tick() };
}

/// Deferred softint drainer: callout callbacks run here, in task context.
pub fn softintd() -> ! {
    loop {
        unsafe { rump_softint_dispatch() };
        (env().sleep_ms)(1);
    }
}

/// Loopback ping task: drains the packet queue and runs the ping state
/// machine until it reports completion.
pub fn loopback_task() -> ! {
    loop {
        if unsafe { rump_net_poll() } != 0 {
            (env().exit)();
        }
        (env().sleep_ms)(10);
    }
}

/// Bounded boot self-test task; exits when the poll reports completion.
pub fn selftest_task() -> ! {
    loop {
        if unsafe { rump_selftest_poll() } != 0 {
            (env().exit)();
        }
        (env().sleep_ms)(10);
    }
}

#[no_mangle]
pub extern "C" fn fantuan_rump_log(buf: *const u8, len: usize) {
    (env().log)(buf, len);
}

#[no_mangle]
pub extern "C" fn fantuan_rump_panic(buf: *const u8, len: usize) -> ! {
    (env().panic)(buf, len)
}

#[no_mangle]
pub extern "C" fn fantuan_rump_pages_alloc(n: usize) -> *mut u8 {
    (env().pages_alloc)(n)
}

#[no_mangle]
pub extern "C" fn fantuan_rump_pages_free(p: *mut u8, n: usize) {
    (env().pages_free)(p, n);
}

#[no_mangle]
pub extern "C" fn fantuan_rump_ticks() -> u64 {
    (env().ticks)()
}

#[no_mangle]
pub extern "C" fn fantuan_rump_physmem_pages() -> u64 {
    (env().physmem_pages)()
}

#[no_mangle]
pub extern "C" fn fantuan_rump_switch_count() -> u64 {
    (env().switch_count)()
}

#[no_mangle]
pub extern "C" fn fantuan_rump_pci_read(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    (env().pci_read)(bus, dev, func, off)
}

#[no_mangle]
pub extern "C" fn fantuan_rump_pci_write(bus: u8, dev: u8, func: u8, off: u8, val: u32) {
    (env().pci_write)(bus, dev, func, off, val);
}

#[no_mangle]
pub extern "C" fn fantuan_rump_mmio_map(phys: u64, len: u64) -> *mut u8 {
    (env().mmio_map)(phys, len)
}

#[no_mangle]
pub extern "C" fn fantuan_rump_virt_to_phys(ptr: *const u8) -> u64 {
    (env().virt_to_phys)(ptr)
}
