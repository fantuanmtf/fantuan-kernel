//! TLS glue for kernel-net (M11 R8).  Owns the Rust side of the mbedTLS
//! client: the build-time generated pinned CA and fixture switch, the boot
//! init/KAT hook and the HTTPS request slot used by the shell `wget`.
#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/tls_extra.rs"));

#[cfg(kconfig_tls)]
use core::sync::atomic::{AtomicU16, Ordering};

#[cfg(kconfig_tls)]
extern "C" {
    fn rump_tls_init() -> i32;
    fn rump_tls_kat() -> i32;
    fn rump_tool_begin_wget_tls(
        host: *const u8,
        hostlen: usize,
        addr: u32,
        port: u16,
        lport: u16,
        verify: i32,
    );
    fn rump_tool_result_http() -> i32;
    fn rump_tool_result_bytes() -> usize;
    fn rump_tool_result_hash() -> u32;
}

/// Build-time offline-fixture switch (FANTUAN_NET_FIXTURES=1 in the smoke).
#[no_mangle]
pub extern "C" fn rump_net_fixtures() -> i32 {
    if NET_FIXTURES {
        1
    } else {
        0
    }
}

/// Pinned CA for the C platform glue (rump_tls.c); empty when absent.
#[cfg(kconfig_tls)]
#[no_mangle]
pub extern "C" fn rump_tls_ca_der(len: *mut usize) -> *const u8 {
    if !len.is_null() {
        unsafe { *len = TLS_CA_DER.len() };
    }
    TLS_CA_DER.as_ptr()
}

/// Seed the CSPRNG, install the platform glue, parse the pinned CA, then run
/// the boot KATs (the `tls:` marker is printed by the C side).
#[cfg(kconfig_tls)]
pub fn init() {
    if unsafe { rump_tls_init() } == 0 {
        let _ = unsafe { rump_tls_kat() };
    }
}

/// HTTPS GET `https://HOST:PORT/` at the already-resolved ADDR; returns
/// (status, body bytes, FNV-1a hash).  VERIFY checks the pinned CA.
#[cfg(kconfig_tls)]
pub fn wget_https(
    host: &[u8],
    addr: u32,
    port: u16,
    verify: bool,
) -> Result<(i32, usize, u32), &'static str> {
    if host.is_empty() || host.len() > 63 {
        return Err("host");
    }
    static NEXT_LPORT: AtomicU16 = AtomicU16::new(0);
    crate::wait_selftest();
    let lport = 42002 + NEXT_LPORT.fetch_add(1, Ordering::Relaxed) % 80;
    unsafe {
        rump_tool_begin_wget_tls(
            host.as_ptr(),
            host.len(),
            addr,
            port,
            lport,
            verify as i32,
        )
    };
    crate::run_request(3000)?;
    Ok((
        unsafe { rump_tool_result_http() },
        unsafe { rump_tool_result_bytes() },
        unsafe { rump_tool_result_hash() },
    ))
}
