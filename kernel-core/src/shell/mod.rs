//! Minimal shell v1 (DESIGN.md §10): serial line editor, command table and
//! the interactive loop. The core commands (help, bootinfo) live in
//! shell/cmds.rs; the rescue/diagnostic commands are in shell/rescue.rs and
//! only compiled under CONFIG_RESCUE_REPAIR. This file owns input, dispatch
//! and the mount aliases.
//!
//! The command table is injected by each kernel, so x86-only commands stay
//! out of the shared crate. Idle behaviour: when no byte is ready the loop
//! enters the arch idle hook (hlt on x86, wfi on riscv) — the timer tick
//! wakes it, so the scheduler keeps running the demo tasks.

use core::fmt::Write;

use fantuan_abi::BootInfo;

use crate::log::Log;
use crate::vfs::{self, Vfs};

#[cfg(kconfig_rescue_repair)]
pub mod cat;
pub mod cmds;
#[cfg(kconfig_rescue_repair)]
pub mod rescue;

const LINE_MAX: usize = 128;
/// Autorun script: EFI/fantuan/shell.cmd, one command per line.
const SCRIPT_LINES: usize = 12;
const SCRIPT_LINE_MAX: usize = 96;

/// Default host identity (C4): the prompt and the help header show
/// `root@Fantuan-MTF`.
pub const HOSTNAME: &str = "Fantuan-MTF";

/// One shell command; the table itself is owned by each kernel.
pub struct Command {
    pub name: &'static str,
    pub help: &'static str,
    pub run: Handler,
}

pub type Handler = fn(&mut Shell, &mut Log, &[&[u8]]);

pub struct Shell<'a> {
    pub vfs: Option<Vfs>,
    pub bi: &'a BootInfo,
    pub rt: u64,
    commands: &'static [Command],
    line: [u8; LINE_MAX],
    len: usize,
    /// Script lines queued by autorun (fed before serial input).
    script: [[u8; SCRIPT_LINE_MAX]; SCRIPT_LINES],
    script_line_len: [usize; SCRIPT_LINES],
    script_count: usize,
    script_next: usize,
    /// mount aliases: (path, active)
    mounts: [([u8; 24], usize, bool); 4],
    idle_logged: bool,
}

