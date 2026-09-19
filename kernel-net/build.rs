//! Build the NetBSD rump slice + the fantuan adaptation layer as one static
//! archive, compiled with clang for x86_64-unknown-none against the R1 shim
//! headers. The crate is only wired into the x86_64 kernel for now; other
//! build targets skip the C compile entirely.
//!
//! The imported sources keep upstream warnings (exempt from the repo's
//! zero-warning rule); the adapter files are compiled with -Wall -Wextra.

use std::env;
use std::path::Path;
use std::process::Command;

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../tools/kconfig_emit.rs"));

const NETBSD_CSRCS: &[&str] = &[
    "sys/kern/subr_evcnt.c",
    "sys/kern/kern_mutex.c",
    "sys/kern/kern_condvar.c",
    "sys/kern/kern_rwlock.c",
    "sys/kern/kern_lock.c",
    "sys/kern/kern_timeout.c",
    "sys/kern/subr_psref.c",
    "sys/kern/subr_pool.c",
    "sys/kern/subr_hash.c",
    "sys/kern/subr_once.c",
    "sys/kern/subr_pserialize.c",
    "sys/kern/uipc_mbuf.c",
    "sys/kern/uipc_socket.c",
    "sys/kern/uipc_socket2.c",
    "sys/net/bpf_stub.c",
    "sys/net/if.c",
    "sys/net/if_llatbl.c",
    "sys/net/if_loop.c",
    "sys/net/if_stats.c",
    "sys/net/nd.c",
    "sys/net/radix.c",
    "sys/net/route.c",
    "sys/net/rtbl.c",
    "sys/netinet/cpu_in_cksum.c",
    "sys/netinet/if_arp.c",
    "sys/netinet/in.c",
    "sys/netinet/in4_cksum.c",
    "sys/netinet/in_cksum.c",
    "sys/netinet/in_offload.c",
    "sys/netinet/in_pcb.c",
    "sys/netinet/in_proto.c",
    "sys/netinet/ip_icmp.c",
    "sys/netinet/ip_input.c",
    "sys/netinet/ip_output.c",
    "sys/netinet/ip_reass.c",
    "sys/netinet/tcp_congctl.c",
    "sys/netinet/tcp_input.c",
    "sys/netinet/tcp_output.c",
    "sys/netinet/tcp_sack.c",
    "sys/netinet/tcp_subr.c",
    "sys/netinet/tcp_syncache.c",
    "sys/netinet/tcp_timer.c",
    "sys/netinet/tcp_usrreq.c",
    "sys/netinet/udp_usrreq.c",
];

const SHIM_CSRCS: &[&str] = &[
    "rump_shim_lib.c",
    "rump_shim_mem.c",
    "rump_shim_time.c",
    "rump_shim_lock.c",
    "rump_shim_sleepq.c",
    "rump_shim_printf.c",
    "rump_shim_sysctl.c",
    "rump_shim_net.c",
    "rump_shim_if.c",
    "rump_shim_route.c",
    "rump_shim_sock.c",
    "rump_shim_softint.c",
    "rump_shim_inet.c",
    "rump_shim_misc.c",
    "rump_shim_ksock.c",
    "rump_shim_mobj.c",
    "rump_shim_init.c",
    "rump_domain.c",
    "rump_loss.c",
    "rump_loopback.c",
    "rump_ip4.c",
    "rump_ping.c",
    "rump_udp.c",
    "rump_tcp.c",
    "rump_tcp_io.c",
    "rump_tcp_conn.c",
    "rump_arp.c",
    "rump_e1000.c",
    "rump_e1000_dma.c",
    "rump_e1000_if.c",
    "rump_e1000_ops.c",
    "rump_dhcp.c",
    "rump_dhcp_pkt.c",
    "rump_dhcp_if.c",
    "rump_http.c",
    "rump_selftest.c",
    "rump_md5.c",
];

