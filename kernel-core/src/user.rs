//! User-mapping abstraction for the shared ELF loader (M9.3a). Each kernel
//! installs the ops at boot; the loader only speaks abstract protections.

/// Page protection as the loader sees it (W^X from PT_LOAD p_flags).
#[derive(Clone, Copy, PartialEq)]
pub enum Prot {
    Ro,
    Rw,
    Rx,
    Rwx,
}

impl Prot {
    pub fn write(self) -> bool {
        matches!(self, Prot::Rw | Prot::Rwx)
    }
    pub fn exec(self) -> bool {
        matches!(self, Prot::Rx | Prot::Rwx)
    }
    /// Union of rights: writable if either writes, executable if either runs.
    pub fn union(self, other: Prot) -> Prot {
        match (self.write() || other.write(), self.exec() || other.exec()) {
            (false, false) => Prot::Ro,
            (true, false) => Prot::Rw,
            (false, true) => Prot::Rx,
            (true, true) => Prot::Rwx,
        }
    }
}

/// Arch operations for user address spaces (installed at boot).
#[derive(Clone, Copy)]
#[repr(C)]
pub struct UserOps {
    /// ELF e_machine this kernel accepts (0x3E x86_64, 0xF3 riscv).
    pub machine: u16,
    /// Fresh address-space root; kernel half cloned where applicable.
    pub new_root: fn() -> u64,
    /// Map one 4K page with PROT.
    pub map: fn(root: u64, va: u64, pa: u64, prot: Prot),
    /// Tear an address space down.
    pub free_root: fn(root: u64),
    /// Physical -> kernel-accessible virtual.
    pub phys_to_virt: fn(u64) -> u64,
    /// Kernel log channel for loader errors.
    pub log: fn(&str),
}

fn unset_root() -> u64 {
    0
}
fn unset_map(_root: u64, _va: u64, _pa: u64, _prot: Prot) {}
fn unset_free(_root: u64) {}
fn unset_p2v(p: u64) -> u64 {
    p
}
fn unset_log(_s: &str) {}

/// Arch user-memory copy bridge (P1). Kernels that run fd/path syscalls
/// install real copies (with SMAP bracketing on x86_64); the default refuses,
/// so file syscalls report ERR_NOSYS on kernels without user mode.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct UserMemOps {
    /// Copy `dst.len()` bytes from user address `src`; false on refusal.
    pub copy_in: fn(dst: &mut [u8], src: u64) -> bool,
    /// Copy `src` to user address `dst`; false on refusal.
    pub copy_out: fn(dst: u64, src: &[u8]) -> bool,
}

fn unset_in(_dst: &mut [u8], _src: u64) -> bool {
    false
}
fn unset_out(_dst: u64, _src: &[u8]) -> bool {
    false
}

static mut MEM_OPS: UserMemOps = UserMemOps { copy_in: unset_in, copy_out: unset_out };

pub fn set_mem_ops(ops: UserMemOps) {
    unsafe { core::ptr::write(core::ptr::addr_of_mut!(MEM_OPS), ops) };
}

pub fn mem_ops() -> UserMemOps {
    unsafe { core::ptr::addr_of!(MEM_OPS).read() }
}

/// Copy into kernel memory; None when no bridge is installed or it refuses.
pub fn copy_in(dst: &mut [u8], src: u64) -> Option<()> {
    if (mem_ops().copy_in)(dst, src) {
        Some(())
    } else {
        None
    }
}

/// Copy out of kernel memory; None when no bridge is installed or it refuses.
pub fn copy_out(dst: u64, src: &[u8]) -> Option<()> {
    if (mem_ops().copy_out)(dst, src) {
        Some(())
    } else {
        None
    }
}

static mut OPS: UserOps = UserOps {
    machine: 0,
    new_root: unset_root,
    map: unset_map,
    free_root: unset_free,
    phys_to_virt: unset_p2v,
    log: unset_log,
};

pub fn set_ops(ops: UserOps) {
    unsafe { core::ptr::write(core::ptr::addr_of_mut!(OPS), ops) };
}

pub fn ops() -> UserOps {
    unsafe { core::ptr::addr_of!(OPS).read() }
}
