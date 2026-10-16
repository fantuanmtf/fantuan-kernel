//! VFS v1 (M6/M6.5, DESIGN.md §8): mount the first FAT32 partition and the
//! first readable ext4 root (read-only, /mnt/root0) and demonstrate file
//! reads. Rust filesystem logic on top of the C block driver — the layering
//! stays: C does device I/O, Rust does the filesystem.
//!
//! Split in 2026-10, when this file sat exactly on the 300-line rule and
//! M13-6's repair broker had to add to it. `mount` owns the mount sequence,
//! `name83` the FAT name helpers the boot-repair diagnostics share, and
//! `repair` the write gate. Every public path is re-exported below, because
//! callers already name them through `vfs::` — `bootrepair` re-exports
//! `to_8_3`/`find_path`/`eq_8_3` from here and the shell calls
//! `enable_repair_mode`, so moving the code must not move the paths.

pub mod dir;
pub mod ext4;
pub mod fat;
pub mod fat_dir;
pub mod fat_write;
pub mod fd;
pub mod io;
pub mod mount;
pub mod name83;
#[cfg(kconfig_ntfs)]
pub mod ntfs;
#[cfg(kconfig_ntfs)]
mod ntfs_fd;
pub mod part;
pub mod pipe;
pub mod posix;
pub mod posix_path;
pub mod probe;
pub mod repair;
pub mod tmpfs;
pub mod tmpfs_file;

// Re-exports: every path a caller already names. `vfs::init` is the mount
// sequence; `to_8_3`/`eq_8_3`/`find_path` are re-exported onward by
// `bootrepair`; `RepairToken` and `repair_guard` are the write gate that the
// FAT writers and the shell name. Moving the code must not move the paths.
pub use mount::init;
pub use name83::{eq_8_3, find_path, fmt_name, to_8_3};
pub use repair::{enable_repair_mode, repair_guard, repair_mode, write_file, RepairToken};

/// Internal fs-layer sentinel: "would block, retry after a sleep". Never
/// reaches user space (the blocking wrappers loop on it).
pub(crate) const ERR_WOULD_BLOCK: u64 = u64::MAX - 1000;

/// The mounted world: filesystems + partition table, shared with the
/// boot-repair diagnostics (M7).
#[derive(Clone, Copy)]
pub struct Vfs {
    pub fs: fat::Fat32,
    pub table: part::Table,
    /// Index of the mounted FAT32 partition (for NVRAM device paths, M7.6).
    pub fat_part: usize,
    /// First readable ext4 root, mounted ro at /mnt/root0 (M6.5).
    pub root: Option<ext4::Ext4>,
    /// Partition index of ROOT (valid when root is Some).
    pub root_part: usize,
    /// First readable NTFS volume, mounted ro at /mnt/win0 (M12-5).
    #[cfg(kconfig_ntfs)]
    pub win: Option<ntfs::Ntfs>,
    /// Partition index of WIN (valid when win is Some).
    #[cfg(kconfig_ntfs)]
    pub win_part: usize,
}
