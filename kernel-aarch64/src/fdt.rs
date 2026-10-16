//! FDT shim for the aarch64 direct-FDT boot. The parser is shared
//! (`kernel_core::fdt`); what stays here is the boot-time report, because
//! `tools/smoke-aarch64.sh` asserts its exact wording, and the UART fallback
//! QEMU's generated map is expected to provide.
//!
//! Before 2026-10 this file carried its own tokenizer, identical to the riscv
//! one except that it read the 40-byte header first and stopped `copy_str` at
//! the NUL. Those are the behaviours the merged parser kept.

use crate::{put_bytes, put_dec, put_hex, puts};

pub use kernel_core::fdt::{parse, Fdt};

/// QEMU virt UART base, used when the device tree carries no pl011 node.
pub const UART_FALLBACK: usize = 0x0900_0000;

/// The name this port's callers already use for the parsed tree.
pub type MemInfo = Fdt;

/// Print the memory/CPU/UART report (bare-mode UART; call before the MMU).
pub fn print_info(mem: &Fdt) {
    for i in 0..mem.mem_n {
        puts("fdt: memory ");
        put_hex(mem.mem[i].base);
        puts("..");
        put_hex(mem.mem[i].base + mem.mem[i].size);
        puts("\n");
    }
    puts("fdt: model=");
    put_bytes(&mem.model[..mem.model_len]);
    puts("\n");
    puts("fdt: uart=");
    let uart = if mem.uart != 0 { mem.uart as usize } else { UART_FALLBACK };
    put_hex(uart as u64);
    puts(" (pl011)\n");
    puts("cpu: ");
    put_dec(mem.cpus as u64);
    puts(" cpu(s), model=");
    put_bytes(&mem.model[..mem.model_len]);
    puts("\n");
}
