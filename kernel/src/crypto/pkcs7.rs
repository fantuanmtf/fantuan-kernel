//! Minimal PKCS#7 SignedData parsing (RFC 2315 shape) for EFI authenticated
//! variable bundles. Supported subset: attached content, one signer, no
//! authenticated attributes, SHA-256 + rsaEncryption — which is what EFI
//! producers emit. Anything else is rejected, and the firmware still decides
//! chain trust when the variable is written.

use super::der::{oid_is, Cursor, Tlv};
use super::x509;

const OID_SIGNED_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02];
const OID_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01];
const OID_SHA256: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];
const OID_RSA_ENCRYPTION: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01];

pub struct Pkcs7<'a> {
    /// Attached eContent (the signed variable value).
    pub content: &'a [u8],
    /// Raw first certificate TLV (the signer's certificate).
    pub cert: &'a [u8],
    pub signature: &'a [u8],
}

pub fn parse(data: &[u8]) -> Option<Pkcs7<'_>> {
    let mut c = Cursor::new(data);
    let ci = c.next()?;
    if ci.tag != 0x30 {
        return None;
    }
    let mut ci_in = Cursor::new(ci.value);
    let oid = ci_in.next()?;
    if !oid_is(&oid, OID_SIGNED_DATA) {
        return None;
    }
    let explicit = ci_in.next()?;
    if explicit.tag != 0xA0 {
        return None;
    }
    let mut e_in = Cursor::new(explicit.value);
    let sd = e_in.next()?;
    if sd.tag != 0x30 {
        return None;
    }
    let mut sd_in = Cursor::new(sd.value);
    let _version = sd_in.next()?;
    let _digest_algs = sd_in.next()?;
    let content_info = sd_in.next()?;
    let content = content_of(&content_info)?;

    let mut cert = None;
    if sd_in.peek() == Some(0xA0) {
        let certs = sd_in.next()?;
        let mut cc = Cursor::new(certs.value);
        let (first, raw) = cc.next_with_raw()?;
        if first.tag != 0x30 {
            return None;
        }
        cert = Some(raw);
    }
    if sd_in.peek() == Some(0xA1) {
        sd_in.next()?; // crls: ignored
    }
    let signers = sd_in.next()?;
    if signers.tag != 0x31 {
        return None;
    }
    let mut sc = Cursor::new(signers.value);
    let si = sc.next()?;
    if si.tag != 0x30 {
        return None;
    }
    let mut si_in = Cursor::new(si.value);
    let _si_version = si_in.next()?;
    let sid = si_in.next()?;
    if sid.tag != 0x30 && sid.tag != 0x80 {
        return None;
    }
    let digest_alg = si_in.next()?;
    if !alg_is(digest_alg.value, OID_SHA256) {
        return None;
    }
    if si_in.peek() == Some(0xA0) {
        return None; // authenticated attributes: unsupported (EFI does not use them)
    }
    let enc_alg = si_in.next()?;
    if !alg_is(enc_alg.value, OID_RSA_ENCRYPTION) {
        return None;
    }
    let sig = si_in.next()?;
    if sig.tag != 0x04 {
        return None;
    }
    Some(Pkcs7 { content, cert: cert?, signature: sig.value })
}

fn content_of<'a>(ci: &Tlv<'a>) -> Option<&'a [u8]> {
    if ci.tag != 0x30 {
        return None;
    }
    let mut c = Cursor::new(ci.value);
    let oid = c.next()?;
    if !oid_is(&oid, OID_DATA) {
        return None;
    }
    let exp = c.next()?;
    if exp.tag != 0xA0 {
        return None;
    }
    let mut e = Cursor::new(exp.value);
    let oct = e.next()?;
    if oct.tag != 0x04 {
        return None;
    }
    Some(oct.value)
}

fn alg_is(alg_seq: &[u8], oid: &[u8]) -> bool {
    let mut c = Cursor::new(alg_seq);
    matches!(c.next(), Some(t) if oid_is(&t, oid))
}

/// Verify the signature against the embedded certificate and, when EXPECTED
/// is given, that the attached content matches it.
pub fn verify_self(p7: &Pkcs7<'_>, expected: Option<&[u8]>) -> bool {
    if let Some(exp) = expected {
        if p7.content != exp {
            return false;
        }
    }
    let Some(cert) = x509::parse(p7.cert) else {
        return false;
    };
    super::rsa::verify_pkcs1_v15_sha256(cert.modulus, cert.exponent, p7.signature, p7.content)
}
