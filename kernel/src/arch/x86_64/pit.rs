//! PIT (8254): channel 2 PC-speaker beep + channel 0 periodic timer.
//!
//! Beeps are the encoding-independent fallback channel (DESIGN.md §3/§6.2).
//! Millisecond sleep lives in tsc.rs now (channel 0 belongs to the timer).

use crate::consts::PIT_FREQ_HZ;
use crate::port::{inb, outb};
use crate::tsc;

const BEEP_HZ: u64 = 880;

#[derive(Clone, Copy)]
pub enum BeepLen {
    Short,
    Long,
}

/// One PC-speaker beep: PIT channel 2 square wave, gated by port 0x61.
pub fn beep(len: BeepLen) {
    let ms: u64 = match len {
        BeepLen::Short => 120,
        BeepLen::Long => 500,
    };
    let divisor = (PIT_FREQ_HZ / BEEP_HZ) as u16;

    unsafe {
        outb(0x43, 0xB6); // channel 2, lobyte/hibyte, mode 3 (square wave)
        outb(0x42, (divisor & 0xFF) as u8);
        outb(0x42, (divisor >> 8) as u8);

        let gate = inb(0x61);
        outb(0x61, gate | 0x03); // enable speaker + gate channel 2

        tsc::sleep_ms(ms);

        let gate = inb(0x61);
        outb(0x61, gate & !0x03);
    }
}

pub fn beep_n(n: u32, len: BeepLen) {
    for i in 0..n {
        beep(len);
        if i + 1 < n {
            tsc::sleep_ms(150);
        }
    }
}

pub fn beep_long() {
    beep(BeepLen::Long);
}

/// Program channel 0 as a periodic rate generator driving IRQ0.
pub fn init_timer(hz: u32) {
    let divisor = (PIT_FREQ_HZ / hz as u64) as u16;
    unsafe {
        outb(0x43, 0x34); // channel 0, lobyte/hibyte, mode 2 (rate generator)
        outb(0x40, (divisor & 0xFF) as u8);
        outb(0x40, (divisor >> 8) as u8);
    }
}