impl<'a> Shell<'a> {
    pub fn new(vfs: Option<Vfs>, bi: &'a BootInfo, rt: u64, commands: &'static [Command]) -> Shell<'a> {
        Shell {
            vfs,
            bi,
            rt,
            commands,
            line: [0; LINE_MAX],
            len: 0,
            script: [[0; SCRIPT_LINE_MAX]; SCRIPT_LINES],
            script_line_len: [0; SCRIPT_LINES],
            script_count: 0,
            script_next: 0,
            mounts: [([0; 24], 0, false); 4],
            idle_logged: false,
        }
    }

    // --- autorun ---

    /// Load EFI/fantuan/shell.cmd (when present) into the script queue.
    fn load_script(&mut self, s: &mut Log) {
        let Some(vfs) = self.vfs else { return };
        let (Some(efi), Some(dir), Some(file)) = (vfs::to_8_3("EFI"), vfs::to_8_3("fantuan"), vfs::to_8_3("shell.cmd")) else {
            return;
        };
        let Some((cluster, size)) = vfs::find_path(&vfs.fs, vfs.fs.root_cluster, &[&efi, &dir, &file]) else {
            return;
        };
        let mut buf = [0u8; SCRIPT_LINES * SCRIPT_LINE_MAX];
        let want = (size as usize).min(buf.len());
        let Some(n) = vfs.fs.read_file(cluster, want as u32, &mut buf[..want]) else { return };

        let mut li = 0usize;
        let mut col = 0usize;
        for &b in &buf[..n] {
            if b == b'\n' || b == b'\r' {
                if col > 0 && li < SCRIPT_LINES {
                    self.script_line_len[li] = col;
                    li += 1;
                    col = 0;
                }
                continue;
            }
            if li < SCRIPT_LINES && col < SCRIPT_LINE_MAX {
                self.script[li][col] = b;
                col += 1;
            }
        }
        if col > 0 && li < SCRIPT_LINES {
            self.script_line_len[li] = col;
            li += 1;
        }
        self.script_count = li;
        if li > 0 {
            let _ = writeln!(s, "shell: autorun {} command(s) from EFI/fantuan/shell.cmd", li);
        }
    }

    // --- input ---

    /// Blocking line read: autorun lines first, then the UART. Echoes input.
    /// Logs the idle notice once after ~30 s without a serial byte.
    pub fn read_line(&mut self, s: &mut Log) -> bool {
        // Autorun: feed the whole line at once (echoed for the transcript).
        if self.script_next < self.script_count {
            let idx = self.script_next;
            self.script_next += 1;
            let n = self.script_line_len[idx].min(LINE_MAX);
            self.line[..n].copy_from_slice(&self.script[idx][..n]);
            self.len = n;
            let _ = s.write(&self.line[..n]);
            let _ = s.write(b"\r\n");
            return true;
        }

        self.len = 0;
        let mut empty = 0u32;
        loop {
            match crate::input::poll_byte() {
                Some(b) => {
                    empty = 0;
                    match b {
                        b'\n' | b'\r' => {
                            let _ = s.write(b"\r\n");
                            return true;
                        }
                        0x7F | 0x08 => {
                            if self.len > 0 {
                                self.len -= 1;
                                let _ = s.write(b"\x08 \x08");
                            }
                        }
                        0x20..=0x7E => {
                            if self.len < LINE_MAX {
                                self.line[self.len] = b;
                                self.len += 1;
                                let _ = s.write(&[b]);
                            }
                        }
                        _ => {}
                    }
                }
                None => {
                    // The timer tick (~10 ms) wakes the arch idle -> ~30 s.
                    empty += 1;
                    if empty == 3000 && !self.idle_logged {
                        self.idle_logged = true;
                        let _ = writeln!(s, "shell: idle on serial (no input yet)");
                    }
                    crate::arch::idle();
                }
            }
        }
    }

    pub fn line_bytes(&self) -> &[u8] {
        &self.line[..self.len]
    }

    // --- mount aliases (rescue commands only) ---

    #[cfg(kconfig_rescue_repair)]
    fn mount_alias(&mut self, path: &[u8]) -> bool {
        for m in self.mounts.iter_mut() {
            if m.2 && &m.0[..m.1] == path {
                return false;
            }
        }
        for m in self.mounts.iter_mut() {
            if !m.2 {
                let n = path.len().min(24);
                m.0[..n].copy_from_slice(&path[..n]);
                m.1 = n;
                m.2 = true;
                return true;
            }
        }
        false
    }

    #[cfg(kconfig_rescue_repair)]
    fn umount_alias(&mut self, path: &[u8]) -> bool {
        for m in self.mounts.iter_mut() {
            if m.2 && &m.0[..m.1] == path {
                m.2 = false;
                return true;
            }
        }
        false
    }

    pub fn mounts(&self) -> &[([u8; 24], usize, bool); 4] {
        &self.mounts
    }

    /// The injected command table (used by the help command).
    pub fn commands(&self) -> &'static [Command] {
        self.commands
    }

    // --- dispatch ---

    pub fn run_line(&mut self, s: &mut Log, line: &[u8]) {
        let mut toks: [&[u8]; 4] = [&[]; 4];
        let mut n = 0usize;
        let mut start: Option<usize> = None;
        for (i, &b) in line.iter().enumerate() {
            if b == b' ' || b == b'\t' {
                if let Some(st) = start.take() {
                    if n < toks.len() {
                        toks[n] = &line[st..i];
                        n += 1;
                    }
                }
            } else if start.is_none() {
                start = Some(i);
            }
        }
        if let Some(st) = start {
            if n < toks.len() {
                toks[n] = &line[st..];
                n += 1;
            }
        }
        if n == 0 {
            return;
        }
        for cmd in self.commands {
            if toks[0] == cmd.name.as_bytes() {
                (cmd.run)(self, s, &toks[1..n]);
                return;
            }
        }
        let _ = write!(s, "shell: unknown command '");
        let _ = s.write(toks[0]);
        let _ = writeln!(s, "' — try 'help'");
    }

    /// The interactive loop: autorun first, then serial input.
    pub fn run(&mut self, s: &mut Log) {
        // C4: quiet the boot heartbeat BEFORE the ready line so no `tick:`
        // line can appear after it or split a typed line.
        crate::heartbeat::shell_ready();
        let _ = writeln!(s, "shell: ready (root@{HOSTNAME}; type 'help'; idle note after 30 s)");
        self.load_script(s);
        loop {
            let _ = write!(s, "root@{HOSTNAME}> ");
            if !self.read_line(s) {
                continue;
            }
            let mut copy = [0u8; LINE_MAX];
            let n = self.len;
            copy[..n].copy_from_slice(&self.line[..n]);
            self.run_line(s, &copy[..n]);
            self.len = 0;
        }
    }
}

/// Wake the shell with a one-line idle message when no input ever arrives.
pub fn enter(vfs: Option<Vfs>, bi: &BootInfo, rt: u64, commands: &'static [Command]) -> ! {
    let mut s = Log::new();
    let _ = writeln!(s, "kernel: idle (hlt; IRQ0 ticks wake it)");
    let mut sh = Shell::new(vfs, bi, rt, commands);
    sh.run(&mut s);
    loop {
        crate::arch::idle();
    }
}
