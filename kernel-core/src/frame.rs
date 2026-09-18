//! Bitmap frame allocator over the EFI memory map (DESIGN.md §4.5).
//!
//! Covers the first 4 GiB — the identity-mapped span the bootloader set up.
//! Scope notes: allocations are a linear scan (fine at boot) and RAM beyond
//! 4 GiB is ignored until the map is extended. `alloc_contiguous` backs
//! stacks, which the CPU walks as one linear region. A second ownership
//! bitmap makes `free()` reject frames the caller never allocated (double
//! frees, reserved kernel pages); the boot-time bootloader teardown uses
//! `reclaim()`.
//!
//! Interrupt safety (M8.3a): every mutating entry point takes the mm::lock
//! IrqLock, so the IRQ0 scheduler path (which will reap tasks and free
//! frames, M8.3b) can never preempt a task mid-allocation and deadlock.
//! The method bodies delegate to private *_unlocked helpers, so the lock
//! is never taken recursively.

use core::ptr;
use core::sync::atomic::AtomicBool;

use fantuan_abi::BootInfo;

use crate::arch::IrqLock;
use crate::mem::to_usize;

pub const FRAME_SIZE: u64 = 4096;
const BITMAP_MAX: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB
pub const BITMAP_BYTES: usize = (BITMAP_MAX / FRAME_SIZE / 8) as usize; // 128 KiB
const LOW_MEMORY_CUTOFF: u64 = 0x10_0000; // never hand out frames below 1 MiB

/// 0 = used, 1 = free. Lives in the kernel .bss (protected by the
/// kernel-image hole). static mut is acceptable here: M2 uses it strictly
/// single-context via FrameAllocator.
static mut BITMAP: [u8; BITMAP_BYTES] = [0; BITMAP_BYTES];

/// 1 = the allocator handed this frame out and free() may take it back.
/// Separate from BITMAP so `free()` can reject frames the caller never owned
/// (reserved kernel/BootInfo pages, firmware frames) and double frees.
static mut OWNED: [u8; BITMAP_BYTES] = [0; BITMAP_BYTES];

pub struct FrameAllocator {
    free_frames: u64,
    next: usize,
}

/// M4: the allocator is a proper global (M3 kept it as a kmain local).
/// Single-context access via get(); still not interrupt-safe (callers hold
/// interrupts off or run before the scheduler starts).
static mut FRAME_ALLOCATOR: FrameAllocator = FrameAllocator { free_frames: 0, next: 0 };

/// Serializes mutating allocator calls (see mm::lock).
static LOCK: AtomicBool = AtomicBool::new(false);

/// Initialize from a BootInfo memory map. KERNEL_END_PHYS is the physical
/// end of the kernel image (.bss); EXTRA_RESERVED lists ranges the map does
/// not mark reserved (e.g. RISC-V firmware/DTB/memreserve regions).
pub fn init(bi: &BootInfo, kernel_end_phys: u64, extra_reserved: &[(u64, u64)]) {
    unsafe {
        FRAME_ALLOCATOR = FrameAllocator::new(bi, kernel_end_phys, extra_reserved);
    }
}

pub fn get() -> &'static mut FrameAllocator {
    unsafe { &mut *ptr::addr_of_mut!(FRAME_ALLOCATOR) }
}

