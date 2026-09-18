//! Round-robin task scheduler shared by both kernels (M9.2c). The static
//! table, states, sleeping and reaping are arch-neutral; everything that
//! touches a stack frame or an address space goes through TaskOps, installed
//! by each kernel at boot.

use core::ptr;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::arch;
use crate::frame;

pub const MAX_TASKS: usize = 16;
const STACK_PAGES: u64 = 4; // 16 KiB per task

#[derive(Clone, Copy, PartialEq)]
pub enum State {
    Unused,
    Ready,
    Sleeping { wake_tick: u64 },
    Exited,
}

pub struct Task {
    pub state: State,
    /// Saved callee-saved registers (arch layout).
    pub rsp: u64,
    /// Address-space root: x86 PML4 physical; 0 on riscv until M9.3.
    pub vm_root: u64,
    /// Kernel stack top (also the arch's kernel-entry stack).
    pub kernel_stack_top: u64,
    /// True for tasks that run in user mode (their vm_root gets freed).
    pub is_user: bool,
    /// Kernel stack frame base (physical); reaping frees STACK_PAGES here.
    pub stack_phys: u64,
    pub body: fn() -> !,
    pub id: u64,
    pub exit_code: u64,
}

/// Placeholder body for tasks that never run it (user tasks).
pub fn dead_body() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

const EMPTY_TASK: Task = Task {
    state: State::Unused,
    rsp: 0,
    vm_root: 0,
    kernel_stack_top: 0,
    is_user: false,
    stack_phys: 0,
    body: dead_body,
    id: 0,
    exit_code: 0,
};

static mut TASKS: [Task; MAX_TASKS] = [EMPTY_TASK; MAX_TASKS];
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static NEXT_ID: AtomicU64 = AtomicU64::new(0);
pub static SWITCHES: AtomicU64 = AtomicU64::new(0);

/// Arch operations the scheduler needs (all installed at boot).
#[derive(Clone, Copy)]
pub struct TaskOps {
    /// Save the current context on OLD_RSP and resume NEW_RSP in NEW_VM_ROOT.
    pub switch: fn(old_rsp: *mut u64, new_rsp: u64, new_vm_root: u64),
    /// Point the kernel-entry stack at TOP (x86 TSS.rsp0, riscv sscratch).
    /// IS_USER tells riscv whether to arm the sscratch trap swap.
    pub set_kernel_stack: fn(top: u64, is_user: bool),
    /// Build the initial frame for a kernel task; returns its saved rsp.
    pub init_kernel_stack: fn(stack_top: u64, body: fn() -> !) -> u64,
    /// Address-space root of kernel tasks.
    pub kernel_vm_root: fn() -> u64,
    /// Tear down a user address space.
    pub free_user_vm: fn(vm_root: u64),
    pub phys_to_virt: fn(u64) -> u64,
    pub now_ticks: fn() -> u64,
    /// Log one reap (the kernel owns serial).
    pub on_reap: fn(tid: u64),
}

fn unset(_old: *mut u64, _new: u64, _vm: u64) {}
fn unset_top(_top: u64, _is_user: bool) {}
fn unset_stack(_top: u64, _body: fn() -> !) -> u64 {
    0
}
fn unset_root() -> u64 {
    0
}
fn unset_vm(_vm: u64) {}
fn unset_p2v(p: u64) -> u64 {
    p
}
fn unset_ticks() -> u64 {
    0
}
fn unset_reap(_tid: u64) {}

static mut OPS: TaskOps = TaskOps {
    switch: unset,
    set_kernel_stack: unset_top,
    init_kernel_stack: unset_stack,
    kernel_vm_root: unset_root,
    free_user_vm: unset_vm,
    phys_to_virt: unset_p2v,
    now_ticks: unset_ticks,
    on_reap: unset_reap,
};

pub fn set_ops(ops: TaskOps) {
    unsafe { ptr::write(ptr::addr_of_mut!(OPS), ops) };
}

fn ops() -> TaskOps {
    unsafe { ptr::addr_of!(OPS).read() }
}

/// Whether a task slot is available (callers allocate before registering).
pub fn has_free_slot() -> bool {
    find_slot().is_some()
}

fn find_slot() -> Option<usize> {
    (0..MAX_TASKS).find(|&i| unsafe { (*ptr::addr_of!(TASKS[i])).state == State::Unused })
}

/// Make the boot context task 0 with BOOT_STACK_TOP as its kernel stack.
pub fn init(boot_stack_top: u64) {
    let ops = ops();
    unsafe {
        TASKS[0] = Task {
            state: State::Ready,
            rsp: 0,
            vm_root: (ops.kernel_vm_root)(),
            kernel_stack_top: boot_stack_top,
            is_user: false,
            stack_phys: 0,
            body: dead_body,
            id: 0,
            exit_code: 0,
        };
    }
    (ops.set_kernel_stack)(boot_stack_top, false);
}

