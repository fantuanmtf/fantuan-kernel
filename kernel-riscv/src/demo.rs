//! RISC-V demo tasks (M9.2c): two kernel tasks printing on the serial UART
//! and sleeping, proving the shared scheduler on this architecture.

use kernel_core::task;

use crate::{put_dec, uart_putc, puts};

fn hello(tag: u8) -> ! {
    let id = task::current_id();
    let mut n = 0u64;
    loop {
        puts("task ");
        uart_putc(tag);
        puts(" (tid ");
        put_dec(id);
        puts("): hello ");
        put_dec(n);
        puts("\n");
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
