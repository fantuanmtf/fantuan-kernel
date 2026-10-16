//! FDT shim for the riscv boot. The parser is shared (`kernel_core::fdt`);
//! what stays here is the boot-time report, because `tools/smoke-riscv.sh`
//! asserts its exact wording, and the type name this port's callers use.
//!
//! Before 2026-10 this file carried its own tokenizer, identical to the
//! aarch64 one except that it read a fixed 512 KiB window and did not stop
//! `copy_str` at the NUL. Merging kept the aarch64 behaviour, so riscv
//! inherits both fixes.

use crate::{put_bytes, put_dec, put_hex, puts};

pub use kernel_core::fdt::{parse, Fdt};

/// The name this port's callers already use for the parsed tree.
pub type MemInfo = Fdt;

/// Print the memory map and CPU report (bare-mode UART; call before paging).
pub fn print_info(mem: &Fdt) {
    for i in 0..mem.mem_n {
        puts("fdt: memory ");
        put_hex(mem.mem[i].base);
        puts("..");
        put_hex(mem.mem[i].base + mem.mem[i].size);
        puts("\n");
    }
    puts("cpu: ");
    put_dec(mem.cpus as u64);
    puts(" hart(s), isa=");
    put_bytes(&mem.isa[..mem.isa_len]);
    puts(", model=");
    put_bytes(&mem.model[..mem.model_len]);
    puts("\n");
}
