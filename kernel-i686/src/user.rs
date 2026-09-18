//! i686 user mode (M10-4b3): maps the ring-3 stub into the shared page
//! directory, builds the ring-3 iret frame, spawns the task and handles
//! `int 0x80`. Ring 3 passes eax = number and ebx/ecx/edx/esi/edi as the
//! five arguments; the shared dispatch in `kernel_core::syscall` provides
//! the semantics (SYS_WRITE is (buf, len), see fantuan-abi).

use core::arch::asm;
use core::ptr;

use kernel_core::task::{self, State, Task};

use crate::gdt;
use crate::serial;

pub const USER_BASE: u32 = 0x0040_0000;
const USER_STACK_PAGE: u32 = 0x007F_F000;
const USER_STACK_TOP: u32 = 0x0080_0000;
const USER_LIMIT: u32 = 0x0080_0000;

static STUB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/user_stub.bin"));

extern "C" {
    fn user_entry();
}

fn cr3() -> u32 {
    let v: u32;
    unsafe { asm!("mov {}, cr3", out(reg) v, options(nomem, nostack)) };
    v
}

/// Map one 4K user page. Only user VAs are passed here: replacing the
/// stage2 identity PSE entry for that 4 MiB region is safe because the
/// kernel reaches all memory through the PHYS_OFFSET alias.
fn map_page(va: u32, pa: u32, writable: bool) {
    unsafe {
        let pd = crate::phys_to_virt(cr3() as u64) as *mut u32;
        let pdi = ((va >> 22) & 0x3FF) as isize;
        let pti = ((va >> 12) & 0x3FF) as isize;
        let mut entry = pd.offset(pdi).read_volatile();
        if entry & 1 == 0 || entry & (1 << 7) != 0 {
            let pt_phys = kernel_core::frame::get().alloc().expect("no frame for user PT") as u32;
            let pt = crate::phys_to_virt(pt_phys as u64) as *mut u32;
            for i in 0..1024 {
                pt.add(i).write_volatile(0);
            }
            entry = pt_phys | 0x7; // present, writable, user
            pd.offset(pdi).write_volatile(entry);
        }
        let flags = if writable { 0x7 } else { 0x5 }; // + writable?
        let pt = crate::phys_to_virt((entry & 0xFFFF_F000) as u64) as *mut u32;
        pt.offset(pti).write_volatile(pa | flags);
    }
}

/// Spawn the built-in ring-3 stub; returns its tid.
pub fn spawn_stub() -> u64 {
    let flags = crate::cpu::irq_save();

    let code_phys = kernel_core::frame::get().alloc().expect("no frame for user code");
    unsafe {
        ptr::copy_nonoverlapping(STUB.as_ptr(), crate::phys_to_virt(code_phys) as *mut u8, STUB.len());
    }
    map_page(USER_BASE, code_phys as u32, false);

    let stack_phys = kernel_core::frame::get().alloc().expect("no frame for user stack");
    map_page(USER_STACK_PAGE, stack_phys as u32, true);

    let (kstack_phys, kstack_top) = task::alloc_kernel_stack().expect("no kernel stack");

    // Kernel-stack frame: the four callee-saved registers context_switch
    // pops, the trampoline as its ret target, then the ring-3 iret frame.
    let sp = (kstack_top - 10 * 4) as *mut u32;
    unsafe {
        sp.add(0).write(0); // edi
        sp.add(1).write(0); // esi
        sp.add(2).write(0); // ebx
        sp.add(3).write(0); // ebp
        sp.add(4).write(user_entry as *const () as u32);
        sp.add(5).write(USER_BASE); // eip
        sp.add(6).write(gdt::UCODE_SEL as u32);
        sp.add(7).write(0x202); // eflags: IF set
        sp.add(8).write(USER_STACK_TOP);
        sp.add(9).write(gdt::UDATA_SEL as u32);
    }

    crate::cpu::irq_restore(flags);
    task::register(Task {
        state: State::Ready,
        rsp: sp as u64,
        vm_root: 0, // shared stage2 page directory
        kernel_stack_top: kstack_top,
        is_user: true,
        stack_phys: kstack_phys,
        body: task::dead_body,
        id: 0,
        exit_code: 0,
    })
    .expect("task table full")
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
