//! RSA PKCS#1 v1.5 verification (RFC 8017 §8.2.2) with SHA-256, on top of the
//! Montgomery arithmetic in bigint.rs. Public exponent only (no private-key
//! operations — a rescue system verifies signatures, it does not sign).

use super::bigint::{Big, Mont};
use super::sha256;

/// DigestInfo prefix for SHA-256 (RFC 8017 §9.2), 19 bytes + 32-byte digest.
pub const SHA256_DIGEST_INFO: [u8; 19] = [
    0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05,
    0x00, 0x04, 0x20,
];

/// Verify a raw PKCS#1 v1.5 signature over MSG with a SHA-256 digest.
/// N_BE is the modulus in big-endian bytes (the key size), E the public
/// exponent, SIG_BE the signature (same length as the modulus).
pub fn verify_pkcs1_v15_sha256(n_be: &[u8], e: u32, sig_be: &[u8], msg: &[u8]) -> bool {
    if n_be.len() != sig_be.len() || n_be.len() < 3 + 8 + SHA256_DIGEST_INFO.len() + 32 {
        return false;
    }
    let Some(n) = Big::from_be(n_be) else {
        return false;
    };
    let Some(sig) = Big::from_be(sig_be) else {
        return false;
    };
    // RSA requires 0 <= s < n.
    if sig.cmp(&n) != core::cmp::Ordering::Less {
        return false;
    }
    let Some(mont) = Mont::new(&n) else {
        return false;
    };
    let em_big = mont.pow_u32(&sig, e);
    let mut em = [0u8; super::bigint::MAX_BYTES];
    let k = n_be.len();
    em_big.to_be(&mut em[..k]);

    // EM = 0x00 || 0x01 || 0xFF...(>=8) || 0x00 || DigestInfo || H
    if em[0] != 0x00 || em[1] != 0x01 {
        return false;
    }
    let mut i = 2usize;
    while i < k && em[i] == 0xFF {
        i += 1;
    }
    if i - 2 < 8 || i >= k || em[i] != 0x00 {
        return false;
    }
    let t = &em[i + 1..k];
    if t.len() != SHA256_DIGEST_INFO.len() + 32 {
        return false;
    }
    if t[..SHA256_DIGEST_INFO.len()] != SHA256_DIGEST_INFO {
        return false;
    }
    t[SHA256_DIGEST_INFO.len()..] == sha256::digest(msg)
}