impl FrameAllocator {
    fn new(bi: &BootInfo, kernel_end_phys: u64, extra_reserved: &[(u64, u64)]) -> Self {
        unsafe {
            BITMAP = [0; BITMAP_BYTES];
            OWNED = [0; BITMAP_BYTES];
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
        //    f) caller-provided reservations (RISC-V firmware/DTB/memreserve)
        for &(begin, end) in extra_reserved {
            a.mark_range(begin, end, false);
        }

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
        let Some(first) = to_usize(begin.max(LOW_MEMORY_CUTOFF) / FRAME_SIZE) else { return };
        let Some(last) = to_usize((end.min(BITMAP_MAX) + FRAME_SIZE - 1) / FRAME_SIZE) else {
            return;
        };
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
        let _g = IrqLock::acquire(&LOCK);
        self.alloc_unlocked()
    }

    fn alloc_unlocked(&mut self) -> Option<u64> {
        let total_bits = BITMAP_BYTES * 8;
        for off in 0..total_bits {
            let idx = (self.next + off) % total_bits;
            let mask = 1u8 << (idx % 8);
            let byte = idx / 8;
            if unsafe { *ptr::addr_of!(BITMAP[byte]) } & mask != 0 {
                unsafe {
                    *ptr::addr_of_mut!(BITMAP[byte]) &= !mask;
                    *ptr::addr_of_mut!(OWNED[byte]) |= mask;
                }
                self.free_frames -= 1;
                self.next = (idx + 1) % total_bits;
                return Some(idx as u64 * FRAME_SIZE);
            }
        }
        None
    }

    /// Return a frame the allocator handed out. Frames that were never
    /// allocated (reserved kernel/BootInfo pages, firmware memory) are
    /// refused; use reclaim() for the boot-time bootloader teardown.
    pub fn free(&mut self, phys: u64) {
        let _g = IrqLock::acquire(&LOCK);
        self.free_unlocked(phys)
    }

    fn free_unlocked(&mut self, phys: u64) {
        let Some(idx) = to_usize(phys / FRAME_SIZE) else { return };
        if idx >= BITMAP_BYTES * 8 {
            return; // outside bitmap coverage
        }
        let mask = 1u8 << (idx % 8);
        let byte = idx / 8;
        if unsafe { *ptr::addr_of!(OWNED[byte]) } & mask == 0 {
            return; // not ours (or already freed)
        }
        unsafe {
            *ptr::addr_of_mut!(OWNED[byte]) &= !mask;
            *ptr::addr_of_mut!(BITMAP[byte]) |= mask;
        }
        self.free_frames += 1;
    }

    /// Boot-time reclaim of a RESERVED frame (e.g. the bootloader's page
    /// tables after the CR3 switch). The only path that may free a frame the
    /// allocator never handed out; refuses frames that are already free or
    /// currently owned.
    pub fn reclaim(&mut self, phys: u64) {
        let _g = IrqLock::acquire(&LOCK);
        self.reclaim_unlocked(phys)
    }

    fn reclaim_unlocked(&mut self, phys: u64) {
        let Some(idx) = to_usize(phys / FRAME_SIZE) else { return };
        if idx >= BITMAP_BYTES * 8 {
            return;
        }
        let mask = 1u8 << (idx % 8);
        let byte = idx / 8;
        let is_free = unsafe { *ptr::addr_of!(BITMAP[byte]) } & mask != 0;
        let is_owned = unsafe { *ptr::addr_of!(OWNED[byte]) } & mask != 0;
        if is_free || is_owned {
            return;
        }
        unsafe { *ptr::addr_of_mut!(BITMAP[byte]) |= mask };
        self.free_frames += 1;
    }

    /// Allocate FRAMES physically contiguous frames; returns the first
    /// physical address. Stacks are used as one linear region, so a run of
    /// separate alloc() calls (which may straddle reserved holes) is not
    /// enough. The scan starts at the round-robin cursor and never lets a run
    /// cross the bitmap end, so the result is always contiguous in physical
    /// memory.
    pub fn alloc_contiguous(&mut self, frames: usize) -> Option<u64> {
        let _g = IrqLock::acquire(&LOCK);
        self.alloc_contiguous_unlocked(frames)
    }

    fn alloc_contiguous_unlocked(&mut self, frames: usize) -> Option<u64> {
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
                        unsafe {
                            *ptr::addr_of_mut!(BITMAP[b]) &= !m;
                            *ptr::addr_of_mut!(OWNED[b]) |= m;
                        }
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
