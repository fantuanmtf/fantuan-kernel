//! Crypto stack (M8.1, DESIGN.md §12): SHA-256, fixed-limb Montgomery
//! arithmetic, RSA PKCS#1 v1.5 verification. Minimal by design — a rescue
//! system verifies signatures (and later parses PKCS#7/X.509); it never
//! holds private keys.
//!
//! Verification levels are explicit: this module proves that a signature is
//! internally consistent with the certificate embedded in the same bundle.
//! Trust in the certificate chain is the firmware's decision when the
//! variable is written.

pub mod bigint;
pub mod der;
pub mod pkcs7;
pub mod rsa;
pub mod sha256;
pub mod x509;
mod vectors;

use core::fmt::Write;

use crate::serial::Serial;

/// Message the RSA vector in vectors.rs was generated over.
const SELFTEST_MSG: &[u8] = b"fantuan crypto selftest vector";

fn hex_eq(bytes: &[u8], hex: &[u8]) -> bool {
    const HEX: &[u8] = b"0123456789abcdef";
    if hex.len() != bytes.len() * 2 {
        return false;
    }
    for (i, b) in bytes.iter().enumerate() {
        if hex[i * 2] != HEX[(b >> 4) as usize] || hex[i * 2 + 1] != HEX[(b & 0xF) as usize] {
            return false;
        }
    }
    true
}

fn report(s: &mut Serial, verbose: bool, name: &str, pass: bool) {
    if verbose || !pass {
        let _ = writeln!(s, "crypto: {} {}", name, if pass { "ok" } else { "FAILED" });
    }
}

/// Known-answer tests: FIPS 180-4 SHA-256 vectors plus an OpenSSL-generated
/// RSA-2048 PKCS#1 v1.5 signature (positive and corrupted negative). VERBOSE
/// logs every vector (shell command); boot logs the summary only.
pub fn selftest(s: &mut Serial, verbose: bool) -> bool {
    let mut ok = true;

    // FIPS 180-4 §B.1/B.2 examples.
    let vectors: [(&str, &[u8], &[u8]); 3] = [
        (
            "sha256 empty",
            b"",
            b"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            "sha256 abc",
            b"abc",
            b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            "sha256 448-bit",
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            b"248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
    ];
    for (name, data, expect) in vectors {
        let pass = hex_eq(&sha256::digest(data), expect);
        report(s, verbose, name, pass);
        ok &= pass;
    }

    // One million 'a' (FIPS 180-4 §B.3).
    let mut h = sha256::Sha256::new();
    let chunk = [b'a'; 1000];
    for _ in 0..1000 {
        h.update(&chunk);
    }
    let pass = hex_eq(&h.finish(), b"cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
    report(s, verbose, "sha256 1e6 x 'a'", pass);
    ok &= pass;

    // RSA-2048 PKCS#1 v1.5 over SELFTEST_MSG.
    let pass = rsa::verify_pkcs1_v15_sha256(&vectors::RSA_N, 65537, &vectors::RSA_SIG, SELFTEST_MSG);
    report(s, verbose, "rsa-2048 pkcs1v15 sha256", pass);
    ok &= pass;

    // A corrupted signature must be rejected.
    let mut bad = vectors::RSA_SIG;
    bad[0] ^= 0x01;
    let pass = !rsa::verify_pkcs1_v15_sha256(&vectors::RSA_N, 65537, &bad, SELFTEST_MSG);
    report(s, verbose, "rsa-2048 corrupted rejected", pass);
    ok &= pass;

    ok
}
