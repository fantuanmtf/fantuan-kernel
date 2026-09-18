//! kernel-core — code shared by the x86_64 and riscv64 kernels
//! (docs/M9_KERNEL_v0.0.1.md, incremental extraction).
//!
//! The crate is `no_std` and has no arch code of its own: the few hooks the
//! shared modules need (interrupt state save/restore) are installed by each
//! kernel at boot via `arch::set_irq_ops`.

#![no_std]

pub mod arch;
pub mod frame;
pub mod task;
