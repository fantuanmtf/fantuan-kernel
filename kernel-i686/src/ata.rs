//! PIO ATA driver for the i686 VFS (M10-4c): primary channel, master, LBA28,
//! 512-byte sectors, IRQ-free polling with bounded timeouts. The shared
//! `kernel-core` VFS reaches the driver through the `blk_read` C symbol — the
//! same seam the x86_64 C driver registry fills. Read-only by design: write
//! and repair paths stay on the UEFI/riscv kernels.

use core::arch::asm;
use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use kernel_core::drv::{BlkIdentity, DrvOps};

use crate::cpu::{inb, outb};

const DATA: u16 = 0x1F0;
const FEATURES: u16 = 0x1F1;
const SECCOUNT: u16 = 0x1F2;
const LBA_LO: u16 = 0x1F3;
const LBA_MID: u16 = 0x1F4;
const LBA_HI: u16 = 0x1F5;
const DRIVE: u16 = 0x1F6;
const STATUS: u16 = 0x1F7;
const CONTROL: u16 = 0x3F6;

const ST_ERR: u8 = 0x01;
const ST_DRQ: u8 = 0x08;
const ST_DF: u8 = 0x20;
const ST_BSY: u8 = 0x80;

const CMD_READ: u8 = 0x20;
const CMD_IDENTIFY: u8 = 0xEC;

/// Bounded poll budget; an absent or wedged device fails instead of hanging
/// (the stage2 ATA path uses the same counter style).
const TIMEOUT: u32 = 2_000_000;
const LBA28_SECTORS: u64 = 1 << 28;

static PRESENT: AtomicBool = AtomicBool::new(false);
static SECTORS: AtomicU32 = AtomicU32::new(0);
static mut MODEL: [u8; 41] = [0; 41];
static mut SERIAL: [u8; 21] = [0; 21];

fn delay_400ns() {
    let _ = inb(CONTROL);
    let _ = inb(CONTROL);
    let _ = inb(CONTROL);
    let _ = inb(CONTROL);
}

fn wait_not_busy() -> bool {
    for _ in 0..TIMEOUT {
        if inb(STATUS) & ST_BSY == 0 {
            return true;
        }
    }
    false
}

fn wait_drq() -> bool {
    for _ in 0..TIMEOUT {
        let st = inb(STATUS);
        if st == 0 || st == 0xFF || st & (ST_ERR | ST_DF) != 0 {
            return false;
        }
        if st & ST_BSY == 0 && st & ST_DRQ != 0 {
            return true;
        }
    }
    false
}

/// Read one 512-byte sector (256 words) from the data port.
fn read_words(dst: *mut u8) {
    unsafe {
        asm!(
            "rep insw",
            in("dx") DATA,
            in("ecx") 256u32,
            in("edi") dst,
            options(nostack, preserves_flags),
        );
    }
}

/// One IDENTIFY word (ATA byte-swaps each 16-bit word on the wire).
fn word(id: &[u8; 512], i: usize) -> u16 {
    u16::from_le_bytes([id[2 * i], id[2 * i + 1]])
}

/// ATA string field: words are byte-swapped, padded with spaces, NUL-ended.
fn ata_string(id: &[u8; 512], first_word: usize, words: usize, out: &mut [u8]) -> usize {
    let mut n = 0;
    for i in 0..words {
        let hi = id[(first_word + i) * 2 + 1];
        let lo = id[(first_word + i) * 2];
        if hi != 0 && n + 1 < out.len() {
            out[n] = hi;
            n += 1;
        }
        if lo != 0 && n + 1 < out.len() {
            out[n] = lo;
            n += 1;
        }
    }
    while n > 0 && out[n - 1] == b' ' {
        n -= 1;
    }
    n
}

