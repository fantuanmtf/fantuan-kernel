//! Memory management (M2/M4): frame allocator, kernel and user page tables
//! (DESIGN.md §4.5/§4.6).

pub mod frame;
pub mod lock;
pub mod paging;
pub mod user;
