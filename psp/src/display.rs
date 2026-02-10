//! Display and vblank synchronization for the PSP.
//!
//! Wraps the common `sceDisplay*` syscalls into ergonomic functions.
//! Every graphics application needs vblank sync — this module removes
//! the need to call raw syscalls directly.

use core::ffi::c_void;

use crate::sys::{DisplayPixelFormat, DisplaySetBufSync};

/// Information about the current framebuffer configuration.
pub struct FrameBufInfo {
    /// Pointer to the top-left pixel of the framebuffer.
    pub top_addr: *mut u8,
    /// Buffer width in pixels (power of 2, typically 512).
    pub buf_width: usize,
    /// Pixel format of the framebuffer.
    pub pixel_format: DisplayPixelFormat,
}

/// Wait for the current vblank period to end.
pub fn wait_vblank() {
    unsafe {
        crate::sys::sceDisplayWaitVblank();
    }
}

/// Wait for the next vblank period to start.
pub fn wait_vblank_start() {
    unsafe {
        crate::sys::sceDisplayWaitVblankStart();
    }
}

/// Get the number of vertical blank pulses since the system started.
pub fn vblank_count() -> u32 {
    unsafe { crate::sys::sceDisplayGetVcount() }
}

/// Check if the display is currently in the vertical blank period.
pub fn is_vblank() -> bool {
    unsafe { crate::sys::sceDisplayIsVblank() != 0 }
}

/// Set the framebuffer displayed on screen.
///
/// # Safety
///
/// `buf_ptr` must point to a valid framebuffer in VRAM with the
/// correct format and stride.
pub unsafe fn set_framebuf(
    buf_ptr: *const u8,
    buf_width: usize,
    fmt: DisplayPixelFormat,
    sync: DisplaySetBufSync,
) {
    unsafe {
        crate::sys::sceDisplaySetFrameBuf(buf_ptr, buf_width, fmt, sync);
    }
}

/// Get the current framebuffer configuration.
///
/// `sync` selects which buffer info to retrieve: the currently
/// displayed buffer ([`Immediate`](DisplaySetBufSync::Immediate)) or the
/// one queued for next frame ([`NextFrame`](DisplaySetBufSync::NextFrame)).
pub fn get_framebuf(sync: DisplaySetBufSync) -> FrameBufInfo {
    let mut top_addr: *mut c_void = core::ptr::null_mut();
    let mut buf_width: usize = 0;
    let mut pixel_format = DisplayPixelFormat::Psm8888;
    unsafe {
        crate::sys::sceDisplayGetFrameBuf(&mut top_addr, &mut buf_width, &mut pixel_format, sync);
    }
    FrameBufInfo {
        top_addr: top_addr as *mut u8,
        buf_width,
        pixel_format,
    }
}
