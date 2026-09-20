//! VFS v1 (M6/M6.5, DESIGN.md §8): mount the first FAT32 partition and the
//! first readable ext4 root (read-only, /mnt/root0) and demonstrate file
//! reads. Rust filesystem logic on top of the C block driver — the layering
//! stays: C does device I/O, Rust does the filesystem.

use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::log::Log;

/// Repair mode: writes are impossible until this is explicitly enabled.
/// The rescue iron rule — the kernel never writes what the user did not ask
/// it to fix.
static REPAIR_MODE: AtomicBool = AtomicBool::new(false);

pub fn enable_repair_mode() {
    REPAIR_MODE.store(true, Ordering::Relaxed);
}

/// Whether writes are currently permitted. The repair path checks this, so a
/// stray call can never write with repair mode off.
pub fn repair_mode() -> bool {
    REPAIR_MODE.load(Ordering::Relaxed)
}

/// Capability token proving repair mode is on. The FAT write API takes this
/// token, so a write without explicit operator consent does not compile; the
/// shell's `grub-fix repair` + YES is the only enabler.
#[derive(Clone, Copy)]
pub struct RepairToken(());

/// Obtain the write capability; None while repair mode is off.
pub fn repair_guard() -> Option<RepairToken> {
    if repair_mode() {
        Some(RepairToken(()))
    } else {
        None
    }
}

/// Create or overwrite an 8.3 file in the given directory — gated behind
/// repair mode.
pub fn write_file(fs: &fat::Fat32, dir_cluster: u32, name: &[u8; 11], data: &[u8]) -> bool {
    let Some(token) = repair_guard() else {
        return false;
    };
    fs.write_file(dir_cluster, name, data, &token)
}

pub mod dir;
pub mod ext4;
pub mod fat;
pub mod fat_dir;
pub mod fat_write;
pub mod fd;
pub mod io;
pub mod part;
pub mod pipe;
pub mod posix;
pub mod posix_path;
pub mod probe;
pub mod tmpfs;
pub mod tmpfs_file;

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
}

// --- FAT 8.3 name helpers (shared with bootrepair via re-export) ----------

fn ascii_upper(b: u8) -> u8 {
    if b.is_ascii_lowercase() {
        b - 32
    } else {
        b
    }
}

