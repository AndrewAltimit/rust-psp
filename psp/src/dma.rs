//! DMA (Direct Memory Access) transfer abstractions.
//!
//! The PSP's DMA controller can perform memory-to-memory transfers
//! independently of the CPU, freeing it for other work. This module
//! provides a safe, ergonomic API over the raw DMA syscalls.
//!
//! # Features
//!
//! - [`DmaResult`] handle for cache management after transfer
//! - [`memcpy_dma`] for bulk memory copies
//! - [`vram_blit_dma`] for efficient VRAM writes
//! - Automatic cache management on completion
//!
//! # Note
//!
//! The PSP's `sceDmacMemcpy` syscall is **synchronous** — it blocks
//! until the DMA transfer completes. The API returns a [`DmaResult`]
//! handle for post-transfer cache invalidation.

use core::ffi::c_void;

/// Error type for DMA operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaError {
    /// The PSP kernel returned an error code.
    KernelError(i32),
    /// Invalid parameter (null pointer, zero size, etc.).
    InvalidParam,
}

/// Result of a completed DMA transfer.
///
/// Since `sceDmacMemcpy` is synchronous, the transfer is already
/// complete when this handle is returned. Use [`invalidate_cache`](Self::invalidate_cache)
/// to invalidate the destination cache region so CPU reads see the
/// DMA'd data through cached access.
pub struct DmaResult {
    dst: *mut u8,
    size: u32,
}

impl DmaResult {
    /// Invalidate the destination cache region so CPU reads see the
    /// DMA'd data.
    ///
    /// Returns a raw pointer to the destination for convenience.
    pub fn invalidate_cache(&self) -> *mut u8 {
        unsafe {
            crate::sys::sceKernelDcacheInvalidateRange(self.dst as *const c_void, self.size);
        }
        self.dst
    }

    /// Get the destination pointer.
    pub fn dst(&self) -> *mut u8 {
        self.dst
    }

    /// Get the transfer size in bytes.
    pub fn size(&self) -> u32 {
        self.size
    }
}

/// Perform a DMA memory copy.
///
/// Copies `len` bytes from `src` to `dst` using the PSP's DMA controller.
/// The call blocks until the transfer completes.
///
/// The source region's cache is written back before the transfer begins.
/// Call [`DmaResult::invalidate_cache`] on the result to read the
/// destination through cached access.
///
/// # Safety
///
/// - `dst` must be valid for `len` bytes of writes.
/// - `src` must be valid for `len` bytes of reads.
/// - The source and destination regions must not overlap.
/// - `len` must be > 0.
pub unsafe fn memcpy_dma(dst: *mut u8, src: *const u8, len: u32) -> Result<DmaResult, DmaError> {
    if dst.is_null() || src.is_null() || len == 0 {
        return Err(DmaError::InvalidParam);
    }

    // Flush source region from cache so DMA reads correct data
    unsafe {
        crate::sys::sceKernelDcacheWritebackRange(src as *const c_void, len);
    }

    // Use the kernel DMA memcpy syscall (synchronous — blocks until done)
    let ret = unsafe { crate::sys::sceDmacMemcpy(dst as *mut c_void, src as *const c_void, len) };

    if ret < 0 {
        return Err(DmaError::KernelError(ret));
    }

    Ok(DmaResult { dst, size: len })
}

/// DMA blit data into VRAM.
///
/// Copies `src` data into VRAM at the given byte offset. The VRAM
/// base address is at `0x0400_0000` (uncached: `0x4400_0000`).
///
/// This is useful for uploading textures, framebuffer updates, or
/// any bulk VRAM write that benefits from DMA rather than CPU loops.
///
/// # Safety
///
/// - `src` must be valid for `src.len()` bytes of reads.
/// - `vram_offset + src.len()` must not exceed VRAM size (2 MiB).
pub unsafe fn vram_blit_dma(vram_offset: usize, src: &[u8]) -> Result<DmaResult, DmaError> {
    const VRAM_BASE: u32 = 0x0400_0000;
    const VRAM_SIZE: usize = 2 * 1024 * 1024;

    if src.is_empty() {
        return Err(DmaError::InvalidParam);
    }

    if vram_offset + src.len() > VRAM_SIZE {
        return Err(DmaError::InvalidParam);
    }

    let dst = (VRAM_BASE as usize + vram_offset) as *mut u8;

    unsafe { memcpy_dma(dst, src.as_ptr(), src.len() as u32) }
}
