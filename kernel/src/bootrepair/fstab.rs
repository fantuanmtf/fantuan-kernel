//! fstab parser (M7, DESIGN.md §9): UUID= and PARTUUID= entries. The real
//! fstab lives on the ext4 root; the demo reads a copy placed on the ESP.
//! Read-only.

use core::fmt::Write;

use super::{find_path, to_8_3};
use crate::serial::Serial;
use crate::vfs::fat::Fat32;

#[derive(Clone, Copy)]
pub struct FstabEntry {
    /// UUID= / PARTUUID= value (lowercase hex text).
    pub spec: [u8; 40],
    pub spec_len: usize,
    pub is_partuuid: bool,
    pub mount: [u8; 20],
    pub mount_len: usize,
}

impl FstabEntry {
    pub const fn none() -> Self {
        Self { spec: [0; 40], spec_len: 0, is_partuuid: false, mount: [0; 20], mount_len: 0 }
    }
}

pub fn parse(s: &mut Serial, fs: &Fat32, out: &mut [FstabEntry; 4]) -> usize {
    let Some(path) = to_8_3("FSTAB") else {
        return 0;
    };
    let Some((cluster, size)) = find_path(fs, fs.root_cluster, &[&path]) else {
        let _ = writeln!(s, "bootrepair: fstab not found on ESP");
        return 0;
    };
    let mut buf = [0u8; 512];
    let Some(n) = fs.read_file(cluster, size.min(512), &mut buf) else {
        return 0;
    };
    let text = &buf[..n];

    let mut count = 0;
    for line in text.split(|&b| b == b'\n') {
        if count >= 4 {
            break;
        }
        let mut fields = line.split(|&b| b == b' ' || b == b'\t').filter(|f| !f.is_empty());
        let Some(spec) = fields.next() else { continue };
        let Some(mount) = fields.next() else { continue };

        let (is_partuuid, value) = if let Some(v) = spec.strip_prefix(b"PARTUUID=") {
            (true, v)
        } else if let Some(v) = spec.strip_prefix(b"UUID=") {
            (false, v)
        } else {
            continue;
        };
        if value.len() > 40 || mount.len() > 20 {
            continue;
        }
        let e = &mut out[count];
        e.spec[..value.len()].copy_from_slice(value);
        e.spec_len = value.len();
        e.is_partuuid = is_partuuid;
        e.mount[..mount.len()].copy_from_slice(mount);
        e.mount_len = mount.len();

        let _ = write!(s, "bootrepair: fstab: {}=", if is_partuuid { "PARTUUID" } else { "UUID" });
        let _ = s.write(&e.spec[..e.spec_len]);
        let _ = write!(s, " -> ");
        let _ = s.write(&e.mount[..e.mount_len]);
        let _ = writeln!(s);
        count += 1;
    }
    count
}
