//! grub.cfg parser (M7, DESIGN.md §9): extract search.fs_uuid and the root
//! device from EFI/ubuntu/grub.cfg on the ESP. Read-only.

use core::fmt::Write;

use super::{find_path, to_8_3};
use crate::log::Log;
use crate::vfs::fat::Fat32;

pub struct GrubConfig {
    /// search.fs_uuid value, lowercase hex text.
    pub fs_uuid: [u8; 40],
    pub fs_uuid_len: usize,
    /// set root='...' value.
    pub root_dev: [u8; 32],
    pub root_dev_len: usize,
}

pub fn parse(s: &mut Log, fs: &Fat32) -> Option<GrubConfig> {
    let path = [
        to_8_3("EFI")?,
        to_8_3("ubuntu")?,
        to_8_3("grub.cfg")?,
    ];
    let (cluster, size) = find_path(fs, fs.root_cluster, &[&path[0], &path[1], &path[2]])?;
    let mut buf = [0u8; 512];
    let n = fs.read_file(cluster, size.min(512), &mut buf)?;
    let text = &buf[..n];

    let mut cfg = GrubConfig {
        fs_uuid: [0; 40],
        fs_uuid_len: 0,
        root_dev: [0; 32],
        root_dev_len: 0,
    };
    for line in text.split(|&b| b == b'\n') {
        if let Some(rest) = line.strip_prefix(b"search.fs_uuid ") {
            let token = rest.split(|&b| b == b' ').next().unwrap_or(&[]);
            if cfg.fs_uuid_len == 0 && !token.is_empty() && token.len() <= 40 {
                cfg.fs_uuid[..token.len()].copy_from_slice(token);
                cfg.fs_uuid_len = token.len();
            }
        } else if let Some(rest) = line.strip_prefix(b"search --fs-uuid ") {
            let token = rest.split(|&b| b == b' ').next().unwrap_or(&[]);
            if cfg.fs_uuid_len == 0 && !token.is_empty() && token.len() <= 40 {
                cfg.fs_uuid[..token.len()].copy_from_slice(token);
                cfg.fs_uuid_len = token.len();
            }
        } else if let Some(rest) = line.strip_prefix(b"set root='") {
            let end = rest.iter().position(|&b| b == b'\'').unwrap_or(rest.len());
            if cfg.root_dev_len == 0 && end <= 32 {
                cfg.root_dev[..end].copy_from_slice(&rest[..end]);
                cfg.root_dev_len = end;
            }
        }
    }

    let _ = write!(s, "bootrepair: grub.cfg: root='");
    let _ = s.write(&cfg.root_dev[..cfg.root_dev_len]);
    let _ = write!(s, "' fs_uuid=");
    let _ = s.write(&cfg.fs_uuid[..cfg.fs_uuid_len]);
    let _ = writeln!(s);
    Some(cfg)
}
