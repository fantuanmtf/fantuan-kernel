//! P2 `sh` command: run the embedded dash (/bin/dash) as a user task on the
//! console, foreground, and return to the built-in shell when it exits. The
//! built-in shell stays the rescue/fallback and keeps reading input until
//! this command hands the console over.

use core::fmt::Write;

use kernel_core::log::Log;
use kernel_core::shell::Shell;

use crate::task;

const ARGV_MAX: usize = 12;

/// Minimal environment for dash: the P2 registry/root have no /etc files.
const ENV: [&[u8]; 5] = [
    b"PATH=/bin:/usr/bin",
    b"HOME=/",
    b"PWD=/",
    b"TERM=fantuan",
    b"USER=root",
];

pub fn cmd_sh(_sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    if crate::DASH_ELF.is_empty() {
        let _ = writeln!(s, "sh: dash is not embedded (run tools/build-dash.sh before the kernel build)");
        return;
    }
    let mut argv: [&[u8]; ARGV_MAX] = [&[]; ARGV_MAX];
    argv[0] = b"/bin/dash";
    let mut n = 1;
    for a in args {
        if n >= ARGV_MAX {
            break;
        }
        argv[n] = a;
        n += 1;
    }
    let Some(child) = task::spawn_user_env(crate::DASH_ELF, &argv[..n], &ENV) else {
        let _ = writeln!(s, "sh: cannot spawn dash (no free task slot?)");
        return;
    };
    let _ = writeln!(s, "sh: dash pid {} on /dev/console ('exit' returns here)", child);
    let status = task::wait_for(child);
    let _ = writeln!(s, "sh: dash pid {} exited, wait status={:#x}", child, status);
}