/// Convert a fixed string to an 8.3 directory name (no allocation).
pub fn to_8_3(s: &str) -> Option<[u8; 11]> {
    let mut n = [b' '; 11];
    let (base, ext) = match s.find('.') {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    if base.is_empty() || base.len() > 8 {
        return None;
    }
    for (i, c) in base.bytes().enumerate() {
        n[i] = ascii_upper(c);
    }
    if let Some(ext) = ext {
        if ext.len() > 3 {
            return None;
        }
        for (i, c) in ext.bytes().enumerate() {
            n[8 + i] = ascii_upper(c);
        }
    }
    Some(n)
}

/// FAT names are case-insensitive: compare 8.3 names accordingly.
pub fn eq_8_3(a: &[u8; 11], b: &[u8; 11]) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

/// Find a file by 8.3 path components under a directory cluster.
/// Returns (cluster, size).
pub fn find_path(fs: &fat::Fat32, start: u32, components: &[&[u8; 11]]) -> Option<(u32, u32)> {
    let mut dir = start;
    for (i, comp) in components.iter().enumerate() {
        let last = i == components.len() - 1;
        let mut found: Option<(u32, u32)> = None;
        fs.walk_dir(dir, |name, attr, cluster, size| {
            if found.is_none() && eq_8_3(name, comp) {
                let is_dir = attr & 0x10 != 0;
                if (last && !is_dir) || (!last && is_dir) {
                    found = Some((cluster, size));
                }
            }
        });
        let (cluster, size) = found?;
        if last {
            return Some((cluster, size));
        }
        dir = cluster;
    }
    None
}

/// Format an 8.3 name as "NAME.EXT" into a fixed buffer.
fn fmt_name(name: &[u8; 11], buf: &mut [u8; 13]) -> usize {
    let mut n = 0;
    for i in 0..8 {
        let c = name[i];
        if c != b' ' {
            buf[n] = c;
            n += 1;
        }
    }
    if name[8] != b' ' {
        buf[n] = b'.';
        n += 1;
        for i in 8..11 {
            let c = name[i];
            if c != b' ' {
                buf[n] = c;
                n += 1;
            }
        }
    }
    n
}

pub fn init() -> Option<Vfs> {
    let mut s = Log::new();
    let Some(table) = part::parse(&mut s) else {
        return None;
    };

    // Find the first FAT32 partition (GPT type GUID or MBR type 0x0B/0x0C).
    let mut target = None;
    let mut target_index = 0;
    for (pi, p) in table.parts[..table.count].iter().enumerate() {
        let is_fat32 = p.type_guid == part::FAT32_GPT_GUID || p.type_guid[0] == 0x0B || p.type_guid[0] == 0x0C;
        let _ = writeln!(
            s,
            "  part: LBA {}..{} ({} sectors) fat32 {}",
            p.first_lba,
            p.last_lba,
            p.last_lba.saturating_sub(p.first_lba) + 1,
            is_fat32
        );
        if is_fat32 && target.is_none() {
            target = Some(*p);
            target_index = pi;
        }
    }
    let Some(p) = target else {
        let _ = writeln!(s, "vfs: no FAT32 partition");
        return None;
    };
    let Some(fs) = fat::parse(p.first_lba) else {
        let _ = writeln!(s, "vfs: FAT32 BPB parse failed");
        return None;
    };
    let _ = writeln!(
        s,
        "vfs: mounted FAT32 at /mnt/disk0 (spc {} spf {} root cluster {})",
        fs.sectors_per_cluster, fs.sectors_per_fat, fs.root_cluster
    );

    // List the root directory; remember HELLO.TXT and INFO.TXT.
    let mut hello: Option<(u32, u32)> = None;
    let mut info: Option<(u32, u32)> = None;
    fs.walk_dir(fs.root_cluster, |name, attr, cluster, size| {
        let mut buf = [0u8; 13];
        let n = fmt_name(name, &mut buf);
        let mut s2 = Log::new();
        let _ = write!(s2, "vfs: root: ");
        let _ = s2.write(&buf[..n]);
        let _ = writeln!(s2, " {} bytes{}", size, if attr & 0x10 != 0 { " (dir)" } else { "" });
        if &buf[..n] == b"HELLO.TXT" {
            hello = Some((cluster, size));
        } else if &buf[..n] == b"INFO.TXT" {
            info = Some((cluster, size));
        }
    });

    // Read HELLO.TXT (single-cluster file).
    if let Some((cluster, size)) = hello {
        let mut buf = [0u8; 256];
        if let Some(n) = fs.read_file(cluster, size, &mut buf) {
            let _ = writeln!(s, "vfs: HELLO.TXT => {:?}", core::str::from_utf8(&buf[..n.min(64)]).unwrap_or("?"));
        } else {
            let _ = writeln!(s, "vfs: HELLO.TXT read failed");
        }
    } else {
        let _ = writeln!(s, "vfs: HELLO.TXT not found");
    }

    // Read INFO.TXT (two-cluster file: exercises the chain walk).
    if let Some((cluster, size)) = info {
        let mut buf = [0u8; 1200];
        if let Some(n) = fs.read_file(cluster, size, &mut buf) {
            let all_x = buf[..n].iter().all(|&b| b == b'X');
            let _ = writeln!(s, "vfs: INFO.TXT => {} bytes read, all-X {}", n, all_x);
        } else {
            let _ = writeln!(s, "vfs: INFO.TXT read failed");
        }
    } else {
        let _ = writeln!(s, "vfs: INFO.TXT not found");
    }

    // M6.5: mount the first readable ext4 root, read-only, at /mnt/root0.
    // A filesystem that only carries the magic (fixture stubs, trashed
    // superblocks) fails parse and stays probe-only.
    let mut root: Option<ext4::Ext4> = None;
    let mut root_index: Option<usize> = None;
    for (pi, p) in table.parts[..table.count].iter().enumerate() {
        if let Some(e) = ext4::Ext4::parse(p.first_lba) {
            e.describe(&mut s);
            let _ = writeln!(s, "ext4: mounted ro at /mnt/root0 (part {})", pi + 1);
            root = Some(e);
            root_index = Some(pi);
            break;
        }
    }
    if root.is_none() {
        let _ = writeln!(s, "vfs: no ext4 root mounted");
    }

    unsafe { probe::init(&table, target_index, root_index) };

    Some(Vfs {
        fs,
        table,
        fat_part: target_index,
        root,
        root_part: root_index.unwrap_or(0),
    })
}
