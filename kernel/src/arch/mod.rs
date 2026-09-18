//! Architecture layer (DESIGN.md §14): everything ISA-specific lives under
//! `arch/<isa>/`. M9.0 carries the x86_64 implementation; the riscv64 one
//! follows in M9.1. The crate root re-exports the modules under their
//! historical names (`crate::gdt`, `crate::pic`, ...), so the generic core
//! does not need to know this layout.

#[cfg(target_arch = "x86_64")]
pub mod x86_64;
