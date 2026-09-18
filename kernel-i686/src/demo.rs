//! i686 scheduler demo tasks (M10-4b2b): each reports through the shared
//! scheduler, prints a few rounds, then keeps sleeping so the PIT-driven
//! scheduler is visibly rotating on 32-bit.

use kernel_core::task;

use crate::serial::{put_dec, puts};

const DEMO_PRINTS: u64 = 3;

fn hello(tag: u64) -> ! {
    let id = task::current_id();
    let mut n = 0u64;
    loop {
        if n < DEMO_PRINTS {
            puts("task ");
            put_dec(tag);
            puts(" (tid ");
            put_dec(id);
            puts("): hello ");
            put_dec(n);
            puts("\n");
        } else if n == DEMO_PRINTS {
            puts("task ");
            put_dec(tag);
            puts(": quiet (scheduler keeps rotating)\n");
        }
        n += 1;
        task::sleep_ms(250 + tag * 150);
    }
}

pub fn demo_1() -> ! {
    hello(1)
}

pub fn demo_2() -> ! {
    hello(2)
}
