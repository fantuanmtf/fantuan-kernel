//! P2/P3 `sh` / `bash` / `dash` commands: run an embedded shell as a user
//! task on the console, foreground, and return to the built-in shell when it
//! exits. The built-in shell stays the rescue/fallback and keeps reading
//! input until one of these commands hands the console over.
//!
//! P3: `sh` prefers bash when embedded (the M14-8 default-shell target),
//! `bash` selects it explicitly and `dash` keeps the P2 shell selectable.

use core::fmt::Write;

use kernel_core::log::Log;
use kernel_core::shell::Shell;

use crate::task;

const ARGV_MAX: usize = 12;

/// Minimal environment for a shell: the registry/root have no /etc files.
const ENV: [&[u8]; 5] = [
    b"PATH=/bin:/usr/bin",
    b"HOME=/",
    b"PWD=/",
    b"TERM=fantuan",
    b"USER=root",
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

fn run_shell(s: &mut Log, args: &[&[u8]], img: &ShellImage) {
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
    let _ = writeln!(s, "sh: {} pid {} on /dev/console ('exit' returns here)", img.label, child);
    let status = task::wait_for(child);
    kernel_core::process::set_tty_pgrp(0);
    let _ = writeln!(s, "sh: {} pid {} exited, wait status={:#x}", img.label, child, status);
}

pub fn cmd_sh(_sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    if let Some(img) = bash_image() {
        run_shell(s, args, &img);
        return;
    }
    if let Some(img) = dash_image() {
        run_shell(s, args, &img);
        return;
    }
    let _ = writeln!(s, "sh: no shell embedded (run tools/build-bash.sh or tools/build-dash.sh)");
}

pub fn cmd_bash(_sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    match bash_image() {
        Some(img) => run_shell(s, args, &img),
        None => {
            let _ = writeln!(s, "bash: not embedded (run tools/build-bash.sh before the kernel build)");
        }
    }
}

pub fn cmd_dash(_sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    match dash_image() {
        Some(img) => run_shell(s, args, &img),
        None => {
            let _ = writeln!(s, "dash: not embedded (run tools/build-dash.sh before the kernel build)");
        }
    }
}
