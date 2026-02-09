use crate::sys::{self, SceSysMemBlockTypes, SceSysMemPartitionId, SceUid};
use core::{mem, ptr};

/// Maximum supported alignment. Alignments larger than this will fail allocation.
/// This limit exists because we store the padding offset in a single byte.
const MAX_ALIGN: usize = 255;

/// Allocate memory with alignment support.
///
/// Uses PSP kernel partition memory. Same algorithm as the global allocator in
/// `alloc_impl.rs`, but exposed via FFI for std's System allocator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_alloc(size: u32, align: u32) -> *mut u8 {
    let align = align as usize;

    // Alignment padding is stored as u8, so we cannot support align > 255.
    if align > MAX_ALIGN {
        return ptr::null_mut();
    }

    let total = size as usize + mem::size_of::<SceUid>() + align;

    let id = unsafe {
        sys::sceKernelAllocPartitionMemory(
            SceSysMemPartitionId::SceKernelPrimaryUserPartition,
            &b"std_block\0"[0],
            SceSysMemBlockTypes::Low,
            total as u32,
            ptr::null_mut(),
        )
    };

    if id.0 < 0 {
        return ptr::null_mut();
    }

    unsafe {
        let mut p: *mut u8 = sys::sceKernelGetBlockHeadAddr(id).cast();
        // Store the block ID at the start.
        *p.cast() = id;
        p = p.add(mem::size_of::<SceUid>());
        // Align and store padding count.
        let align_padding = 1 + p.add(1).align_offset(align);
        *p.add(align_padding - 1) = align_padding as u8;
        p.add(align_padding)
    }
}

/// Free memory allocated by `__psp_alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_dealloc(ptr: *mut u8) {
    unsafe {
        let align_padding = *ptr.sub(1);
        let id = *ptr.sub(align_padding as usize).cast::<SceUid>().offset(-1);
        sys::sceKernelFreePartitionMemory(id);
    }
}

/// Reallocate memory. Allocates a new block, copies data, frees old.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_realloc(
    old_ptr: *mut u8,
    old_size: u32,
    new_size: u32,
    align: u32,
) -> *mut u8 {
    if old_ptr.is_null() {
        return unsafe { __psp_alloc(new_size, align) };
    }
    let new_ptr = unsafe { __psp_alloc(new_size, align) };
    if new_ptr.is_null() {
        return ptr::null_mut();
    }
    let copy_size = if old_size < new_size {
        old_size
    } else {
        new_size
    } as usize;
    // Manual byte copy to avoid memcpy recursion on MIPS.
    let mut i = 0;
    while i < copy_size {
        unsafe { *new_ptr.add(i) = *old_ptr.add(i) };
        i += 1;
    }
    unsafe { __psp_dealloc(old_ptr) };
    new_ptr
}
