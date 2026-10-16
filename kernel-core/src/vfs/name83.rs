//! FAT 8.3 name helpers: build, compare, format and resolve the fixed 11-byte
//! directory names. Shared with the boot-repair diagnostics, which re-export
//! `to_8_3`, `eq_8_3` and `find_path` through `vfs::`. A child of `vfs`.

use super::fat;

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
pub fn fmt_name(name: &[u8; 11], buf: &mut [u8; 13]) -> usize {
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
