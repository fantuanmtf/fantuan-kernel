//! kernel-net - fantuan's NetBSD rump adaptation layer (M11 R2-R7).  The
//! imported sources plus the C shim live in this crate's build archive; this
//! module owns the Rust<->C surface and the kernel tasks.  No kernel-core
//! dependency: the kernel installs an `Env` of callbacks before `init()`.
#![no_std]

use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};

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
    fn rump_dns_default_server() -> u32;
    fn rump_tool_begin_dns(name: *const u8, len: usize, server: u32, port: u16);
    fn rump_tool_begin_ping(addr: u32);
    fn rump_tool_begin_wget(host: *const u8, hostlen: usize, addr: u32, port: u16, lport: u16);
    fn rump_tool_status() -> i32;
    fn rump_tool_error() -> *const u8;
    fn rump_tool_result_addr() -> u32;
    fn rump_tool_result_rtt() -> i32;
    fn rump_tool_result_http() -> i32;
    fn rump_tool_result_bytes() -> usize;
    fn rump_tool_result_hash() -> u32;
    fn rump_net_selftest_busy() -> i32;
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

/// Network poll task: drains the driver RX ring and every packet queue and
/// runs the boot test state machine.  Stays alive after the boot sequence
/// (returning 1) because the R7 shell tools share the same stack: they need
/// the queues drained while they poll their own clients.
pub fn loopback_task() -> ! {
    loop {
        let _ = unsafe { rump_net_poll() };
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

// --- M11 R7: shell tool clients: the request slot (rump_toolreq.c) ------
// is stepped by the net task; the shell only waits (cooperative sleeps).
fn wait_selftest() {
    let start = (env().ticks)();
    while unsafe { rump_net_selftest_busy() } != 0 {
        if (env().ticks)().wrapping_sub(start) > 6000 {
            break;
        }
        (env().sleep_ms)(100);
    }
}
fn cstr(ptr: *const u8) -> &'static str {
    if ptr.is_null() {
        return "unknown";
    }
    let mut n = 0usize;
    while unsafe { *ptr.add(n) } != 0 {
        n += 1;
    }
    let bytes: &'static [u8] = unsafe { core::slice::from_raw_parts(ptr, n) };
    core::str::from_utf8(bytes).unwrap_or("unknown")
}

/// Wait for the net task to finish the request (safety net only).
fn run_request(timeout_ticks: u64) -> Result<(), &'static str> {
    let start = (env().ticks)();
    loop {
        match unsafe { rump_tool_status() } {
            0 => {
                if (env().ticks)().wrapping_sub(start) > timeout_ticks {
                    return Err("timeout");
                }
                (env().sleep_ms)(100);
            }
            1 => return Ok(()),
            _ => return Err(cstr(unsafe { rump_tool_error() })),
        }
    }
}

/// DHCP-provided resolver address (host byte order); 0 without a lease.
pub fn dns_default_server() -> u32 {
    unsafe { rump_dns_default_server() }
}

/// Resolve NAME (A record) against SERVER:PORT (host byte order address).
pub fn dns_lookup(name: &[u8], server: u32, port: u16) -> Result<u32, &'static str> {
    if name.is_empty() || name.len() > 63 {
        return Err("name");
    }
    if server == 0 {
        return Err("no server");
    }
    wait_selftest();
    unsafe { rump_tool_begin_dns(name.as_ptr(), name.len(), server, port) };
    run_request(3000)?;
    Ok(unsafe { rump_tool_result_addr() })
}

/// One ICMP echo to ADDR (host byte order); returns the RTT in PIT ticks.
pub fn ping_once(addr: u32) -> Result<u32, &'static str> {
    wait_selftest();
    unsafe { rump_tool_begin_ping(addr) };
    run_request(3000)?;
    Ok(unsafe { rump_tool_result_rtt() } as u32)
}

/// HTTP GET `http://HOST:PORT/` at the already-resolved ADDR; returns
/// (status, body bytes, FNV-1a hash).  The local port walks a small range
/// so back-to-back shell invocations do not collide in TIME_WAIT.
pub fn wget(host: &[u8], addr: u32, port: u16) -> Result<(i32, usize, u32), &'static str> {
    if host.is_empty() || host.len() > 63 {
        return Err("host");
    }
    static NEXT_LPORT: AtomicU16 = AtomicU16::new(0);
    wait_selftest();
    let lport = 40002 + NEXT_LPORT.fetch_add(1, Ordering::Relaxed) % 80;
    unsafe { rump_tool_begin_wget(host.as_ptr(), host.len(), addr, port, lport) };
    run_request(3000)?;
    Ok((
        unsafe { rump_tool_result_http() },
        unsafe { rump_tool_result_bytes() },
        unsafe { rump_tool_result_hash() },
    ))
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
