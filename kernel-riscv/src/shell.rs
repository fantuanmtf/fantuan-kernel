//! RISC-V shell glue (M9.4): the shared shell core lives in kernel-core;
//! this module owns the riscv command table. `grub-fix` can diagnose, and
//! its NVRAM paths see rt = 0 and degrade honestly.

use fantuan_abi::BootInfo;
use kernel_core::shell::Command;
use kernel_core::vfs::Vfs;

/// Shared commands only: the x86 hardware commands (hwdiag/lsdev/crypto)
/// have no riscv counterpart yet.
static COMMANDS: [Command; 9] = [
    Command { name: "help", help: "this table", run: kernel_core::shell::cmds::cmd_help },
    Command { name: "lsos", help: "filesystems per partition (probe table)", run: kernel_core::shell::cmds::cmd_lsos },
    Command { name: "lsmnt", help: "mount table", run: kernel_core::shell::cmds::cmd_lsmnt },
    Command { name: "mount", help: "mount esp0 /mnt/esp0 - ro alias only", run: kernel_core::shell::cmds::cmd_mount },
    Command { name: "umount", help: "umount <path>", run: kernel_core::shell::cmds::cmd_umount },
    Command { name: "cat", help: "cat <path> - print a file (FAT or ext4, 4 KiB max)", run: kernel_core::shell::cat::cmd_cat },
    Command { name: "bootinfo", help: "boot handover details", run: kernel_core::shell::cmds::cmd_bootinfo },
    Command { name: "diskhealth", help: "disk health [--scan]", run: kernel_core::shell::cmds::cmd_diskhealth },
    Command { name: "grub-fix", help: "boot repair [diagnose|repair|install]", run: kernel_core::shell::cmds::cmd_grubfix },
];

/// Enter the interactive shell (never returns); idle() is wfi, so the SBI
/// timer keeps scheduling the kernel and user tasks while waiting.
pub fn enter(vfs: Option<Vfs>, bi: &BootInfo) -> ! {
    kernel_core::input::set_poll(crate::uart::getc);
    kernel_core::shell::enter(vfs, bi, 0, &COMMANDS)
}
