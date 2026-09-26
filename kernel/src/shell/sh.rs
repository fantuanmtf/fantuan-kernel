//! Embedded shells: the P4 **login shell** and the P2/P3 `sh` / `bash` /
//! `dash` commands. Each runs the shell as a foreground user task on
//! /dev/console and returns to the built-in shell when it exits.
//!
//! P4 (DESIGN §10): `login()` is the interactive interface at boot - bash
//! when embedded, else dash; the built-in shell only keeps the console when
//! neither is there. P3: `sh` uses the same preference, `bash` selects bash
//! explicitly and `dash` keeps the P2 shell selectable.

use core::fmt::Write;

use kernel_core::log::Log;
use kernel_core::shell::Shell;

use crate::task;

const ARGV_MAX: usize = 12;

/// Minimal environment for a shell: the registry/root have no /etc files, so
/// the identity and the prompt come from here. `PS1` renders as
/// `root@Fantuan-MTF:/# `, the same host string the built-in shell uses
/// (kernel_core::shell::HOSTNAME) - spelled out rather than `\h`, because
/// `\h` goes through gethostname() and libc-fantuan's stub still answers
/// "fantuan" (there is no hostname syscall yet; see PROGRESS "Known issues").
const ENV: [&[u8]; 7] = [
    b"PATH=/bin:/usr/bin",
    b"HOME=/root",
    b"PWD=/",
    b"TERM=fantuan",
    b"USER=root",
    b"HOSTNAME=Fantuan-MTF",
    b"PS1=\\u@Fantuan-MTF:\\w# ",
];

struct ShellImage {
    image: &'static [u8],
    argv0: &'static [u8],
    label: &'static str,
}

fn bash_image() -> Option<ShellImage> {
    if crate::BASH_ELF.is_empty() {
        None
    } else {
        Some(ShellImage { image: crate::BASH_ELF, argv0: b"/bin/bash", label: "bash" })
    }
}

fn dash_image() -> Option<ShellImage> {
    if crate::DASH_ELF.is_empty() {
        None
    } else {
        Some(ShellImage { image: crate::DASH_ELF, argv0: b"/bin/dash", label: "dash" })
    }
}

fn run_shell(s: &mut Log, args: &[&[u8]], img: &ShellImage, login: bool) {
    let mut argv: [&[u8]; ARGV_MAX] = [&[]; ARGV_MAX];
    argv[0] = img.argv0;
    let mut n = 1;
    for a in args {
        if n >= ARGV_MAX {
            break;
        }
        argv[n] = a;
        n += 1;
    }
    let Some(child) = task::spawn_user_env(img.image, &argv[..n], &ENV) else {
        let _ = writeln!(s, "sh: cannot spawn {} (no free task slot?)", img.label);
        return;
    };
    // Job control handover: the spawned shell becomes the console's foreground
    // process group (the kernel shell has no pgrp), so the shell's setjobctl
    // probe `tcgetpgrp(0) == getpgrp()` succeeds instead of spinning on SIGTTIN.
    kernel_core::process::set_tty_pgrp(child);
    if login {
        let _ = writeln!(
            s,
            "shell: login shell: {} pid {} on /dev/console ('exit' returns to the built-in shell)",
            img.label, child
        );
    } else {
        let _ = writeln!(s, "sh: {} pid {} on /dev/console ('exit' returns here)", img.label, child);
    }
    let status = task::wait_for(child);
    kernel_core::process::set_tty_pgrp(0);
    let _ = writeln!(s, "sh: {} pid {} exited, wait status={:#x}", img.label, child, status);
}

/// P4 login shell (DESIGN §10): bash when embedded, else dash, else nothing -
/// in that last case the built-in shell keeps the console. Returns true once a
/// shell has run (and exited), so the caller knows to print the fallback note.
pub fn login() -> bool {
    let Some(img) = bash_image().or_else(dash_image) else {
        return false;
    };
    let mut s = Log::new();
    run_shell(&mut s, &[], &img, true);
    true
}

pub fn cmd_sh(_sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    if let Some(img) = bash_image() {
        run_shell(s, args, &img, false);
        return;
    }
    if let Some(img) = dash_image() {
        run_shell(s, args, &img, false);
        return;
    }
    let _ = writeln!(s, "sh: no shell embedded (run tools/build-bash.sh or tools/build-dash.sh)");
}

pub fn cmd_bash(_sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    match bash_image() {
        Some(img) => run_shell(s, args, &img, false),
        None => {
            let _ = writeln!(s, "bash: not embedded (run tools/build-bash.sh before the kernel build)");
        }
    }
}

pub fn cmd_dash(_sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    match dash_image() {
        Some(img) => run_shell(s, args, &img, false),
        None => {
            let _ = writeln!(s, "dash: not embedded (run tools/build-dash.sh before the kernel build)");
        }
    }
}
