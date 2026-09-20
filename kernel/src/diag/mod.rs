//! x86 diagnostics glue: the shared framework and the storage/RAM/disk-health
//! checks live in kernel-core; the CPU and GPU checks (CPUID, SMBIOS, beep
//! codes) stay x86-only. The re-exports keep the historical `crate::diag::*`
//! paths for the stage builders.

pub mod cpu;
#[cfg(kconfig_virt)]
pub mod virt;
#[cfg(kconfig_graphics)]
pub mod gpu;

pub use kernel_core::diag::{diskhealth, ram, run_stage, storage, Check, Severity};
