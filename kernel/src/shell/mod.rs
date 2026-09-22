//! x86 shell glue: the shared shell core lives in kernel-core; this module
//! owns the x86 command table (core + optional rescue + optional tools) and
//! enters the shared loop.
//!
//! The table length is assembled from cfg-gated blocks, so every combination
//! of CONFIG_RESCUE_REPAIR (C5) and CONFIG_TOOLS/CONFIG_NET stays correct:
//!   core    help, bootinfo
//!   rescue  hwdiag, lsdev, lsos, lsmnt, mount, umount, cat, diskhealth,
//!           grub-fix, crypto-selftest; gpu when CONFIG_GRAPHICS is on
//!   imager  clone (CONFIG_IMAGER; independent of RESCUE_REPAIR)
//!   tools   ping, nslookup, wget (CONFIG_TOOLS; CONFIG_NET=n stubs)

use fantuan_abi::BootInfo;
use kernel_core::shell::Command;
use kernel_core::vfs::Vfs;

pub use kernel_core::shell::cmds as shared_cmds;
#[cfg(kconfig_rescue_repair)]
pub use kernel_core::shell::{cat, ls, rescue};
#[cfg(kconfig_rescue_repair)]
pub mod cmds;
#[cfg(kconfig_tools)]
pub mod cmds_net;
pub mod sh;

const CORE_COMMANDS: usize = 5;
#[cfg(all(kconfig_rescue_repair, kconfig_graphics))]
const RESCUE_COMMANDS: usize = 12;
#[cfg(all(kconfig_rescue_repair, not(kconfig_graphics)))]
const RESCUE_COMMANDS: usize = 11;
#[cfg(not(kconfig_rescue_repair))]
const RESCUE_COMMANDS: usize = 0;
#[cfg(kconfig_imager)]
const IMAGER_COMMANDS: usize = 1;
#[cfg(not(kconfig_imager))]
const IMAGER_COMMANDS: usize = 0;
#[cfg(kconfig_tools)]
const TOOL_COMMANDS: usize = 3;
#[cfg(not(kconfig_tools))]
const TOOL_COMMANDS: usize = 0;

pub const N_COMMANDS: usize = CORE_COMMANDS + RESCUE_COMMANDS + IMAGER_COMMANDS + TOOL_COMMANDS;

/// The x86 command table: core commands, the rescue/diagnostic set and the
/// CONFIG_TOOLS network tools, each block compiled only when selected.
pub static COMMANDS: [Command; N_COMMANDS] = [
    Command { name: "help", help: "this table", run: shared_cmds::cmd_help },
    Command { name: "bootinfo", help: "boot handover details", run: shared_cmds::cmd_bootinfo },
    Command { name: "sh", help: "default shell (bash, else dash); args pass through, 'exit' returns", run: sh::cmd_sh },
    Command { name: "bash", help: "GNU bash 5.3 (/bin/bash, GPLv3 app layer); args pass through", run: sh::cmd_bash },
    Command { name: "dash", help: "dash 0.5.12 (/bin/dash, the P2 fallback shell)", run: sh::cmd_dash },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "hwdiag", help: "re-run hardware + storage diagnostics", run: cmds::cmd_hwdiag },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "lsdev", help: "list PCI storage/display devices + drive ID", run: cmds::cmd_lsdev },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "lsos", help: "filesystems per partition (probe table)", run: rescue::cmd_lsos },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "lsmnt", help: "mount table", run: rescue::cmd_lsmnt },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "mount", help: "mount esp0 /mnt/esp0 — ro alias only", run: rescue::cmd_mount },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "umount", help: "umount <path>", run: rescue::cmd_umount },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "ls", help: "ls <path> — list a directory (FAT/ext4/NTFS)", run: ls::cmd_ls },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "cat", help: "cat <path> — print a file (FAT/ext4/NTFS, 4 KiB max)", run: cat::cmd_cat },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "diskhealth", help: "disk health [--scan]", run: rescue::cmd_diskhealth },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "grub-fix", help: "boot repair [diagnose|repair|install]", run: rescue::cmd_grubfix },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "crypto-selftest", help: "run the SHA-256/RSA known-answer tests", run: cmds::cmd_crypto },
    #[cfg(all(kconfig_rescue_repair, kconfig_graphics))]
    Command { name: "gpu", help: "GPU/PCI report: IDs, BARs, PCIe link, ACPI thermal (ro)", run: cmds::cmd_gpu },
    #[cfg(kconfig_imager)]
    Command { name: "clone", help: "clone <src> <dst> [--verify] [--yes] — verified raw disk copy", run: kernel_core::shell::imager::cmd_clone },
    #[cfg(kconfig_tools)]
    Command { name: "ping", help: "ping <host> [count] — ICMP echo (count 1-5)", run: cmds_net::cmd_ping },
    #[cfg(kconfig_tools)]
    Command { name: "nslookup", help: "nslookup <name> [server[:port]] — resolve A record", run: cmds_net::cmd_nslookup },
    #[cfg(kconfig_tools)]
    Command { name: "wget", help: "wget [--insecure] http[s]://host[:port]/ — GET status/bytes", run: cmds_net::cmd_wget },
];

/// Enter the interactive shell with the x86 table.
pub fn enter(vfs: Option<Vfs>, bi: &BootInfo, rt: u64) -> ! {
    kernel_core::shell::enter(vfs, bi, rt, &COMMANDS)
}
