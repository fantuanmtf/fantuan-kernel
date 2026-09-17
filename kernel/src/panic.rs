//! Panic handler: no allocation, no console dependency — raw bytes to COM1,
//! then the fatal beep. Split out of main.rs to keep every file inside the
//! size rule.

use core::fmt::Write;
use core::panic::PanicInfo;

use crate::bootlog::{halt_forever, serial};
use crate::pit;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut s = serial();
    let _ = writeln!(s, "\nPANIC: {}", info);
    pit::beep_n(4, pit::BeepLen::Short);
    halt_forever()
}
