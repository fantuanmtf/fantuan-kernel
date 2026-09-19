//! i686 user mode (M10-4b3b): per-task page directories, the shared ELF32
//! loader and the ring-3 task spawn path. The stage2 page directory stays the
//! kernel's; each user task gets a fresh PD whose kernel half (PDEs 767..1024:
//! the 0xBFC00000 framebuffer slot plus the PHYS_OFFSET alias) is copied and
//! whose user half is empty. Ring 3
//! passes eax = number and ebx/ecx/edx/esi/edi as the five arguments; the
//! shared dispatch in `kernel_core::syscall` provides the semantics
//! (SYS_WRITE is (buf, len), see fantuan-abi).

use core::arch::asm;
use core::ptr;

use kernel_core::frame;
use kernel_core::task::{self, State, Task};
use kernel_core::user::Prot;

use crate::gdt;
use crate::serial;

pub const USER_BASE: u32 = 0x0040_0000;
const USER_STACK_TOP: u32 = 0x0080_0000;
const USER_STACK_PAGES: u32 = 4;
const USER_STACK_BASE: u32 = USER_STACK_TOP - USER_STACK_PAGES * 4096;
const USER_LIMIT: u32 = USER_STACK_TOP;

const P_PRESENT: u32 = 1;
const P_WRITABLE: u32 = 1 << 1;
const P_USER: u32 = 1 << 2;
const P_PSE: u32 = 1 << 7;
const ADDR_MASK: u32 = 0xFFFF_F000;
/// First PDE of the kernel half (0xC0000000 / 4 MiB, PSE).
const KERNEL_PDE: usize = 768;
/// PDE of the 0xBFC00000 VBE framebuffer slot (M10-5); cloned into every user
/// PD so the mirrored console stays visible from ring 3.
const FB_PDE: usize = 767;

static FAULT_STUB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/user_stub.bin"));

include!(concat!(env!("OUT_DIR"), "/user_i686.rs"));

extern "C" {
    fn user_entry();
}

fn cr3() -> u32 {
    let v: u32;
    unsafe { asm!("mov {}, cr3", out(reg) v, options(nomem, nostack)) };
    v
}

/// Fresh PD: user half empty, kernel-half alias PDEs copied from the active
/// page directory. Access is always through the PHYS_OFFSET alias, so the
/// identity half is deliberately not copied.
pub fn new_root() -> u64 {
    let pd_phys = frame::get().alloc().expect("no frame for user PD") as u32;
    unsafe {
        let pd = crate::phys_to_virt(pd_phys as u64) as *mut u32;
        let src = crate::phys_to_virt(cr3() as u64) as *const u32;
        for i in 0..FB_PDE {
            pd.add(i).write_volatile(0);
        }
        for i in FB_PDE..1024 {
            pd.add(i).write_volatile(src.add(i).read_volatile());
        }
    }
    pd_phys as u64
}

/// Map one 4K user page, allocating its PT on demand. Only user VAs are
/// passed here; the kernel reaches every frame through the alias.
pub fn map(root: u64, va: u64, pa: u64, prot: Prot) {
    let (Ok(va), Ok(pa)) = (u32::try_from(va), u32::try_from(pa)) else {
        return;
    };
    let root = root as u32;
    unsafe {
        let pd = crate::phys_to_virt(root as u64) as *mut u32;
        let pdi = ((va >> 22) & 0x3FF) as isize;
        let pti = ((va >> 12) & 0x3FF) as isize;
        let mut entry = pd.offset(pdi).read_volatile();
        if entry & P_PRESENT == 0 || entry & P_PSE != 0 {
            let pt_phys = frame::get().alloc().expect("no frame for user PT") as u32;
            let pt = crate::phys_to_virt(pt_phys as u64) as *mut u32;
            for i in 0..1024 {
                pt.add(i).write_volatile(0);
            }
            entry = pt_phys | P_PRESENT | P_WRITABLE | P_USER;
            pd.offset(pdi).write_volatile(entry);
        }
        let flags = P_PRESENT | P_USER | if prot.write() { P_WRITABLE } else { 0 };
        let pt = crate::phys_to_virt((entry & ADDR_MASK) as u64) as *mut u32;
        pt.offset(pti).write_volatile(pa | flags);
    }
}

