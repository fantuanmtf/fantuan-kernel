//! aarch64 shell glue (M11 R9a/R9b): the shared shell core lives in
//! kernel-core; this module owns the aarch64 command table (core + optional
//! rescue + optional CONFIG_TOOLS network tools) and enters the shared loop.
//! There is no storage path on the direct-FDT machine, so the rescue commands
//! would see `vfs = None` and degrade; the minimal profile (C5) gates them
//! out entirely.
//!
//! The R7 ping/nslookup/wget handlers are shared source with the x86_64
//! kernel (`#[path]` include, kernel/src/shell/cmds_net.rs) so the two
//! command tables cannot drift; they only depend on kernel-net.

use fantuan_abi::BootInfo;

use kernel_core::shell::Command;

#[cfg(kconfig_tools)]
#[path = "../../kernel/src/shell/cmds_net.rs"]
pub mod cmds_net;

const CORE_COMMANDS: usize = 2;
#[cfg(kconfig_rescue_repair)]
const RESCUE_COMMANDS: usize = 7;
#[cfg(not(kconfig_rescue_repair))]
const RESCUE_COMMANDS: usize = 0;
#[cfg(kconfig_tools)]
const TOOL_COMMANDS: usize = 3;
#[cfg(not(kconfig_tools))]
const TOOL_COMMANDS: usize = 0;

const N_COMMANDS: usize = CORE_COMMANDS + RESCUE_COMMANDS + TOOL_COMMANDS;

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
    #[cfg(kconfig_tools)]
    Command { name: "ping", help: "ping <host> [count] — ICMP echo (count 1-5)", run: cmds_net::cmd_ping },
    #[cfg(kconfig_tools)]
    Command { name: "nslookup", help: "nslookup <name> [server[:port]] — resolve A record", run: cmds_net::cmd_nslookup },
    #[cfg(kconfig_tools)]
    Command { name: "wget", help: "wget [--insecure] http[s]://host[:port]/ — GET status/bytes", run: cmds_net::cmd_wget },
];

/// Enter the interactive shell (never returns); idle() is wfi, so the
/// generic-timer IRQ keeps scheduling the kernel tasks while waiting.
pub fn enter(bi: &BootInfo) -> ! {
    kernel_core::input::set_poll(crate::uart::getc);
    kernel_core::shell::enter(None, bi, 0, &COMMANDS)
}
