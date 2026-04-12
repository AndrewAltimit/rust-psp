//! `__psp_alloc` / `__psp_dealloc` / `__psp_realloc` C-ABI shims.
//!
//! `std::sys::alloc::psp::System` declares these as `extern "C"` and
//! routes calls to `std::alloc::System::alloc` through them. We
//! delegate to the global Rust allocator (`alloc::alloc::alloc`)
//! defined in `alloc_impl.rs` so std and `#[global_allocator]`
//! callers share one arena. See `alloc_impl.rs` for the
//! linked-list arena rationale.

use alloc::alloc::{alloc, dealloc};
use core::alloc::Layout;
use core::ptr;

/// Maximum supported alignment. Mirrors `alloc_impl::MAX_ALIGN`.
const MAX_ALIGN: usize = 128;

/// Allocate `size` bytes with `align` alignment via the global
/// Rust allocator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_alloc(size: u32, align: u32) -> *mut u8 {
    let user_align = align as usize;
    if user_align > MAX_ALIGN || !user_align.is_power_of_two() {
        return ptr::null_mut();
    }
    let layout = match Layout::from_size_align(size as usize, user_align) {
        Ok(l) => l,
        Err(_) => return ptr::null_mut(),
    };
    unsafe { alloc(layout) }
}

/// Free a block previously returned by `__psp_alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_dealloc(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    // Layout is unused by the global allocator's dealloc shim
    // (`alloc_impl.rs` recovers size from its own per-block header)
    // so passing a placeholder is safe.
    let layout = unsafe { Layout::from_size_align_unchecked(1, 1) };
    unsafe { dealloc(ptr, layout) };
}

/// Reallocate `old_ptr` to `new_size`.
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
    let copy_size = if old_size < new_size { old_size } else { new_size } as usize;
    // Manual byte copy to avoid memcpy recursion on MIPS.
    let mut i = 0;
    while i < copy_size {
        unsafe { *new_ptr.add(i) = *old_ptr.add(i) };
        i += 1;
    }
    unsafe { __psp_dealloc(old_ptr) };
    new_ptr
}