/// Tear a user address space down: every 4K leaf in the user half, its PTs,
/// then the PD. Huge (PSE) entries are skipped defensively.
pub fn free_root(vm: u64) {
    unsafe {
        let pd = crate::phys_to_virt(vm) as *mut u32;
        for pdi in 0..KERNEL_PDE {
            let e = pd.add(pdi).read_volatile();
            if e & P_PRESENT == 0 || e & P_PSE != 0 {
                continue;
            }
            let pt = crate::phys_to_virt((e & ADDR_MASK) as u64) as *mut u32;
            for pti in 0..1024 {
                let leaf = pt.add(pti).read_volatile();
                if leaf & P_PRESENT != 0 {
                    frame::get().free((leaf & ADDR_MASK) as u64);
                }
            }
            frame::get().free((e & ADDR_MASK) as u64);
        }
    }
    frame::get().free(vm);
}

/// Kernel-stack frame: the four callee-saved registers context_switch pops,
/// the trampoline as its ret target, then the ring-3 iret frame.
fn register_user(root: u64, entry: u32) -> Option<u64> {
    let (kstack_phys, kstack_top) = task::alloc_kernel_stack()?;
    let sp = (kstack_top - 10 * 4) as *mut u32;
    unsafe {
        sp.add(0).write(0); // edi
        sp.add(1).write(0); // esi
        sp.add(2).write(0); // ebx
        sp.add(3).write(0); // ebp
        sp.add(4).write(user_entry as *const () as u32);
        sp.add(5).write(entry); // eip
        sp.add(6).write(gdt::UCODE_SEL as u32);
        sp.add(7).write(0x202); // eflags: IF set
        sp.add(8).write(USER_STACK_TOP);
        sp.add(9).write(gdt::UDATA_SEL as u32);
    }
    task::register(Task {
        state: State::Ready,
        rsp: sp as u64,
        vm_root: root,
        kernel_stack_top: kstack_top,
        is_user: true,
        stack_phys: kstack_phys,
        body: task::dead_body,
        id: 0,
        exit_code: 0,
    })
}

/// Spawn the shared user crate (ELF32) into a per-task address space (M10-4b3b).
pub fn spawn_user(elf_image: &[u8]) -> Option<u64> {
    if !task::has_free_slot() {
        serial::puts("user: no free task slot\n");
        return None;
    }
    let flags = crate::cpu::irq_save();
    let Some((entry, root)) = kernel_core::elf::load(elf_image) else {
        crate::cpu::irq_restore(flags);
        return None;
    };
    let Some(ustack_phys) = frame::get().alloc_contiguous(USER_STACK_PAGES as usize) else {
        free_root(root);
        crate::cpu::irq_restore(flags);
        return None;
    };
    for i in 0..USER_STACK_PAGES as u64 {
        map(root, USER_STACK_BASE as u64 + i * 4096, ustack_phys + i * 4096, Prot::Rw);
    }
    let id = register_user(root, entry as u32);
    if id.is_none() {
        free_root(root);
    }
    crate::cpu::irq_restore(flags);
    id
}

/// Spawn the `ud2` stub in its own address space: the permanent ring-3 fault
/// regression (the fault must kill this task, never the kernel).
pub fn spawn_fault_stub() -> Option<u64> {
    if !task::has_free_slot() {
        return None;
    }
    let flags = crate::cpu::irq_save();
    let Some(code_phys) = frame::get().alloc() else {
        crate::cpu::irq_restore(flags);
        return None;
    };
    let root = new_root();
    map(root, USER_BASE as u64, code_phys, Prot::Rx);
    unsafe {
        ptr::copy_nonoverlapping(
            FAULT_STUB.as_ptr(),
            crate::phys_to_virt(code_phys) as *mut u8,
            FAULT_STUB.len(),
        );
    }
    let id = register_user(root, USER_BASE);
    if id.is_none() {
        free_root(root);
    }
    crate::cpu::irq_restore(flags);
    id
}

/// int 0x80 entry, called from `isr_common` with the pushad frame pointer.
/// Frame layout (low to high): edi, esi, ebp, esp, ebx, edx, ecx, eax.
#[no_mangle]
pub extern "C" fn syscall_handle(frame: *mut u32) {
    unsafe {
        let nr = frame.add(7).read() as u64;
        let args = [
            frame.add(4).read() as u64, // ebx
            frame.add(6).read() as u64, // ecx
            frame.add(5).read() as u64, // edx
            frame.add(1).read() as u64, // esi
            frame.add(0).read() as u64, // edi
        ];
        let r = kernel_core::syscall::dispatch(write_user, nr, &args);
        frame.add(7).write(r as u32); // result in the saved eax
    }
}

/// SYS_WRITE bridge: bounded copy from user memory to the serial log.
fn write_user(ptr: u64, len: u64) -> u64 {
    if len > 4096 || ptr < 0x1000 || ptr + len > USER_LIMIT as u64 {
        return fantuan_abi::SYS_ERR_INVAL;
    }
    let s = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    serial::log_bytes(s);
    len
}
