//! VFS v1 (M6, DESIGN.md §8): mount the first FAT32 partition on the first
//! block device and demonstrate file reads. Rust filesystem logic on top of
//! the C block driver — the layering stays: C does device I/O, Rust does the
//! filesystem.

use core::fmt::Write;

use crate::serial::{self, Serial};

pub mod fat;
pub mod part;

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

pub fn init() -> bool {
    let mut s = Serial::new(serial::COM1);
    let Some(table) = part::parse(&mut s) else {
        return false;
    };

    // Find the first FAT32 partition (GPT type GUID or MBR type 0x0B/0x0C).
    let mut target = None;
    for p in &table.parts[..table.count] {
        let is_fat32 = p.type_guid == part::FAT32_GPT_GUID || p.type_guid[0] == 0x0B || p.type_guid[0] == 0x0C;
        let _ = writeln!(
            s,
            "  part: LBA {}..{} ({} sectors) fat32 {}",
            p.first_lba,
            p.last_lba,
            p.last_lba - p.first_lba + 1,
            is_fat32
        );
        if is_fat32 && target.is_none() {
            target = Some(*p);
        }
    }
    let Some(p) = target else {
        let _ = writeln!(s, "vfs: no FAT32 partition");
        return false;
    };
    let Some(fs) = fat::parse(p.first_lba) else {
        let _ = writeln!(s, "vfs: FAT32 BPB parse failed");
        return false;
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
        let mut s2 = Serial::new(serial::COM1);
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

    true
}
