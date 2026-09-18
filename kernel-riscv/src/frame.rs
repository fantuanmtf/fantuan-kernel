//! Frame allocator for RISC-V (M9.1b): bitmap over the first 4 GiB, free
//! ranges come from the FDT memory nodes, reserved ranges from the caller
//! (firmware, kernel image, DTB, memreserve). API-compatible with the x86
//! allocator so M9.2a can extract one shared implementation into
//! kernel-core.
//!
//! Temporary local copy (M9.1b): keep the same method set as
//! kernel/src/mm/frame.rs when changing this.

use crate::fdt::MemInfo;

pub const FRAME_SIZE: u64 = 4096;
const BITMAP_MAX: u64 = 4 * 1024 * 1024 * 1024;
pub const BITMAP_BYTES: usize = (BITMAP_MAX / FRAME_SIZE / 8) as usize; // 128 KiB

static mut BITMAP: [u8; BITMAP_BYTES] = [0; BITMAP_BYTES];
static mut ALLOC: FrameAllocator = FrameAllocator { free_frames: 0, next: 0 };

pub struct FrameAllocator {
    free_frames: u64,
    next: usize,
}

pub fn init(mem: &MemInfo) {
    unsafe {
        let a = &mut *core::ptr::addr_of_mut!(ALLOC);
        a.free_frames = 0;
        a.next = 0;
    }
    // Free the FDT memory ranges (everything else stays used).
    for b in &mem.mem[..mem.mem_n] {
        mark_range(b.base, b.base + b.size, true);
    }
    // Reserve firmware/kernel/DTB/memreserve: punch holes.
    for b in &mem.reserved[..mem.reserved_n] {
        mark_range(b.base, b.base + b.size, false);
    }
}

/// Extra reservation after init (kernel image, DTB): updates the free count.
pub fn reserve(begin: u64, end: u64) {
    mark_range(begin, end, false);
}

/// Set/clear the free bits in [begin, end), keeping `free_frames` exact.
fn mark_range(begin: u64, end: u64, free: bool) {
    let first = (begin / FRAME_SIZE) as usize;
    let last = ((end.min(BITMAP_MAX) + FRAME_SIZE - 1) / FRAME_SIZE) as usize;
    for idx in first..last.min(BITMAP_BYTES * 8) {
        let mask = 1u8 << (idx % 8);
        let b = unsafe { &raw mut BITMAP[idx / 8] };
        let was_free = unsafe { *b } & mask != 0;
        if free && !was_free {
            unsafe {
                *b |= mask;
                (*core::ptr::addr_of_mut!(ALLOC)).free_frames += 1;
            }
        } else if !free && was_free {
            unsafe {
                *b &= !mask;
                (*core::ptr::addr_of_mut!(ALLOC)).free_frames -= 1;
            }
        }
    }
}

pub fn get() -> &'static mut FrameAllocator {
    unsafe { &mut *core::ptr::addr_of_mut!(ALLOC) }
}

impl FrameAllocator {
    pub fn alloc(&mut self) -> Option<u64> {
        let total = BITMAP_BYTES * 8;
        for off in 0..total {
            let idx = (self.next + off) % total;
            let mask = 1u8 << (idx % 8);
            let byte = idx / 8;
            if unsafe { *core::ptr::addr_of!(BITMAP[byte]) } & mask != 0 {
                unsafe { *core::ptr::addr_of_mut!(BITMAP[byte]) &= !mask };
                self.free_frames -= 1;
                self.next = (idx + 1) % total;
                return Some(idx as u64 * FRAME_SIZE);
            }
        }
        None
    }

    pub fn free(&mut self, phys: u64) {
        let idx = (phys / FRAME_SIZE) as usize;
        if idx >= BITMAP_BYTES * 8 {
            return;
        }
        let mask = 1u8 << (idx % 8);
        if unsafe { *core::ptr::addr_of!(BITMAP[idx / 8]) } & mask == 0 {
            unsafe { *core::ptr::addr_of_mut!(BITMAP[idx / 8]) |= mask };
            self.free_frames += 1;
        }
    }

    pub fn usable_mib(&self) -> u64 {
        self.free_frames * 4 / 1024
    }
}
