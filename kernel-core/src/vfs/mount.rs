//! The mount sequence: probe the partition table, mount the first FAT32
//! partition, then the first readable ext4 root (read-only, `/mnt/root0`) and
//! the first readable NTFS volume (`/mnt/win0`, CONFIG_NTFS), and hand the
//! result to the probe table. A child of `vfs`; `vfs::init` is a re-export of
//! `init` here.

use core::fmt::Write;

use super::{ext4, fat, part, probe, Vfs};
use super::name83::fmt_name;
#[cfg(kconfig_ntfs)]
use super::ntfs;
use crate::log::Log;

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

    // M12-5: mount the first readable NTFS volume read-only at /mnt/win0.
    #[cfg(kconfig_ntfs)]
    let (win, win_part) = match ntfs::mount_first(&table, &mut s) {
        Some((n, i)) => (Some(n), Some(i)),
        None => (None, None),
    };
    #[cfg(not(kconfig_ntfs))]
    let win_part: Option<usize> = None;

    unsafe { probe::init(&table, target_index, root_index, win_part) };

    Some(Vfs {
        fs,
        table,
        fat_part: target_index,
        root,
        root_part: root_index.unwrap_or(0),
        #[cfg(kconfig_ntfs)]
        win,
        #[cfg(kconfig_ntfs)]
        win_part: win_part.unwrap_or(0),
    })
}
