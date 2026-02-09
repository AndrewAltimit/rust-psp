use super::MIN_ALIGN;
use crate::alloc::{GlobalAlloc, Layout, System};
use crate::ptr;

unsafe extern "C" {
    fn __psp_alloc(size: u32, align: u32) -> *mut u8;
    fn __psp_dealloc(ptr: *mut u8);
    fn __psp_realloc(ptr: *mut u8, old_size: u32, new_size: u32, align: u32) -> *mut u8;
}

#[stable(feature = "alloc_system_type", since = "1.28.0")]
unsafe impl GlobalAlloc for System {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { __psp_alloc(layout.size() as u32, layout.align() as u32) }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { self.alloc(layout) };
        if !p.is_null() {
            unsafe { ptr::write_bytes(p, 0, layout.size()) };
        }
        p
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        unsafe { __psp_dealloc(ptr) }
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        unsafe {
            __psp_realloc(ptr, layout.size() as u32, new_size as u32, layout.align() as u32)
        }
    }
}
