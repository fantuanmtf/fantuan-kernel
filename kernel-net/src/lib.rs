//! kernel-net - the fantuan adaptation layer for the NetBSD rump slice
//! (M11 R2). The imported NetBSD sources plus the C shim live in this
//! crate's build archive; this module owns the Rust<->C hook surface and
//! the two kernel tasks that drive softints and the boot self-test.
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
};

static READY: AtomicBool = AtomicBool::new(false);

fn env() -> Env {
    unsafe { core::ptr::addr_of!(ENV).read() }
}

extern "C" {
    fn rump_shim_init();
    fn rump_shim_tick();
    fn rump_softint_dispatch();
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
