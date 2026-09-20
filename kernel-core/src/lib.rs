//! kernel-core — code shared by the x86_64 and riscv64 kernels
//! (docs/M9_KERNEL_v0.0.1.md, incremental extraction).
//!
//! The crate is `no_std` and has no arch code of its own: the few hooks the
//! shared modules need (interrupt state, idle, input, storage accessors, the
//! clock, the phys->virt alias) are installed by each kernel at boot via the
//! matching `set_*` functions.

#![no_std]

pub mod arch;
#[cfg(kconfig_rescue_repair)]
pub mod bootrepair;
pub mod brk;
pub mod diag;
pub mod drv;
pub mod elf;
pub mod frame;
pub mod heartbeat;
pub mod input;
pub mod log;
pub mod mem;
pub mod runtime;
pub mod shell;
pub mod syscall;
pub mod task;
pub mod time;
pub mod user;
pub mod vfs;
