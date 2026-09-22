//! RISC-V shell glue (M9.4): the shared shell core lives in kernel-core;
//! this module owns the riscv command table. `grub-fix` can diagnose, and
//! its NVRAM paths see rt = 0 and degrade honestly.
//!
//! Core: help, bootinfo. The rescue/diagnostic block is compiled only under
//! CONFIG_RESCUE_REPAIR (C5), so the minimal command table lists the core
//! builtins only.

use fantuan_abi::BootInfo;
use kernel_core::shell::Command;
use kernel_core::vfs::Vfs;

const CORE_COMMANDS: usize = 2;
#[cfg(kconfig_rescue_repair)]
const RESCUE_COMMANDS: usize = 8;
#[cfg(not(kconfig_rescue_repair))]
const RESCUE_COMMANDS: usize = 0;
#[cfg(kconfig_imager)]
const IMAGER_COMMANDS: usize = 1;
#[cfg(not(kconfig_imager))]
const IMAGER_COMMANDS: usize = 0;

const N_COMMANDS: usize = CORE_COMMANDS + RESCUE_COMMANDS + IMAGER_COMMANDS;

static COMMANDS: [Command; N_COMMANDS] = [
    Command { name: "help", help: "this table", run: kernel_core::shell::cmds::cmd_help },
    Command { name: "bootinfo", help: "boot handover details", run: kernel_core::shell::cmds::cmd_bootinfo },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "lsos", help: "filesystems per partition (probe table)", run: kernel_core::shell::rescue::cmd_lsos },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "lsmnt", help: "mount table", run: kernel_core::shell::rescue::cmd_lsmnt },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "mount", help: "mount esp0 /mnt/esp0 - ro alias only", run: kernel_core::shell::rescue::cmd_mount },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "umount", help: "umount <path>", run: kernel_core::shell::rescue::cmd_umount },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "ls", help: "ls <path> - list a directory (FAT/ext4/NTFS)", run: kernel_core::shell::ls::cmd_ls },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "cat", help: "cat <path> - print a file (FAT/ext4/NTFS, 4 KiB max)", run: kernel_core::shell::cat::cmd_cat },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "diskhealth", help: "disk health [--scan]", run: kernel_core::shell::rescue::cmd_diskhealth },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "grub-fix", help: "boot repair [diagnose|repair|install]", run: kernel_core::shell::rescue::cmd_grubfix },
    #[cfg(kconfig_imager)]
    Command { name: "clone", help: "clone <src> <dst> [--verify] [--yes] — verified raw disk copy", run: kernel_core::shell::imager::cmd_clone },
];

/// Enter the interactive shell (never returns); idle() is wfi, so the SBI
/// timer keeps scheduling the kernel and user tasks while waiting.
pub fn enter(vfs: Option<Vfs>, bi: &BootInfo) -> ! {
    kernel_core::input::set_poll(crate::uart::getc);
    kernel_core::shell::enter(vfs, bi, 0, &COMMANDS)
}