/// Allocate a kernel stack; returns (physical base, virtual top).
pub fn alloc_kernel_stack() -> Option<(u64, u64)> {
    let phys = frame::get().alloc_contiguous(STACK_PAGES as usize)?;
    let top = (ops().phys_to_virt)(phys + STACK_PAGES * frame::FRAME_SIZE);
    Some((phys, top))
}

/// Reserve a slot, assign the id and publish the task. Interrupt-safe.
pub fn register(mut t: Task) -> Option<u64> {
    let flags = arch::irq_save();
    let Some(slot) = find_slot() else {
        arch::irq_restore(flags);
        return None;
    };
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;
    t.id = id;
    unsafe { ptr::write(ptr::addr_of_mut!(TASKS[slot]), t) };
    arch::irq_restore(flags);
    Some(id)
}

/// Spawn a kernel task with a fresh stack and the arch's initial frame.
pub fn spawn(body: fn() -> !) -> u64 {
    let ops = ops();
    let (stack_phys, stack_top) = alloc_kernel_stack().expect("no contiguous frames for task stack");
    let rsp = (ops.init_kernel_stack)(stack_top, body);
    register(Task {
        state: State::Ready,
        rsp,
        vm_root: (ops.kernel_vm_root)(),
        kernel_stack_top: stack_top,
        is_user: false,
        stack_phys,
        body,
        id: 0,
        exit_code: 0,
    })
    .expect("task table full")
}

/// Body of the task that is currently entering for the first time.
pub fn current_body() -> fn() -> ! {
    unsafe { (*ptr::addr_of!(TASKS[CURRENT.load(Ordering::Relaxed)])).body }
}

/// Round-robin: wake due sleepers, pick the next Ready task, switch.
/// Called from the timer interrupt on both architectures.
pub fn schedule() {
    let cur = CURRENT.load(Ordering::Relaxed);
    reap_exited(cur);
    let ops = ops();
    let now = (ops.now_ticks)();
    unsafe {
        for i in 0..MAX_TASKS {
            let t = &mut *ptr::addr_of_mut!(TASKS[i]);
            if let State::Sleeping { wake_tick } = t.state {
                if wake_tick <= now {
                    t.state = State::Ready;
                }
            }
        }
        let mut next = None;
        for off in 1..=MAX_TASKS {
            let i = (cur + off) % MAX_TASKS;
            if (*ptr::addr_of!(TASKS[i])).state == State::Ready {
                next = Some(i);
                break;
            }
        }
        let Some(n) = next else { return };
        if n == cur {
            return;
        }
        SWITCHES.fetch_add(1, Ordering::Relaxed);
        CURRENT.store(n, Ordering::Relaxed);
        // Point the kernel-entry stack at the NEXT task before switching: a
        // fresh user task enters user mode immediately and needs it already.
        let n_user = TASKS[n].is_user;
        (ops.set_kernel_stack)(TASKS[n].kernel_stack_top, n_user);
        let old_rsp = &raw mut TASKS[cur].rsp;
        let new_rsp = TASKS[n].rsp;
        let new_vm = TASKS[n].vm_root;
        (ops.switch)(old_rsp, new_rsp, new_vm);
    }
}

pub fn sleep_ms(ms: u64) {
    let now = (ops().now_ticks)();
    let wake = now + (ms * 100 / 1000).max(1);
    unsafe {
        (*ptr::addr_of_mut!(TASKS[CURRENT.load(Ordering::Relaxed)])).state = State::Sleeping { wake_tick: wake };
    }
    schedule();
}

/// Mark the current task exited and never run it again.
pub fn exit(code: u64) -> ! {
    unsafe {
        let t = &mut *ptr::addr_of_mut!(TASKS[CURRENT.load(Ordering::Relaxed)]);
        t.state = State::Exited;
        t.exit_code = code;
    }
    loop {
        schedule();
    }
}

pub fn current_id() -> u64 {
    unsafe { (*ptr::addr_of!(TASKS[CURRENT.load(Ordering::Relaxed)])).id }
}

/// Address-space root of the running task (used by riscv to re-enter the
/// user root on the way back to U-mode).
pub fn current_vm_root() -> u64 {
    unsafe { (*ptr::addr_of!(TASKS[CURRENT.load(Ordering::Relaxed)])).vm_root }
}

/// Free every Exited task except CURRENT's slot.
fn reap_exited(current: usize) {
    let ops = ops();
    for i in 0..MAX_TASKS {
        if i == current {
            continue;
        }
        unsafe {
            let t = &mut *ptr::addr_of_mut!(TASKS[i]);
            if t.state != State::Exited {
                continue;
            }
            let tid = t.id;
            for p in 0..STACK_PAGES {
                frame::get().free(t.stack_phys + p * frame::FRAME_SIZE);
            }
            if t.is_user && t.vm_root != 0 && t.vm_root != (ops.kernel_vm_root)() {
                (ops.free_user_vm)(t.vm_root);
            }
            t.state = State::Unused;
            (ops.on_reap)(tid);
        }
    }
}
