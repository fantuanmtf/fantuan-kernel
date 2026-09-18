//! Secure Boot key enrollment (M7.7, DESIGN.md §9). In Setup Mode the UEFI
//! spec allows an unauthenticated platform-key update, so a rescue system can
//! hand Secure Boot control back to the operator: certificates are read from
//! the ESP, written with NV|BS|RT, and verified by re-reading.
//!
//! Once a PK exists the firmware leaves Setup Mode and KEK/db updates require
//! EFI_VARIABLE_AUTHENTICATION_2 (PKCS#7 over SHA-256) — that needs a crypto
//! stack and is deferred to the crypto milestone (M8). Callers must have
//! enabled repair mode: this changes platform security state.

use core::fmt::Write;

use crate::runtime::{Runtime, GLOBAL_GUID};
use crate::log::Log;
use crate::vfs::Vfs;

use super::nvram::{ascii_to_utf16, read_by_name, read_by_name_status};
use super::{find_path, to_8_3};

const NV_BS_RT: u32 = 0x7;
/// Certificate blob cap (real DER certs are ~1 KiB).
const CERT_MAX: usize = 2048;

/// Presence/size of a key variable (a too-small buffer still proves it
/// exists — certs are far larger than the 4-byte probe below).
fn key_state(rt: &Runtime, name: &[u8]) -> Option<usize> {
    let mut probe = [0u8; 4];
    read_by_name_status(rt, name, &mut probe)
}

/// Load a certificate fixture from \EFI\fantuan\<file> on the ESP.
fn load_cert(vfs: &Vfs, file: &str, buf: &mut [u8]) -> Option<usize> {
    let efi = to_8_3("EFI")?;
    let dir = to_8_3("fantuan")?;
    let f = to_8_3(file)?;
    let (cluster, size) = find_path(&vfs.fs, vfs.fs.root_cluster, &[&efi, &dir, &f])?;
    let want = (size as usize).min(buf.len()).min(CERT_MAX);
    vfs.fs.read_file(cluster, want as u32, &mut buf[..want])
}

pub(super) fn enroll(s: &mut Log, rt: &Runtime, vfs: &Vfs) {
    // Setup Mode is the precondition: without it the firmware rejects
    // unauthenticated key writes.
    let mut mode = [0u8; 4];
    let _ = read_by_name_status(rt, b"SetupMode", &mut mode);
    let setup_active = mode[0] != 0;
    if !setup_active {
        let _ = writeln!(s, "secureboot: not in Setup Mode — key enrollment skipped");
        return;
    }

    let mut buf = [0u8; CERT_MAX];
    let mut enrolled_pk = false;

    // Platform key first: enrolling it takes the platform out of Setup Mode.
    match load_cert(vfs, "PK.cer", &mut buf) {
        Some(n) => {
            let _ = writeln!(s, "secureboot: SetupMode ACTIVE — enrolling PK from EFI/fantuan/PK.cer ({} bytes)", n);
            let mut name = [0u16; 32];
            ascii_to_utf16(b"PK", &mut name);
            let sts = rt.set_variable(&name, &GLOBAL_GUID, NV_BS_RT, &buf[..n]);
            let mut verify = [0u8; CERT_MAX];
            let got = read_by_name(rt, b"PK", &mut verify);
            let ok = sts == 0 && got == Some(n) && verify[..n] == buf[..n];
            let _ = writeln!(s, "secureboot: PK write sts={:#x}, verified {}", sts, ok);
            if !ok {
                // The usual cause is a variable store that is not in
                // authenticated format: keyless OVMF vars templates are plain
                // stores, so the firmware rejects Secure Boot variables
                // (EFI_INVALID_PARAMETER) even in Setup Mode.
                let _ = writeln!(
                    s,
                    "secureboot: enrollment refused — this firmware's variable store does not accept authenticated variables"
                );
                let _ = writeln!(
                    s,
                    "secureboot: (needs a Secure-Boot-enabled OVMF build with an empty auth var store to enroll)"
                );
            }
            enrolled_pk = ok;
        }
        None => {
            let _ = writeln!(s, "secureboot: no PK certificate at EFI/fantuan/PK.cer — nothing to enroll");
        }
    }

    if !enrolled_pk {
        return;
    }

    // The platform leaves Setup Mode on PK enrollment; report the security
    // consequence loudly and check whether KEK/db are still writable.
    let mut after = [0u8; 4];
    let _ = read_by_name_status(rt, b"SetupMode", &mut after);
    let still_setup = after[0] != 0;
    let _ = writeln!(s, "secureboot: WARNING Secure Boot will be enforced on the next boot");
    if still_setup {
        for file in ["KEK.cer", "db.cer"] {
            let Some(n) = load_cert(vfs, file, &mut buf) else { continue };
            let var: &[u8] = if file == "KEK.cer" { b"KEK" } else { b"db" };
            let mut name = [0u16; 32];
            ascii_to_utf16(var, &mut name);
            let sts = rt.set_variable(&name, &GLOBAL_GUID, NV_BS_RT, &buf[..n]);
            let mut verify = [0u8; CERT_MAX];
            let got = read_by_name(rt, var, &mut verify);
            let ok = sts == 0 && got == Some(n);
            let _ = writeln!(
                s,
                "secureboot: {} write sts={:#x}, verified {}",
                core::str::from_utf8(var).unwrap_or("?"),
                sts,
                ok
            );
        }
    } else {
        let _ = writeln!(s, "secureboot: SetupMode now off — KEK/db need authenticated writes (M8)");
    }
}

/// Report-only key inventory (used before the repair decision).
pub(super) fn report(s: &mut Log, rt: &Runtime) {
    for var in [&b"PK"[..], b"KEK", b"db"] {
        let name = core::str::from_utf8(var).unwrap_or("?");
        match key_state(rt, var) {
            Some(size) => {
                let _ = writeln!(s, "secureboot: {} present ({} bytes)", name, size);
            }
            None => {
                let _ = writeln!(s, "secureboot: {} absent", name);
            }
        }
    }
}
