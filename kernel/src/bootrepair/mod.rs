//! x86 boot-repair glue: the shared diagnosis/repair stack lives in
//! kernel-core and is re-exported here. Authenticated-variable bundles need
//! the x86 crypto stack (M8.1b), so that one module stays local; the kernel
//! installs it as a kernel-core hook before the shell runs.

pub use kernel_core::bootrepair::*;

pub mod secureboot_auth;
