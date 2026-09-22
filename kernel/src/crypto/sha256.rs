//! SHA-256 (M8.1): the implementation moved to `kernel-core` so the shared
//! disk imager and this signature path use the same streaming hash; the
//! module path `crate::crypto::sha256` is kept as a re-export.

pub use kernel_core::sha256::*;
