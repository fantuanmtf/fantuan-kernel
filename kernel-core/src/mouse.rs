//! PS/2 3-byte packet decoder (M13-3): pure logic shared by the x86_64 and
//! i686 mouse drivers. Each driver owns the 8042 port I/O and IRQ routing; this
//! module turns the raw byte stream into (dx, dy, buttons) triples, handling
//! the sign and overflow bits of the standard packet format.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PointerPacket {
    pub dx: i16,
    pub dy: i16,
    pub buttons: u8,
}

pub struct MouseDecoder {
    bytes: [u8; 3],
    idx: usize,
}

impl MouseDecoder {
    pub const fn new() -> MouseDecoder {
        MouseDecoder {
            bytes: [0; 3],
            idx: 0,
        }
    }

    /// Feed one byte; returns a decoded packet when three bytes have formed a
    /// complete packet. The first byte must have bit 3 set (the "always 1" sync
    /// bit), otherwise the stream is resynchronised.
    pub fn push(&mut self, b: u8) -> Option<PointerPacket> {
        if self.idx == 0 && b & 0x08 == 0 {
            return None;
        }
        self.bytes[self.idx] = b;
        self.idx += 1;
        if self.idx < 3 {
            return None;
        }
        self.idx = 0;
        Some(decode(&self.bytes))
    }
}

fn decode(b: &[u8; 3]) -> PointerPacket {
    let b0 = b[0];
    let left = b0 & 0x01 != 0;
    let right = b0 & 0x02 != 0;
    let middle = b0 & 0x04 != 0;
    let x_sign = b0 & 0x10 != 0;
    let y_sign = b0 & 0x20 != 0;
    let x_ovf = b0 & 0x40 != 0;
    let y_ovf = b0 & 0x80 != 0;

    let mut dx = b[1] as i16;
    let mut dy = b[2] as i16;
    if x_sign {
        dx -= 256;
    }
    if y_sign {
        dy -= 256;
    }
    // Overflow bits mean the movement exceeded the ±255 range; clamp to the
    // full-scale value on the reported sign so a fast move is not lost.
    if x_ovf {
        dx = if x_sign { -255 } else { 255 };
    }
    if y_ovf {
        dy = if y_sign { -255 } else { 255 };
    }

    PointerPacket {
        dx,
        dy,
        buttons: (left as u8) | ((right as u8) << 1) | ((middle as u8) << 2),
    }
}
