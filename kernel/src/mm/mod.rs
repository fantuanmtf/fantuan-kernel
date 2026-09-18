//! Memory management (M2/M4): frame allocator, kernel and user page tables
//! (DESIGN.md §4.5/§4.6).

pub mod frame;
pub mod lock;

// Page tables are ISA-specific (x86_64 4-level here; Sv39 in M9.1) and stay
// reachable as crate::mm::paging / crate::mm::user.
pub use crate::arch::x86_64::{paging, user};
