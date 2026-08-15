//! RAM quick test (DESIGN.md §6.1): write/read patterns over frames borrowed
//! from the frame allocator, so the test never touches live kernel memory.
//! A quick sample, not memtest86 — deep own-address tests are optional and
//! background (M5.5). Read-only with respect to anything that matters.

use core::fmt::Write;

use super::Severity;
use crate::mm::frame;
use crate::mm::paging::phys_to_virt;
use crate::serial::Serial;

const TEST_FRAMES: usize = 8; // 32 KiB sample
const WORDS_PER_FRAME: usize = 512; // 4096 / 8
const PATTERNS: [u64; 3] = [
    0xAAAA_AAAA_AAAA_AAAA,
    0x5555_5555_5555_5555,
    0x0102_0408_1020_4080,
];

pub fn check(s: &mut Serial) -> Severity {
    let mut frames = [0u64; TEST_FRAMES];
    for f in frames.iter_mut() {
        *f = frame::get().alloc().expect("no frames for RAM test");
    }

    let mut mismatches = 0u64;
    for &f in &frames {
        let base = phys_to_virt(f) as *mut u64;
        for &pat in &PATTERNS {
            for i in 0..WORDS_PER_FRAME {
                unsafe { *base.add(i) = pat };
            }
            for i in 0..WORDS_PER_FRAME {
                if unsafe { *base.add(i) } != pat {
                    mismatches += 1;
                }
            }
        }
        for i in 0..WORDS_PER_FRAME {
            unsafe { *base.add(i) = 0 };
        }
    }

    for f in frames {
        frame::get().free(f);
    }

    let _ = writeln!(
        s,
        "  ram: {} frames x {} patterns, {} mismatches",
        TEST_FRAMES,
        PATTERNS.len(),
        mismatches
    );
    if mismatches == 0 {
        Severity::Ok
    } else {
        Severity::Critical
    }
}
