//! Fixed-size big integers + Montgomery modular arithmetic for RSA
//! verification. Up to 4096-bit operands; methods process only the modulus's
//! used limbs, so a 2048-bit key costs half of the worst case.

use core::cmp::Ordering;

pub const MAX_LIMBS: usize = 64; // 4096 bits
pub const MAX_BYTES: usize = MAX_LIMBS * 8;

/// Little-endian limb array (l[0] = least significant).
#[derive(Clone, Copy)]
pub struct Big {
    pub l: [u64; MAX_LIMBS],
}

impl Big {
    pub const fn zero() -> Self {
        Self { l: [0; MAX_LIMBS] }
    }

    /// Parse big-endian bytes (any length up to 4096 bits).
    pub fn from_be(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            return None;
        }
        let mut b = Big::zero();
        let mut limb = 0usize;
        let mut cur = 0u64;
        let mut shift = 0u32;
        for &byte in bytes.iter().rev() {
            cur |= (byte as u64) << shift;
            shift += 8;
            if shift == 64 {
                b.l[limb] = cur;
                limb += 1;
                cur = 0;
                shift = 0;
            }
        }
        if shift > 0 {
            b.l[limb] = cur;
        }
        Some(b)
    }

    /// Write the value big-endian into OUT, zero-padded on the left. OUT must
    /// be long enough for `used()` limbs.
    pub fn to_be(&self, out: &mut [u8]) {
        for b in out.iter_mut() {
            *b = 0;
        }
        let n = out.len();
        for (i, limb) in self.l[..self.used()].iter().enumerate() {
            let bytes = limb.to_be_bytes();
            let start = n - (i + 1) * 8;
            out[start..start + 8].copy_from_slice(&bytes);
        }
    }

    /// Index of the highest non-zero limb + 1.
    pub fn used(&self) -> usize {
        for i in (0..MAX_LIMBS).rev() {
            if self.l[i] != 0 {
                return i + 1;
            }
        }
        0
    }

    pub fn cmp(&self, o: &Self) -> Ordering {
        for i in (0..MAX_LIMBS).rev() {
            if self.l[i] != o.l[i] {
                return self.l[i].cmp(&o.l[i]);
            }
        }
        Ordering::Equal
    }

    /// self -= o (assumes self >= o).
    pub fn sub_assign(&mut self, o: &Self) {
        let mut borrow = 0u64;
        for i in 0..MAX_LIMBS {
            let (r, b1) = self.l[i].overflowing_sub(o.l[i]);
            let (r2, b2) = r.overflowing_sub(borrow);
            self.l[i] = r2;
            borrow = (b1 as u64) + (b2 as u64);
        }
    }

    /// Shift left one bit; returns the carry out of the top limb.
    pub fn shl1(&mut self) -> u64 {
        let mut carry = 0u64;
        for limb in self.l.iter_mut() {
            let next = *limb >> 63;
            *limb = (*limb << 1) | carry;
            carry = next;
        }
        carry
    }
}

/// Montgomery context for an odd modulus N (R = 2^(64*used limbs)).
pub struct Mont {
    n: Big,
    l: usize,
    ninv: u64,
}

impl Mont {
    /// Build for an odd modulus with at least one limb; None otherwise.
    pub fn new(n: &Big) -> Option<Self> {
        let l = n.used();
        if l == 0 || n.l[0] & 1 == 0 {
            return None;
        }
        let n0 = n.l[0];
        // Newton iteration for n^{-1} mod 2^64, then negate.
        let mut inv = 1u64;
        for _ in 0..63 {
            inv = inv.wrapping_mul(2u64.wrapping_sub(n0.wrapping_mul(inv)));
        }
        Some(Self { n: *n, l, ninv: inv.wrapping_neg() })
    }

    fn dbl_mod(&self, x: &mut Big) {
        let carry = x.shl1();
        if carry != 0 || x.cmp(&self.n) != Ordering::Less {
            x.sub_assign(&self.n);
        }
    }

    /// R^2 mod N, built by doubling (cheap: O(l) per step).
    fn r2(&self) -> Big {
        let mut x = Big::zero();
        x.l[0] = 1;
        for _ in 0..(2 * 64 * self.l) {
            self.dbl_mod(&mut x);
        }
        x
    }

    /// out = a * b * R^{-1} mod N (CIOS, Koç).
    pub fn mul(&self, a: &Big, b: &Big, out: &mut Big) {
        let l = self.l;
        let mut t = [0u64; MAX_LIMBS + 2];
        for i in 0..l {
            let ai = a.l[i];
            let mut carry = 0u64;
            for j in 0..l {
                let (lo, hi) = mul_add(ai, b.l[j], t[j], carry);
                t[j] = lo;
                carry = hi;
            }
            let (s, c) = t[l].overflowing_add(carry);
            t[l] = s;
            t[l + 1] = c as u64;

            let m = t[0].wrapping_mul(self.ninv);
            let (_, mut carry) = mul_add(m, self.n.l[0], t[0], 0);
            for j in 1..l {
                let (lo, hi) = mul_add(m, self.n.l[j], t[j], carry);
                t[j - 1] = lo;
                carry = hi;
            }
            let (s, c) = t[l].overflowing_add(carry);
            t[l - 1] = s;
            t[l] = t[l + 1].wrapping_add(c as u64);
        }
        for i in 0..l {
            out.l[i] = t[i];
        }
        for i in l..MAX_LIMBS {
            out.l[i] = 0;
        }
        // The result is < 2N; one conditional subtraction settles it.
        if t[l] != 0 || out.cmp(&self.n) != Ordering::Less {
            out.sub_assign(&self.n);
        }
    }

    pub fn to_mont(&self, a: &Big) -> Big {
        let r2 = self.r2();
        let mut out = Big::zero();
        self.mul(a, &r2, &mut out);
        out
    }

    pub fn from_mont(&self, a: &Big) -> Big {
        let mut one = Big::zero();
        one.l[0] = 1;
        let mut out = Big::zero();
        self.mul(a, &one, &mut out);
        out
    }

    /// base^e mod N in normal form; BASE must be < N.
    pub fn pow_u32(&self, base: &Big, e: u32) -> Big {
        let bm = self.to_mont(base);
        let mut one = Big::zero();
        one.l[0] = 1;
        let mut acc = self.to_mont(&one);
        let mut started = false;
        for bit in (0..32).rev() {
            if started {
                let mut sq = Big::zero();
                self.mul(&acc, &acc, &mut sq);
                acc = sq;
            }
            if (e >> bit) & 1 != 0 {
                if started {
                    let mut p = Big::zero();
                    self.mul(&acc, &bm, &mut p);
                    acc = p;
                } else {
                    acc = bm;
                    started = true;
                }
            }
        }
        self.from_mont(&acc)
    }
}

/// (a*b + c + carry) -> (low, high) with a 128-bit intermediate.
fn mul_add(a: u64, b: u64, c: u64, carry: u64) -> (u64, u64) {
    let prod = (a as u128) * (b as u128) + (c as u128) + (carry as u128);
    (prod as u64, (prod >> 64) as u64)
}