fn main() {
    kconfig_emit();
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let target = env::var("TARGET").unwrap();
    let netbsd = format!("{dir}/../third_party/netbsd");
    let shim = format!("{netbsd}/shim/include");

    for f in NETBSD_CSRCS {
        println!("cargo:rerun-if-changed={netbsd}/{f}");
    }
    for f in SHIM_CSRCS {
        println!("cargo:rerun-if-changed={dir}/src/c/{f}");
    }
    println!("cargo:rerun-if-changed={dir}/src/c/rump_shim.h");
    println!("cargo:rerun-if-changed={shim}/machine/mutex.h");
    println!("cargo:rerun-if-changed={shim}/machine/cpu.h");
    println!("cargo:rerun-if-changed={shim}/ether.h");
    println!("cargo:rerun-if-changed={shim}/bridge.h");
    println!("cargo:rerun-if-changed={shim}/carp.h");
    println!("cargo:rerun-if-changed={shim}/arp.h");
    println!("cargo:rerun-if-changed={shim}/arcnet.h");
    println!("cargo:rerun-if-changed={shim}/gif.h");
    println!("cargo:rerun-if-changed={shim}/gre.h");
    println!("cargo:rerun-if-changed={shim}/pfsync.h");
    for h in [
        "faith.h",
        "sys/file.h",
        "sys/filedesc.h",
        "sys/poll.h",
        "sys/kthread.h",
        "sys/buf.h",
        "sys/md5.h",
        "uvm/uvm_loan.h",
        "uvm/uvm_page.h",
        "net/if_faith.h",
        "netinet/sctp_route.h",
        "netinet6/nd6.h",
        "netinet6/scope6_var.h",
        "netinet6/in6_offload.h",
        "netinet6/ip6protosw.h",
        "netipsec/ipsec.h",
        "netipsec/ipsec6.h",
        "netipsec/key.h",
        "ddb/db_active.h",
        "compat/sys/socket.h",
    ] {
        println!("cargo:rerun-if-changed={shim}/{h}");
    }

    if target != "x86_64-unknown-none" {
        return;
    }

    let resource = Command::new("clang")
        .arg("-print-resource-dir")
        .output()
        .expect("clang is required to build the NetBSD rump slice");
    let resource = String::from_utf8(resource.stdout).unwrap();
    let resource = resource.trim();
    assert!(Path::new(resource).exists(), "clang resource dir missing");

    let base = |warnings: bool| {
        let mut build = cc::Build::new();
        build
            .compiler("clang")
            .archiver("llvm-ar")
            .warnings(warnings)
            .flag(format!("--target={target}"))
            .flag("-ffreestanding")
            .flag("-nostdinc")
            .flag("-isystem")
            .flag(format!("{resource}/include"))
            .flag("-mno-red-zone")
            /* Match the Rust kernel's soft-float ABI: the BIOS boot path
             * does not enable CR4.OSFXSR, so SSE would fault with #UD. */
            .flag("-mno-sse")
            .flag("-mno-sse2")
            .flag("-mno-mmx")
            .flag("-msoft-float")
            .flag("-fno-stack-protector")
            .flag("-fno-builtin")
            .flag("-fno-pic")
            .flag("-fno-pie")
            .flag("-mcmodel=large")
            .flag("-ffunction-sections")
            .flag("-fdata-sections")
            .flag("-D_KERNEL")
            /* The sources are NetBSD's; the target triple is not.  __NetBSD__
             * selects the NetBSD-specific paths in shared headers (e.g. the
             * 802.11 ioctl numbers), INET the IPv4 stack paths. */
            .flag("-D__NetBSD__")
            .flag("-DINET=1")
            .include(&shim)
            .include(format!("{netbsd}/sys"))
            .include(format!("{netbsd}/common/include"))
            .include(format!("{dir}/src/c"));
        build
    };

    let mut imported = base(false);
    for f in NETBSD_CSRCS {
        imported.file(format!("{netbsd}/{f}"));
    }
    imported.compile("rumpobj");

    let mut adapter = base(true);
    /* -Wextra flags unused parameters inside the imported NetBSD headers
     * (inline helpers the adapter only includes); keep the rest of -Wall
     * -Wextra for our own files. */
    adapter.flag("-Wno-unused-parameter");
    for f in SHIM_CSRCS {
        adapter.file(format!("{dir}/src/c/{f}"));
    }
    adapter.compile("rumpobj_shim");
}
