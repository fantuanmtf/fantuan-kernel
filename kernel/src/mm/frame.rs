//! Bitmap frame allocator over the EFI memory map (DESIGN.md §4.5).
//!
//! Covers the first 4 GiB — the identity-mapped span the bootloader set up.
//! Scope notes: allocations are a linear scan (fine at boot), the allocator is
//! still not interrupt-safe (callers are single-context), and RAM beyond 4 GiB
//! is ignored until the map is extended. `alloc_contiguous` backs stacks,
//! which the CPU walks as one linear region.

use core::ptr;

use fantuan_abi::{BootInfo, PHYS_OFFSET};

pub const FRAME_SIZE: u64 = 4096;
const BITMAP_MAX: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB
pub const BITMAP_BYTES: usize = (BITMAP_MAX / FRAME_SIZE / 8) as usize; // 128 KiB
const LOW_MEMORY_CUTOFF: u64 = 0x10_0000; // never hand out frames below 1 MiB

/// 0 = used, 1 = free. Lives in the kernel .bss (protected by the
/// kernel-image hole). static mut is acceptable here: M2 uses it strictly
/// single-context via FrameAllocator.
static mut BITMAP: [u8; BITMAP_BYTES] = [0; BITMAP_BYTES];

// Kernel image end, defined in link.ld.
extern "C" {
    static __bss_end: u8;
}

pub struct FrameAllocator {
    free_frames: u64,
    next: usize,
}

/// M4: the allocator is a proper global (M3 kept it as a kmain local).
/// Single-context access via get(); still not interrupt-safe (callers hold
/// interrupts off or run before the scheduler starts).
static mut FRAME_ALLOCATOR: FrameAllocator = FrameAllocator { free_frames: 0, next: 0 };

pub fn init(bi: &BootInfo) {
    unsafe {
        FRAME_ALLOCATOR = FrameAllocator::new(bi);
    }
}

pub fn get() -> &'static mut FrameAllocator {
    unsafe { &mut *ptr::addr_of_mut!(FRAME_ALLOCATOR) }
}

impl FrameAllocator {
    pub fn new(bi: &BootInfo) -> Self {
        unsafe {
            BITMAP = [0; BITMAP_BYTES];
        }
        let mut a = FrameAllocator { free_frames: 0, next: 0 };

        // 1. Mark reclaimable memory free, clamped to [1 MiB, 4 GiB).
        //    Types: LoaderCode/Data, BootServices Code/Data, Conventional,
        //    and Unaccepted (type 15: usable in non-confidential VMs — same
        //    caveat as the bootloader's region check).
        let n = bi.memmap.count;
        let mut ptr = bi.memmap.ptr;
        for _ in 0..n {
            let d = unsafe { &*ptr };
            if matches!(d.type_, 1 | 2 | 3 | 4 | 7 | 15) {
                let start = d.physical_start.max(LOW_MEMORY_CUTOFF);
                let end = (d.physical_start + d.number_of_pages * FRAME_SIZE).min(BITMAP_MAX);
                if start < end {
                    a.mark_range(start, end, true);
                }
            }
            ptr = unsafe { (ptr as *const u8).add(bi.memmap.desc_size) as *const _ };
        }

        // 2. Punch holes for everything that must stay alive.
        //    a) kernel image
        let kernel_end_phys = ptr::addr_of!(__bss_end) as *const u8 as u64 - PHYS_OFFSET;
        a.mark_range(bi.kernel_base, kernel_end_phys, false);
        //    b) initial kernel stack
        let stack_bottom = bi.stack_top - fantuan_abi::BOOT_STACK_PAGES * FRAME_SIZE;
        a.mark_range(stack_bottom, bi.stack_top, false);
        //    c) the BootInfo struct's page
        let bi_page = (bi as *const BootInfo as u64) & !(FRAME_SIZE - 1);
        a.mark_range(bi_page, bi_page + FRAME_SIZE, false);
        //    d) EFI memory-map buffer
        let map_end = bi.memmap.ptr as u64 + (bi.memmap.count as u64) * (bi.memmap.desc_size as u64);
        a.mark_range(bi.memmap.ptr as u64, map_end, false);
        //    e) bootloader page tables (CR3 points at them until we switch)
        a.mark_range(bi.boot_pml4, bi.boot_pml4 + bi.boot_tables_pages * FRAME_SIZE, false);

        // 3. Count free frames.
        unsafe {
            for i in 0..BITMAP_BYTES {
                a.free_frames += (*ptr::addr_of!(BITMAP[i])).count_ones() as u64;
            }
        }
        a
    }

