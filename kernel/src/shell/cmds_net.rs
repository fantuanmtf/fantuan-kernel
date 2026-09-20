//! x86 network tools (M11 R7): ping / nslookup / wget glue over the
//! kernel-net clients.  The DNS wire format, the ICMP echo path and the
//! HTTP GET all live in kernel-net; these handlers only parse arguments,
//! resolve names through the DHCP (or explicit) resolver and format the
//! result for the shell.  Gated behind CONFIG_TOOLS + CONFIG_NET.

use core::fmt::Write;

use kernel_core::log::Log;
use kernel_core::shell::Shell;

#[cfg(kconfig_net)]
macro_rules! out {
    ($s:expr, $($arg:tt)*) => {
        { let _ = writeln!($s, $($arg)*); }
    };
}

#[cfg(not(kconfig_net))]
fn not_built(s: &mut Log, name: &str) {
    let _ = writeln!(s, "{}: not built (CONFIG_NET=n)", name);
}

#[cfg(kconfig_net)]
fn display(bytes: &[u8]) -> &str {
    core::str::from_utf8(bytes).unwrap_or("?")
}

#[cfg(kconfig_net)]
fn parse_ip(tok: &[u8]) -> Option<u32> {
    let mut oct = [0u8; 4];
    let mut n = 0usize;
    let mut start = 0usize;
    for i in 0..=tok.len() {
        if i == tok.len() || tok[i] == b'.' {
            if n >= 4 || i == start || i - start > 3 {
                return None;
            }
            let mut v = 0u32;
            for &b in &tok[start..i] {
                if !b.is_ascii_digit() {
                    return None;
                }
                v = v * 10 + (b - b'0') as u32;
            }
            if v > 255 {
                return None;
            }
            oct[n] = v as u8;
            n += 1;
            start = i + 1;
        }
    }
    if n != 4 {
        return None;
    }
    Some(((oct[0] as u32) << 24) | ((oct[1] as u32) << 16) | ((oct[2] as u32) << 8) | oct[3] as u32)
}

#[cfg(kconfig_net)]
fn parse_dec(tok: &[u8]) -> Option<u32> {
    if tok.is_empty() || tok.len() > 5 {
        return None;
    }
    let mut v = 0u32;
    for &b in tok {
        if !b.is_ascii_digit() {
            return None;
        }
        v = v * 10 + (b - b'0') as u32;
    }
    Some(v)
}

/// `addr[:port]`, the port defaulting to PORT.  The address must be a
/// dotted quad (the R7 tools take a host for DNS and a server literal).
#[cfg(kconfig_net)]
fn parse_host_port(tok: &[u8], port: u16) -> Option<(u32, u16)> {
    let (host, p) = match tok.iter().rposition(|&b| b == b':') {
        Some(i) => (&tok[..i], Some(&tok[i + 1..])),
        None => (tok, None),
    };
    let addr = parse_ip(host)?;
    let port = match p {
        Some(t) => {
            let v = parse_dec(t)?;
            if v == 0 || v > 65535 {
                return None;
            }
            v as u16
        }
        None => port,
    };
    Some((addr, port))
}

/// Resolve HOST (literal or DNS name) through the DHCP resolver.
#[cfg(kconfig_net)]
fn resolve(host: &[u8]) -> Result<u32, &'static str> {
    if let Some(addr) = parse_ip(host) {
        return Ok(addr);
    }
    let server = kernel_net::dns_default_server();
    if server == 0 {
        return Err("no DNS server (no DHCP lease)");
    }
    kernel_net::dns_lookup(host, server, 53)
}

