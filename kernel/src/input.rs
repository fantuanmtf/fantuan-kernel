//! Merged input source (M8.5a): serial first, then the PS/2 keyboard. The
//! shell reads through this so a byte from either channel behaves the same;
//! each byte is consumed once.

use crate::kbd;
use crate::serial::{Serial, COM1};

/// Non-blocking read of one byte from any input channel.
pub fn poll_byte() -> Option<u8> {
    if let Some(b) = Serial::new(COM1).read() {
        return Some(b);
    }
    kbd::getc()
}