    /// Set the bit for frames in [begin, end). Range-clamped, frame-aligned
    /// outward; frames below the 1 MiB cutoff are never touched.
    fn mark_range(&mut self, begin: u64, end: u64, free: bool) {
        let first = ((begin.max(LOW_MEMORY_CUTOFF)) / FRAME_SIZE) as usize;
        let last = ((end.min(BITMAP_MAX) + FRAME_SIZE - 1) / FRAME_SIZE) as usize;
        for idx in first..last.min(BITMAP_BYTES * 8) {
            let mask = 1u8 << (idx % 8);
            let byte = idx / 8;
            let b = unsafe { &raw mut BITMAP[byte] };
            unsafe {
                if free {
                    *b |= mask;
                } else {
                    *b &= !mask;
                }
            }
        }
    }

    /// First-fit allocation; returns the PHYSICAL address. Use
    /// mm::paging::phys_to_virt to access it.
    pub fn alloc(&mut self) -> Option<u64> {
        let total_bits = BITMAP_BYTES * 8;
        for off in 0..total_bits {
            let idx = (self.next + off) % total_bits;
            let mask = 1u8 << (idx % 8);
            let byte = idx / 8;
            if unsafe { *ptr::addr_of!(BITMAP[byte]) } & mask != 0 {
                unsafe {
                    *ptr::addr_of_mut!(BITMAP[byte]) &= !mask;
                }
                self.free_frames -= 1;
                self.next = (idx + 1) % total_bits;
                return Some(idx as u64 * FRAME_SIZE);
            }
        }
        None
    }

    pub fn free(&mut self, phys: u64) {
        let idx = (phys / FRAME_SIZE) as usize;
        if idx >= BITMAP_BYTES * 8 {
            return; // outside bitmap coverage
        }
        let mask = 1u8 << (idx % 8);
        let byte = idx / 8;
        if unsafe { *ptr::addr_of!(BITMAP[byte]) } & mask == 0 {
            unsafe {
                *ptr::addr_of_mut!(BITMAP[byte]) |= mask;
            }
            self.free_frames += 1;
        }
    }

    /// Allocate FRAMES physically contiguous frames; returns the first
    /// physical address. Stacks are used as one linear region, so a run of
    /// separate alloc() calls (which may straddle reserved holes) is not
    /// enough. The scan starts at the round-robin cursor and never lets a run
    /// cross the bitmap end, so the result is always contiguous in physical
    /// memory.
    pub fn alloc_contiguous(&mut self, frames: usize) -> Option<u64> {
        let total = BITMAP_BYTES * 8;
        if frames == 0 || frames > total {
            return None;
        }
        let start = self.next % total;
        if let Some(p) = self.scan_run(start, total, frames) {
            return Some(p);
        }
        if start > 0 {
            if let Some(p) = self.scan_run(0, start, frames) {
                return Some(p);
            }
        }
        None
    }

    /// Find and claim a run of FRAMES free frames in [begin, end).
    fn scan_run(&mut self, begin: usize, end: usize, frames: usize) -> Option<u64> {
        let total = BITMAP_BYTES * 8;
        let mut run = 0usize;
        let mut first = 0usize;
        let mut idx = begin;
        while idx < end {
            let byte = idx / 8;
            let mask = 1u8 << (idx % 8);
            let free = unsafe { *ptr::addr_of!(BITMAP[byte]) } & mask != 0;
            if free {
                if run == 0 {
                    first = idx;
                }
                run += 1;
                if run == frames {
                    for i in first..first + frames {
                        let b = i / 8;
                        let m = 1u8 << (i % 8);
                        unsafe { *ptr::addr_of_mut!(BITMAP[b]) &= !m };
                    }
                    self.free_frames -= frames as u64;
                    self.next = (first + frames) % total;
                    return Some(first as u64 * FRAME_SIZE);
                }
            } else {
                run = 0;
            }
            idx += 1;
        }
        None
    }

    pub fn usable_mib(&self) -> u64 {
        self.free_frames * 4 / 1024
    }
}