#[cfg(kconfig_net)]
pub fn cmd_nslookup(_sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    let Some(name) = args.first() else {
        out!(s, "usage: nslookup <name> [server[:port]]");
        return;
    };
    let (server, port) = match args.get(1) {
        Some(tok) => match parse_host_port(tok, 53) {
            Some(v) => v,
            None => {
                out!(s, "nslookup: bad server '{}'", display(tok));
                return;
            }
        },
        None => {
            let server = kernel_net::dns_default_server();
            if server == 0 {
                out!(s, "nslookup: no DNS server (no DHCP lease; pass server[:port])");
                return;
            }
            (server, 53)
        }
    };
    match kernel_net::dns_lookup(name, server, port) {
        Ok(addr) => out!(
            s,
            "nslookup: {} => {}.{}.{}.{}",
            display(name),
            (addr >> 24) & 0xff,
            (addr >> 16) & 0xff,
            (addr >> 8) & 0xff,
            addr & 0xff
        ),
        Err(step) => out!(s, "nslookup: {} FAILED ({})", display(name), step),
    }
}

#[cfg(kconfig_net)]
pub fn cmd_ping(_sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    let Some(host) = args.first() else {
        out!(s, "usage: ping <host> [count] (count 1-5)");
        return;
    };
    let count = match args.get(1) {
        Some(tok) => match parse_dec(tok) {
            Some(v) if (1..=5).contains(&v) => v,
            _ => {
                out!(s, "ping: count must be 1-5");
                return;
            }
        },
        None => 1,
    };
    let addr = match resolve(host) {
        Ok(a) => a,
        Err(step) => {
            out!(s, "ping: {} FAILED ({})", display(host), step);
            return;
        }
    };
    for seq in 1..=count {
        match kernel_net::ping_once(addr) {
            Ok(rtt) => out!(
                s,
                "ping: {} ({}.{}.{}.{}) seq={} rtt={} ticks",
                display(host),
                (addr >> 24) & 0xff,
                (addr >> 16) & 0xff,
                (addr >> 8) & 0xff,
                addr & 0xff,
                seq,
                rtt
            ),
            Err(step) => {
                out!(s, "ping: {} FAILED ({})", display(host), step);
                break;
            }
        }
    }
}

#[cfg(kconfig_net)]
pub fn cmd_wget(_sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    let Some(url) = args.first() else {
        out!(s, "usage: wget http://host[:port]/ (root path only)");
        return;
    };
    let Some(rest) = url.strip_prefix(b"http://") else {
        out!(s, "wget: only http:// URLs are supported");
        return;
    };
    let (hostport, path) = match rest.iter().position(|&b| b == b'/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, &b"/"[..]),
    };
    if hostport.is_empty() || path != b"/" {
        out!(s, "wget: usage: wget http://host[:port]/ (root path only)");
        return;
    }
    // The URL host without its optional :port, for the request Host header
    // and the printed URL.
    let host_end = hostport.iter().rposition(|&b| b == b':').unwrap_or(hostport.len());
    let (host, addr, port) = match parse_host_port(hostport, 80) {
        Some((addr, port)) => (&hostport[..host_end], addr, port),
        None => match resolve(hostport) {
            Ok(addr) => (hostport, addr, 80),
            Err(step) => {
                out!(s, "wget: {} FAILED ({})", display(hostport), step);
                return;
            }
        },
    };
    match kernel_net::wget(host, addr, port) {
        Ok((status, bytes, hash)) => out!(
            s,
            "wget: http://{}:{}/ {} bytes={} hash={:08x}",
            display(host),
            port,
            status,
            bytes,
            hash
        ),
        Err(step) => out!(s, "wget: {} FAILED ({})", display(host), step),
    }
}

#[cfg(not(kconfig_net))]
pub fn cmd_nslookup(_sh: &mut Shell, s: &mut Log, _args: &[&[u8]]) {
    not_built(s, "nslookup");
}

#[cfg(not(kconfig_net))]
pub fn cmd_ping(_sh: &mut Shell, s: &mut Log, _args: &[&[u8]]) {
    not_built(s, "ping");
}

#[cfg(not(kconfig_net))]
pub fn cmd_wget(_sh: &mut Shell, s: &mut Log, _args: &[&[u8]]) {
    not_built(s, "wget");
}
