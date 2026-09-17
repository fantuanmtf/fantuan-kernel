//! Minimal X.509 parsing for RSA certificates: extract the tbsCertificate
//! bytes and the SubjectPublicKeyInfo modulus/exponent, and verify the
//! self-signature. Chain validation is deliberately out of scope — the
//! firmware decides trust when the variable is written.

use super::der::{oid_is, Cursor};
use super::rsa;

/// OID content octets (without the tag/length).
const OID_RSA_ENCRYPTION: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01];
const OID_SHA256_WITH_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B];

pub struct Cert<'a> {
    /// Raw tbsCertificate TLV (the exact bytes the signature covers).
    pub tbs: &'a [u8],
    /// RSA modulus, big-endian without the DER sign byte.
    pub modulus: &'a [u8],
    pub exponent: u32,
    pub signature: &'a [u8],
}

pub fn parse(cert: &[u8]) -> Option<Cert<'_>> {
    let mut c = Cursor::new(cert);
    let outer = c.next()?;
    if outer.tag != 0x30 {
        return None;
    }
    let mut inner = Cursor::new(outer.value);
    let (tbs, tbs_raw) = inner.next_with_raw()?;
    if tbs.tag != 0x30 {
        return None;
    }
    let alg = inner.next()?;
    if !alg_is(alg.value, OID_SHA256_WITH_RSA) {
        return None;
    }
    let bits = inner.next()?;
    if bits.tag != 0x03 || bits.value.is_empty() || bits.value[0] != 0 {
        return None;
    }
    let (modulus, exponent) = spki(&tbs.value)?;
    Some(Cert { tbs: tbs_raw, modulus, exponent, signature: &bits.value[1..] })
}

/// Verify the certificate against its own public key (self-signed path).
pub fn verify_self(cert: &[u8]) -> bool {
    let Some(c) = parse(cert) else {
        return false;
    };
    rsa::verify_pkcs1_v15_sha256(c.modulus, c.exponent, c.signature, c.tbs)
}

fn alg_is(alg_seq: &[u8], oid: &[u8]) -> bool {
    let mut c = Cursor::new(alg_seq);
    matches!(c.next(), Some(t) if oid_is(&t, oid))
}

/// Walk tbsCertificate to the SubjectPublicKeyInfo and pull out (n, e).
fn spki(tbs: &[u8]) -> Option<(&[u8], u32)> {
    let mut c = Cursor::new(tbs);
    if c.peek() == Some(0xA0) {
        c.next()?; // [0] EXPLICIT version
    }
    let _serial = c.next()?;
    let _sig_alg = c.next()?;
    let _issuer = c.next()?;
    let _validity = c.next()?;
    let _subject = c.next()?;
    let spki = c.next()?;
    if spki.tag != 0x30 {
        return None;
    }
    let mut s = Cursor::new(spki.value);
    let alg = s.next()?;
    if !alg_is(alg.value, OID_RSA_ENCRYPTION) {
        return None;
    }
    let bits = s.next()?;
    if bits.tag != 0x03 || bits.value.is_empty() || bits.value[0] != 0 {
        return None;
    }
    let mut k = Cursor::new(&bits.value[1..]);
    let rsa_seq = k.next()?;
    if rsa_seq.tag != 0x30 {
        return None;
    }
    let mut rs = Cursor::new(rsa_seq.value);
    let n = rs.next()?;
    let e = rs.next()?;
    if n.tag != 0x02 || e.tag != 0x02 {
        return None;
    }
    let modulus = n.value.strip_prefix(&[0x00]).unwrap_or(n.value);
    let exponent = parse_u32(e.value)?;
    if modulus.is_empty() || modulus.len() > super::bigint::MAX_BYTES {
        return None;
    }
    Some((modulus, exponent))
}

fn parse_u32(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() || bytes.len() > 4 {
        return None;
    }
    let mut v = 0u32;
    for &b in bytes {
        v = (v << 8) | b as u32;
    }
    Some(v)
}