/// Probe and IDENTIFY the primary master. False for an absent device, an
/// ATAPI device, or a timeout — the caller then skips the VFS.
pub fn init() -> bool {
    outb(DRIVE, 0xA0);
    delay_400ns();
    if !wait_not_busy() {
        return false;
    }
    let probe = inb(STATUS);
    if probe == 0 || probe == 0xFF {
        return false;
    }

    outb(FEATURES, 0);
    outb(SECCOUNT, 0);
    outb(LBA_LO, 0);
    outb(LBA_MID, 0);
    outb(LBA_HI, 0);
    outb(STATUS, CMD_IDENTIFY);
    if !wait_not_busy() {
        return false;
    }
    if inb(LBA_MID) != 0 || inb(LBA_HI) != 0 {
        return false; // ATAPI signature
    }
    if !wait_drq() {
        return false;
    }

    let mut id = [0u8; 512];
    read_words(id.as_mut_ptr());

    let mut sectors = 0u64;
    if word(&id, 83) & (1 << 10) != 0 {
        for i in 0..4 {
            sectors |= (word(&id, 100 + i) as u64) << (16 * i);
        }
    } else {
        sectors = word(&id, 60) as u64 | (word(&id, 61) as u64) << 16;
    }
    if sectors == 0 {
        return false;
    }

    let mut model = [0u8; 41];
    let mut serial = [0u8; 21];
    ata_string(&id, 27, 20, &mut model);
    ata_string(&id, 10, 10, &mut serial);
    unsafe {
        ptr::copy_nonoverlapping(model.as_ptr(), ptr::addr_of_mut!(MODEL).cast::<u8>(), 41);
        ptr::copy_nonoverlapping(serial.as_ptr(), ptr::addr_of_mut!(SERIAL).cast::<u8>(), 21);
    }
    SECTORS.store(sectors.min(u32::MAX as u64) as u32, Ordering::Relaxed);
    PRESENT.store(true, Ordering::Release);
    true
}

pub fn sectors() -> u32 {
    SECTORS.load(Ordering::Relaxed)
}

/// Read one LBA28 sector into `dst` (512 bytes). Polled, no IRQs.
fn read_sector(lba: u32, dst: *mut u8) -> bool {
    if !wait_not_busy() {
        return false;
    }
    outb(DRIVE, 0xE0 | ((lba >> 24) & 0x0F) as u8);
    delay_400ns();
    outb(FEATURES, 0);
    outb(SECCOUNT, 1);
    outb(LBA_LO, lba as u8);
    outb(LBA_MID, (lba >> 8) as u8);
    outb(LBA_HI, (lba >> 16) as u8);
    outb(STATUS, CMD_READ);
    if !wait_drq() {
        return false;
    }
    read_words(dst);
    true
}

/// The shared VFS block seam (`extern "C" blk_read` in kernel-core).
#[no_mangle]
pub extern "C" fn blk_read(_dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32 {
    if !PRESENT.load(Ordering::Acquire) || buf.is_null() || sectors == 0 {
        return -1;
    }
    let Ok(n) = u32::try_from(sectors) else {
        return -1;
    };
    if lba >= LBA28_SECTORS || lba + n as u64 > LBA28_SECTORS {
        return -1;
    }
    for s in 0..n {
        let dst = unsafe { (buf as *mut u8).add(s as usize * 512) };
        if !read_sector(lba as u32 + s, dst) {
            return -1;
        }
    }
    0
}

/// Write path is deliberately absent on the i686/BIOS kernel (W3 read-only):
/// the shared repair code compiles against the same symbol.
#[no_mangle]
pub extern "C" fn blk_write(_dev: *mut c_void, _lba: u64, _buf: *const c_void, _sectors: usize) -> i32 {
    -1
}

#[no_mangle]
pub extern "C" fn blk_smart_read_data(_dev: *mut c_void, _out: *mut c_void) -> i32 {
    -1
}

#[no_mangle]
pub extern "C" fn blk_smart_read_log(_dev: *mut c_void, _page: u8, _buf: *mut c_void, _sectors: usize) -> i32 {
    -1
}

fn drive_handle() -> *mut c_void {
    ptr::null_mut()
}

fn drive_name() -> &'static str {
    "ata-pio"
}

fn storage_bdf() -> u32 {
    u32::MAX
}

fn drive_identity() -> Option<BlkIdentity> {
    if !PRESENT.load(Ordering::Acquire) {
        return None;
    }
    let mut id = BlkIdentity::EMPTY;
    unsafe {
        ptr::copy_nonoverlapping(ptr::addr_of!(MODEL).cast::<u8>(), id.model.as_mut_ptr(), 41);
        ptr::copy_nonoverlapping(ptr::addr_of!(SERIAL).cast::<u8>(), id.serial.as_mut_ptr(), 21);
    }
    id.sectors = SECTORS.load(Ordering::Relaxed) as u64;
    Some(id)
}

/// Install the shared diagnostics identity hooks (diag stage 2 and any
/// future shell command that asks the active drive for its identity).
pub fn install_hooks() {
    kernel_core::drv::set_ops(DrvOps {
        drive_handle,
        drive_name,
        drive_identity,
        storage_bdf,
    });
}
