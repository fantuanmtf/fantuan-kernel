//! x86 shell glue: the shared shell core lives in kernel-core; this module
//! owns the x86 command table (shared + x86-only commands) and enters the
//! shared loop.

use fantuan_abi::BootInfo;
use kernel_core::shell::Command;
use kernel_core::vfs::Vfs;

pub use kernel_core::shell::{cat, cmds as shared_cmds};

#[cfg(kconfig_tools)]
pub mod cmds_net;
pub mod cmds;

/// Command slots: the shared x86 set plus the CONFIG_TOOLS network tools.
const BASE_COMMANDS: usize = 12;
#[cfg(kconfig_tools)]
pub const N_COMMANDS: usize = BASE_COMMANDS + 3;
#[cfg(not(kconfig_tools))]
pub const N_COMMANDS: usize = BASE_COMMANDS;

/// The x86 command table: shared commands plus the hardware-specific ones.
pub static COMMANDS: [Command; N_COMMANDS] = [
    Command { name: "help", help: "this table", run: shared_cmds::cmd_help },
    Command { name: "hwdiag", help: "re-run hardware + storage diagnostics", run: cmds::cmd_hwdiag },
    Command { name: "lsdev", help: "list PCI storage/display devices + drive ID", run: cmds::cmd_lsdev },
    Command { name: "lsos", help: "filesystems per partition (probe table)", run: shared_cmds::cmd_lsos },
    Command { name: "lsmnt", help: "mount table", run: shared_cmds::cmd_lsmnt },
    Command { name: "mount", help: "mount esp0 /mnt/esp0 — ro alias only", run: shared_cmds::cmd_mount },
    Command { name: "umount", help: "umount <path>", run: shared_cmds::cmd_umount },
    Command { name: "cat", help: "cat <path> — print a file (FAT or ext4, 4 KiB max)", run: cat::cmd_cat },
    Command { name: "bootinfo", help: "boot handover details", run: shared_cmds::cmd_bootinfo },
    Command { name: "diskhealth", help: "disk health [--scan]", run: shared_cmds::cmd_diskhealth },
    Command { name: "grub-fix", help: "boot repair [diagnose|repair|install]", run: shared_cmds::cmd_grubfix },
    Command { name: "crypto-selftest", help: "run the SHA-256/RSA known-answer tests", run: cmds::cmd_crypto },
    #[cfg(kconfig_tools)]
    Command { name: "ping", help: "ping <host> [count] — ICMP echo (count 1-5)", run: cmds_net::cmd_ping },
    #[cfg(kconfig_tools)]
    Command { name: "nslookup", help: "nslookup <name> [server[:port]] — resolve A record", run: cmds_net::cmd_nslookup },
    #[cfg(kconfig_tools)]
    Command { name: "wget", help: "wget http://host[:port]/ — HTTP GET status/bytes", run: cmds_net::cmd_wget },
];

/// Enter the interactive shell with the x86 table.
pub fn enter(vfs: Option<Vfs>, bi: &BootInfo, rt: u64) -> ! {
    kernel_core::shell::enter(vfs, bi, rt, &COMMANDS)
}
