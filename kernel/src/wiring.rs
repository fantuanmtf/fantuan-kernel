//! The x86_64 wiring half of `kmain`: the embedded-binary registry, the
//! kernel-core hooks this arch installs, the task spawns, and the hand-over
//! to the login shell.
//!
//! Split out of `main.rs` in 2026-10 alongside `boot` (see `boot.rs` for the
//! why). `boot` powers the machine on; this starts tasks and gives the
//! console away. `main.rs` calls these in sequence and the order matters: the
//! hooks must be installed before anything can fork, and `enter_shell` never
//! returns.

use core::fmt::Write;

use fantuan_abi::BootInfo;

use crate::bootlog::serial;
use crate::serial::Serial;

/// P2/P3 embedded-binary registry: execve("/bin/sh") resolves without a
/// filesystem mount (the tmpfs file cap is far below a static shell).
/// P3: /bin/sh prefers bash when embedded, dash stays /bin/dash.
pub fn lookup_bin(path: &[u8]) -> Option<&'static [u8]> {
    if path == b"/bin/bash" && !crate::BASH_ELF.is_empty() {
        Some(crate::BASH_ELF)
    } else if path == b"/bin/sh" {
        if !crate::BASH_ELF.is_empty() {
            Some(crate::BASH_ELF)
        } else if !crate::DASH_ELF.is_empty() {
            Some(crate::DASH_ELF)
        } else {
            None
        }
    } else if path == b"/bin/dash" && !crate::DASH_ELF.is_empty() {
        Some(crate::DASH_ELF)
    } else if path == b"/bin/hello" && !crate::HELLO_ELF.is_empty() {
        Some(crate::HELLO_ELF)
    } else if path == b"/bin/proc-test" && !crate::PROC_ELF.is_empty() {
        Some(crate::PROC_ELF)
    } else if path == b"/bin/ls" && !crate::LS_ELF.is_empty() {
        Some(crate::LS_ELF)
    } else if path == b"/bin/cat" && !crate::CAT_ELF.is_empty() {
        Some(crate::CAT_ELF)
    } else {
        None
    }
}

/// The syscall ABI probe, the process-layer hooks, the embedded shell's hash
/// line and the three kernel demo tasks.
pub fn phase_processes(bi: &BootInfo, s: &mut Serial) {
    // --- M3: kernel tasks + syscall ABI ------------------------------------
    let abi = crate::syscall::syscall(crate::syscall::SYS_VERSION, 0, 0, 0, 0, 0);
    let _ = writeln!(s, "syscall: ABI v{} (int 0x60, versioned dispatch)", abi);
    crate::task::init_arch();
    crate::syscall::init_frame_ops();
    kernel_core::process::set_bin_lookup(lookup_bin);
    let _ = writeln!(
        s,
        "p2: process layer ready (fork/exec/wait, signals, VMA; dash embedded={} bash embedded={})",
        if crate::DASH_ELF.is_empty() { "no" } else { "yes" },
        if crate::BASH_ELF.is_empty() { "no" } else { "yes" }
    );
    // P3/licensing: state which shell artifact the image carries. The hash
    // comes from kernel/bash_program.sha256 via build.rs (SHELL_IMAGE_SHA256)
    // and is what tools/smoke-gpl.sh searches the ELF for, so the source
    // provision claim in THIRD_PARTY.md is checkable inside the image.
    if !crate::SHELL_IMAGE_SHA256.is_empty() {
        let _ = writeln!(s, "shell: image sha256 {}", crate::SHELL_IMAGE_SHA256);
    }
    crate::task::init(bi.stack_top);
    crate::task::spawn(crate::demo::demo_1);
    crate::task::spawn(crate::demo::demo_2);
    crate::task::spawn(crate::demo::demo_3);
    let _ = writeln!(s, "sched: 3 kernel demo tasks spawned (quantum 100 ms)");
}

/// The ring-3 user tasks: the shared demo ELF, the libc C hello and the P2
/// proc-test. **Must follow** `boot::phase_hardening`: NX has to be live
/// before the ELF loader maps anything with `P_NX`.
pub fn phase_userland(s: &mut Serial) {
    // --- M4: user mode ------------------------------------------------------
    let u1 = crate::task::spawn_user(crate::USER_ELF);
    let u2 = crate::task::spawn_user(crate::USER_ELF);
    let _ = writeln!(s, "user: ELF {} bytes -> tids {} {} (ring 3, per-task page tables)", crate::USER_ELF.len(), u1.unwrap_or(0), u2.unwrap_or(0));

    // --- P1: POSIX round 1 --------------------------------------------------
    // The C hello (libc-fantuan) uses the same loader with SysV argv; it is
    // only present when tools/build-libc.sh produced it.
    crate::syscall::init_mem_ops();
    if !crate::HELLO_ELF.is_empty() {
        let h = crate::task::spawn_user_args(crate::HELLO_ELF, &[b"/bin/hello"]);
        let _ = writeln!(s, "user: C hello ELF {} bytes -> tid {} (libc-fantuan, fd syscalls)", crate::HELLO_ELF.len(), h.unwrap_or(0));
    }
    if !crate::PROC_ELF.is_empty() {
        let p = crate::task::spawn_user_args(crate::PROC_ELF, &[b"/bin/proc-test"]);
        let _ = writeln!(s, "user: P2 proc-test ELF {} bytes -> tid {} (fork/exec/wait/signals/mmap)", crate::PROC_ELF.len(), p.unwrap_or(0));
    }
}

/// Mirror serial to the GOP, let the user tasks finish, freeze the mirror
/// while the demo animates, install the login shell and hand over the
/// console. Never returns: the shell owns the machine from here, and the
/// built-in shell keeps it when the login shell exits.
pub fn enter_shell(bi: &BootInfo, mounted: Option<crate::vfs::Vfs>) -> ! {
    // --- §10 Minimal shell -------------------------------------------------
    // M8.5b: mirror serial to the GOP so the shell works without a serial port.
    #[cfg(kconfig_graphics)]
    if crate::console::up() {
        crate::serial::enable_mirror();
        let _ = writeln!(serial(), "console: serial output mirrored to GOP (keyboard + display)");
        // M13-2: wait for the userland/proc-test tasks to be reaped, then
        // freeze the GOP mirror while the demo animates so the box is the only
        // thing moving on screen (serial keeps flowing; the demo unfreezes).
        while kernel_core::task::user_tasks_remaining() {
            kernel_core::task::sleep_ms(100);
        }
        crate::serial::freeze_mirror();
        crate::task::spawn(crate::gfx_demo::run);
    }
    // P4: the interactive interface is the embedded login shell (bash, else
    // dash); the built-in shell takes the console back when it exits, and
    // keeps it outright when no shell is embedded (DESIGN §10).
    kernel_core::shell::set_login_shell(crate::shell::sh::login);
    // The loop halts between polls, so the scheduler keeps running the tasks.
    crate::shell::enter(mounted, bi, bi.runtime_services);
}
