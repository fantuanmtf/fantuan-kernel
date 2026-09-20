//! Shared heartbeat-quiet flag (C4): the arch timers print a `tick: N s`
//! boot heartbeat for the first 30 s. The heartbeat must never interleave
//! with interactive shell input, so the shell raises this flag before its
//! `shell: ready` line and the timers skip the heartbeat from then on; the
//! 30 s cap stays as the fallback when no shell is ever reached.

use core::sync::atomic::{AtomicBool, Ordering};

static SHELL_READY: AtomicBool = AtomicBool::new(false);

/// Called by the shared shell as soon as it takes over the console.
pub fn shell_ready() {
    SHELL_READY.store(true, Ordering::Release);
}

/// True once the shell owns the console; the timers stop the heartbeat.
pub fn quiet() -> bool {
    SHELL_READY.load(Ordering::Acquire)
}
