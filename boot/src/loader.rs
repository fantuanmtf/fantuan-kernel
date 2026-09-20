//! Kernel image loading: SFS open/read + load-region validation.

use core::ffi::c_void;

use crate::console;
use crate::memory::MemMapBuf;
use crate::uefi::guid::{FILE_INFO_GUID, LOADED_IMAGE_GUID, SFS_GUID};
use crate::uefi::protocol::{File, LoadedImage, MemoryDescriptor, SimpleFileSystem, SimpleTextOutput};
use crate::uefi::table::BootServices;
use crate::uefi::{Handle, EFI_FILE_MODE_READ, EFI_LOAD_ERROR, EFI_SUCCESS, Status};

/// The kernel is a flat binary linked at 16 MiB (see kernel/link.ld).
/// Low memory is a firmware minefield — the EFI app image itself can sit right
/// above 1 MiB (OVMF did exactly that, inside a 1 MiB kernel's .bss footprint).
pub const KERNEL_ADDR: u64 = 0x1000000;
const KERNEL_MAX_SIZE: u64 = 0x400000; // 4 MiB sanity cap on the file
/// The whole span we must verify usable: file + alignment + .bss headroom.
const KERNEL_FOOTPRINT: u64 = 0x200000; // 2 MiB

/// Memory types that are reclaimable after ExitBootServices.
fn usable(t: u32) -> bool {
    // 1/2/3/4/7 = LoaderCode/Data, BootServices Code/Data, Conventional.
    // 15 = EfiUnacceptedMemoryType: OVMF marks all RAM unaccepted (TDX/SEV
    // confidential-computing protocol). A non-confidential VM can use it
    // directly; the full accept-memory protocol is out of scope for M0.
    matches!(t, 1 | 2 | 3 | 4 | 7 | 15)
}

/// Locate \fantuan\kernel.bin on the ESP, verify the load region and read the
/// image to KERNEL_ADDR. Returns the file size in bytes.
pub fn load_kernel(
    bs: &BootServices,
    con: *mut SimpleTextOutput,
    map: &MemMapBuf,
    image_handle: Handle,
) -> Result<u64, Status> {
    // Prefer the volume this image was loaded from: locate_protocol(SFS)
    // returns the FIRST SimpleFileSystem handle, which can belong to another
    // FAT volume (e.g. the test disk) whose root has no \fantuan\kernel.bin.
    // The loaded-image protocol names the exact device handle; fall back to
    // the first SFS only when it is unavailable.
    let mut sfs: *mut c_void = core::ptr::null_mut();
    let mut sts = EFI_LOAD_ERROR;
    if !image_handle.is_null() {
        let mut li: *mut c_void = core::ptr::null_mut();
        if (bs.handle_protocol)(image_handle, &LOADED_IMAGE_GUID, &mut li) == EFI_SUCCESS
            && !li.is_null()
        {
            let dev = unsafe { (*(li as *const LoadedImage)).device_handle };
            sts = (bs.handle_protocol)(dev, &SFS_GUID, &mut sfs);
        }
    }
    if sts != EFI_SUCCESS || sfs.is_null() {
        sts = (bs.locate_protocol)(&SFS_GUID, core::ptr::null_mut(), &mut sfs);
    }
    if sts != EFI_SUCCESS {
        console::println(con, "ERROR: Simple File System not found");
        return Err(sts);
    }
    let sfs = sfs as *mut SimpleFileSystem;
    let mut root: *mut File = core::ptr::null_mut();
    let sts = unsafe { ((*sfs).open_volume)(sfs, &mut root) };
    if sts != EFI_SUCCESS {
        console::println(con, "ERROR: cannot open ESP volume");
        return Err(sts);
    }
    let mut path = [0u16; 64];
    for (i, b) in "\\fantuan\\kernel.bin".bytes().enumerate() {
        path[i] = b as u16;
    }
    let mut file: *mut File = core::ptr::null_mut();
    let sts = unsafe { ((*root).open)(root, &mut file, path.as_mut_ptr(), EFI_FILE_MODE_READ, 0) };
    if sts != EFI_SUCCESS {
        console::println(con, "ERROR: cannot open \\fantuan\\kernel.bin on ESP");
        return Err(sts);
    }
    // EFI_FILE_INFO: Size(8) FileSize(8) PhysicalSize(8) ...
    let mut info = [0u8; 256];
    let mut info_size = info.len();
    let sts = unsafe {
        ((*file).get_info)(
            file,
            &mut (FILE_INFO_GUID as crate::uefi::guid::Guid),
            &mut info_size,
            info.as_mut_ptr() as *mut c_void,
        )
    };
    let file_size =
        u64::from_le_bytes([info[8], info[9], info[10], info[11], info[12], info[13], info[14], info[15]]);
    if sts != EFI_SUCCESS || file_size == 0 || file_size > KERNEL_MAX_SIZE {
        console::println(con, "ERROR: bad kernel size");
        return Err(EFI_LOAD_ERROR);
    }
    let mut buf = [0u16; 256];
    let mut off = 0;
    console::write_ascii(&mut buf, &mut off, "kernel: ");
    console::write_dec(&mut buf, &mut off, file_size);
    console::write_ascii(&mut buf, &mut off, " bytes");
    console::output_line(con, &mut buf, off);

    // Verify the whole footprint (file + alignment + .bss) is usable.
    let mut region_ok = false;
    let mut covering_type: u32 = 0;
    for i in 0..map.count {
        let d = unsafe { &*(map.ptr as *const MemoryDescriptor).add(i) };
        let start = d.physical_start;
        let end = start + d.number_of_pages * 4096;
        if start <= KERNEL_ADDR && KERNEL_ADDR + KERNEL_FOOTPRINT <= end {
            covering_type = d.type_;
            region_ok = usable(d.type_);
            break;
        }
    }
    if !region_ok {
        let mut buf = [0u16; 256];
        let mut off = 0;
        console::write_ascii(&mut buf, &mut off, "ERROR: kernel region unusable (type=");
        console::write_dec(&mut buf, &mut off, covering_type as u64);
        console::write_ascii(&mut buf, &mut off, ")");
        console::output_line(con, &mut buf, off);
        return Err(EFI_LOAD_ERROR);
    }
    let mut read = file_size as usize;
    let sts = unsafe { ((*file).read)(file, &mut read, KERNEL_ADDR as *mut c_void) };
    if sts != EFI_SUCCESS || read as u64 != file_size {
        console::println(con, "ERROR: kernel read failed");
        return Err(EFI_LOAD_ERROR);
    }
    console::println(con, "kernel: loaded at 0x1000000");
    Ok(file_size)
}
