//! aarch64 demo tasks (M11 R9a): two kernel tasks printing on the PL011 and
//! sleeping, proving the shared scheduler on this architecture. Printing
//! stops after a few rounds so the interactive shell is not flooded; the
//! tasks keep sleeping so the scheduler still has work to rotate.

use kernel_core::task;

use crate::{put_dec, puts, uart_putc};

const DEMO_PRINTS: u64 = 3;

fn hello(tag: u8) -> ! {
    let id = task::current_id();
    let mut n = 0u64;
    loop {
        if n < DEMO_PRINTS {
            puts("task ");
            uart_putc(tag);
            puts(" (tid ");
            put_dec(id);
            puts("): hello ");
            put_dec(n);
            puts("\n");
        } else if n == DEMO_PRINTS {
            puts("task ");
            uart_putc(tag);
            puts(": quiet (scheduler keeps rotating)\n");
        }
        n += 1;
        task::sleep_ms(500);
    }
}

pub fn demo_1() -> ! {
    hello(b'1')
}

pub fn demo_2() -> ! {
    hello(b'2')
}
