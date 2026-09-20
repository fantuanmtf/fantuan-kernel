//! Shared core shell commands (DESIGN.md §10): `help` and `bootinfo`.
//! The rescue/diagnostic commands (lsos, lsmnt, mount, umount, diskhealth,
//! grub-fix) live in shell/rescue.rs behind CONFIG_RESCUE_REPAIR, so the
//! minimal kernel's command table lists only the core builtins.

use core::fmt::Write;

use crate::log::Log;

use super::Shell;

macro_rules! out {
    ($s:expr, $($arg:tt)*) => {
        { let _ = writeln!($s, $($arg)*); }
    };
}

pub fn cmd_help(sh: &mut Shell, s: &mut Log, _args: &[&[u8]]) {
    out!(s, "shell commands (root@{}, DESIGN.md §10):", super::HOSTNAME);
    for cmd in sh.commands() {
        let _ = write!(s, "  {:<11} {}\n", cmd.name, cmd.help);
    }
}

pub fn cmd_bootinfo(sh: &mut Shell, s: &mut Log, _args: &[&[u8]]) {
    let bi = sh.bi;
    out!(s, "  magic {:#010x}  version {}", bi.magic, bi.version);
    out!(s, "  kernel_base {:#x}  stack_top {:#x}  rsdp {:#x}", bi.kernel_base, bi.stack_top, bi.rsdp);
    out!(s, "  caps {:#x}  runtime_services {:#x}  smbios_table {:#x}", bi.caps, bi.runtime_services, bi.smbios_table);
    out!(s, "  fb {}x{} stride {} format {}", bi.fb.width, bi.fb.height, bi.fb.stride, bi.fb.format);
    out!(s, "  memmap entries {} (desc {} bytes)", bi.memmap.count, bi.memmap.desc_size);
    out!(s, "  page tables: pml4 {:#x}, {} pages", bi.boot_pml4, bi.boot_tables_pages);
}
