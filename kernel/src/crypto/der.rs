//! Minimal DER (ITU-T X.690) reader for the crypto stack: the definite-length
//! subset used by X.509 and PKCS#7 — single-byte tags, short and long form
//! lengths up to 4 bytes. Indefinite lengths, multi-byte tags and trailing
//! garbage are rejected.

pub struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

pub struct Tlv<'a> {
    pub tag: u8,
    pub value: &'a [u8],
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    pub fn next(&mut self) -> Option<Tlv<'a>> {
        self.next_with_raw().map(|(tlv, _)| tlv)
    }

    /// Like next(), but also returns the raw TLV bytes (tag + length + value)
    /// — needed to hash the tbsCertificate exactly as signed.
    pub fn next_with_raw(&mut self) -> Option<(Tlv<'a>, &'a [u8])> {
        let start = self.pos;
        let tag = *self.data.get(self.pos)?;
        if tag & 0x1F == 0x1F {
            return None; // multi-byte tag: outside the supported subset
        }
        self.pos += 1;
        let first = *self.data.get(self.pos)?;
        self.pos += 1;
        let len = if first & 0x80 == 0 {
            first as usize
        } else {
            let n = (first & 0x7F) as usize;
            if n == 0 || n > 4 {
                return None; // indefinite or absurd length
            }
            let mut v = 0usize;
            for _ in 0..n {
                v = (v << 8) | *self.data.get(self.pos)? as usize;
                self.pos += 1;
            }
            v
        };
        if self.pos + len > self.data.len() {
            return None;
        }
        let value = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Some((Tlv { tag, value }, &self.data[start..self.pos]))
    }
}

/// Compare an OBJECT IDENTIFIER TLV's value with the expected content octets.
pub fn oid_is(tlv: &Tlv<'_>, oid: &[u8]) -> bool {
    tlv.tag == 0x06 && tlv.value == oid
}
