//! Apply operator-provided authenticated variable bundles (M8.1b, DESIGN.md
//! §9.1). The ESP carries EFI/fantuan/<VAR>.AUTH files holding an
//! EFI_VARIABLE_AUTHENTICATION_2 descriptor (EFI_TIME + WIN_CERTIFICATE_UEFI_GUID
//! with a PKCS#7 SignedData) followed by the new variable value, exactly as
//! the UEFI spec requires for TIME_BASED authenticated writes.
//!
//! The kernel proves internal consistency only (descriptor shape, PKCS#7
//! signature against the embedded certificate, content match). Chain trust
//! is the firmware's decision: the bundle is passed through with
//! EFI_VARIABLE_TIME_BASED_AUTHENTICATED_WRITE_ACCESS and the result is
//! reported honestly.

use core::fmt::Write;

use crate::crypto::pkcs7;
use crate::runtime::{Runtime, GLOBAL_GUID};
use kernel_core::log::Log;
use crate::vfs::Vfs;

use super::nvram::ascii_to_utf16;
use super::{find_path, to_8_3};

/// Bundle cap (a real KEK/db .auth is a few KiB).
const BLOB_MAX: usize = 8192;
static mut BLOB: [u8; BLOB_MAX] = [0; BLOB_MAX];

const NV_BS_RT: u32 = 0x7;
/// EFI_VARIABLE_TIME_BASED_AUTHENTICATED_WRITE_ACCESS (UEFI 2.10 §8.2.3).
const TIME_AUTH: u32 = 0x40;
/// WIN_CERT_TYPE_EFI_GUID.
const WIN_CERT_TYPE_EFI_GUID: u16 = 0x0EF1;
/// gEfiCertPkcs7Guid (4aafd29d-68df-49ee-8aa9-347d375665a7) on disk.
const PKCS7_GUID: [u8; 16] = [
    0x9d, 0xd2, 0xaf, 0x4a, 0xdf, 0x68, 0xee, 0x49, 0x8a, 0xa9, 0x34, 0x7d, 0x37, 0x56, 0x65, 0xa7,
];

/// Load EFI/fantuan/FILE into the static blob; returns the length.
fn load_bundle(vfs: &Vfs, file: &str) -> Option<usize> {
    let efi = to_8_3("EFI")?;
    let dir = to_8_3("fantuan")?;
    let f = to_8_3(file)?;
    let (cluster, size) = find_path(&vfs.fs, vfs.fs.root_cluster, &[&efi, &dir, &f])?;
    let want = (size as usize).min(BLOB_MAX);
    if want < 16 + 24 {
        return None;
    }
    let blob = unsafe { &mut *core::ptr::addr_of_mut!(BLOB) };
    vfs.fs.read_file(cluster, want as u32, &mut blob[..want])?;
    Some(want)
}

/// Parse and apply one bundle. Returns false when nothing writeable was
/// found (missing file), and reports every verification step.
fn apply_one(s: &mut Log, rt: &Runtime, vfs: &Vfs, file: &str, var: &[u8]) -> bool {
    let var_name = core::str::from_utf8(var).unwrap_or("?");
    let Some(n) = load_bundle(vfs, file) else {
        return false;
    };
    let blob = unsafe { &*core::ptr::addr_of!(BLOB) };
    let data = &blob[..n];

    // EFI_VARIABLE_AUTHENTICATION_2: 16-byte EFI_TIME + WIN_CERTIFICATE_UEFI_GUID.
    let dw_len = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;
    let wcert_type = u16::from_le_bytes([data[22], data[23]]);
    if wcert_type != WIN_CERT_TYPE_EFI_GUID || data[24..40] != PKCS7_GUID {
        let _ = writeln!(s, "secureboot: {} has an unsupported AuthInfo type", file);
        return true;
    }
    if dw_len < 24 || 16 + dw_len > n {
        let _ = writeln!(s, "secureboot: {} AuthInfo length {} is inconsistent", file, dw_len);
        return true;
    }
    let p7_der = &data[40..16 + dw_len];
    let payload = &data[16 + dw_len..];

    let Some(p7) = pkcs7::parse(p7_der) else {
        let _ = writeln!(s, "secureboot: {} PKCS#7 parse failed (unsupported shape)", file);
        return true;
    };
    if !pkcs7::verify_self(&p7, Some(payload)) {
        let _ = writeln!(
            s,
            "secureboot: {} PKCS#7 self-verify FAILED — signature does not match the embedded certificate",
            file
        );
        return true;
    }
    let _ = writeln!(
        s,
        "secureboot: {} descriptor ok, PKCS#7 self-verified ({} bytes payload), applying",
        file,
        payload.len()
    );
    // Informational: a self-signed bundle is the common PK case; KEK/db
    // bundles are usually signed by an intermediate, so "false" is fine.
    let self_signed = crate::crypto::x509::verify_self(p7.cert);
    let _ = writeln!(s, "secureboot: {} signer certificate self-signed: {}", file, self_signed);

    let mut name = [0u16; 32];
    ascii_to_utf16(var, &mut name);
    let sts = rt.set_variable(&name, &GLOBAL_GUID, NV_BS_RT | TIME_AUTH, data);
    if sts == 0 {
        let _ = writeln!(s, "secureboot: {} write accepted (sts=0)", var_name);
    } else {
        let _ = writeln!(
            s,
            "secureboot: {} write sts={:#x} — firmware rejected the chain (expected without a matching KEK/PK)",
            var_name, sts
        );
    }
    true
}

/// Apply every bundle present on the ESP: PK/KEK/db .auth files. The FAT
/// reader only sees 8.3 aliases, so "PK.auth" is found as PK.AUT (and so on
/// for KEK/DB; longer base names become NAME~1.AUT and are not matched).
/// Callers must have enabled repair mode (this changes platform trust state).
pub fn apply(s: &mut Log, rt: &Runtime, vfs: &Vfs) {
    let mut seen = false;
    seen |= apply_one(s, rt, vfs, "PK.AUT", b"PK");
    seen |= apply_one(s, rt, vfs, "KEK.AUT", b"KEK");
    seen |= apply_one(s, rt, vfs, "DB.AUT", b"db");
    if !seen {
        let _ = writeln!(s, "secureboot: no EFI/fantuan/*.aut bundles on the ESP");
    }
}
