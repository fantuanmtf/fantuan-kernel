//! x86 shell glue: the shared shell core lives in kernel-core; this module
//! owns the x86 command table (shared + x86-only commands) and enters the
//! shared loop.

use fantuan_abi::BootInfo;
use kernel_core::shell::Command;
use kernel_core::vfs::Vfs;

pub use kernel_core::shell::{cat, cmds as shared_cmds};

pub mod cmds;

/// The x86 command table: shared commands plus the hardware-specific ones.
pub static COMMANDS: [Command; 12] = [
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
];

/// Enter the interactive shell with the x86 table.
pub fn enter(vfs: Option<Vfs>, bi: &BootInfo, rt: u64) -> ! {
    kernel_core::shell::enter(vfs, bi, rt, &COMMANDS)
}
