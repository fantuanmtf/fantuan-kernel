//! aarch64 shell glue (M11 R9a): the shared shell core lives in kernel-core;
//! this module owns the aarch64 command table. There is no storage path in
//! R9a, so the rescue commands would see `vfs = None` and degrade; the
//! minimal profile (C5) gates them out of the table entirely.

use fantuan_abi::BootInfo;

use kernel_core::shell::Command;

const CORE_COMMANDS: usize = 2;
#[cfg(kconfig_rescue_repair)]
const RESCUE_COMMANDS: usize = 7;
#[cfg(not(kconfig_rescue_repair))]
const RESCUE_COMMANDS: usize = 0;

const N_COMMANDS: usize = CORE_COMMANDS + RESCUE_COMMANDS;

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
    Command { name: "cat", help: "cat <path> - print a file (FAT or ext4, 4 KiB max)", run: kernel_core::shell::cat::cmd_cat },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "diskhealth", help: "disk health [--scan]", run: kernel_core::shell::rescue::cmd_diskhealth },
    #[cfg(kconfig_rescue_repair)]
    Command { name: "grub-fix", help: "boot repair [diagnose|repair|install]", run: kernel_core::shell::rescue::cmd_grubfix },
];

/// Enter the interactive shell (never returns); idle() is wfi, so the
/// generic-timer IRQ keeps scheduling the kernel tasks while waiting.
pub fn enter(bi: &BootInfo) -> ! {
    kernel_core::input::set_poll(crate::uart::getc);
    kernel_core::shell::enter(None, bi, 0, &COMMANDS)
}
