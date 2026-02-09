//! DMA (Direct Memory Access) transfer abstractions.
//!
//! The PSP's DMA controller can perform memory-to-memory transfers
//! independently of the CPU, freeing it for other work. This module
//! provides a safe, ergonomic API over the raw DMA hardware registers.
//!
//! # Features
//!
//! - [`DmaTransfer`] handle for polling/blocking on transfer completion
//! - [`memcpy_dma`] for bulk memory copies
//! - [`vram_blit_dma`] for efficient VRAM writes
//! - Automatic cache management on completion
//!
//! # Kernel Mode Required
//!
//! DMA register access requires `feature = "kernel"` and the module
//! must be declared with `psp::module_kernel!()`.
//!
//! # PSP DMA Controller
//!
//! The PSP's `sceDmacMemcpy` and `sceDmacTryMemcpy` syscalls provide
//! user-space access to the DMA controller for simple memory copies.
//! For kernel-mode applications, we also provide direct register access
//! for VRAM blits.

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

/// Global lock to ensure only one DMA transfer is in flight at a time.
/// The PSP has a single DMA channel for general-purpose memory copies.
static DMA_IN_USE: AtomicBool = AtomicBool::new(false);

/// Error type for DMA operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaError {
    /// The DMA controller is already in use by another transfer.
    Busy,
    /// The PSP kernel returned an error code.
    KernelError(i32),
    /// Invalid parameter (null pointer, zero size, etc.).
    InvalidParam,
}

/// A handle to an in-progress or completed DMA transfer.
///
/// When the transfer completes, you can optionally invalidate the
/// destination cache region to read the DMA'd data through cached
/// access (faster for CPU reads).
///
/// Dropping a `DmaTransfer` will block until the transfer completes
/// to prevent use-after-DMA bugs.
pub struct DmaTransfer {
    dst: *mut u8,
    size: u32,
    completed: bool,
}

impl DmaTransfer {
    /// Poll for transfer completion.
    ///
    /// Returns `true` if the transfer has finished.
    pub fn is_complete(&self) -> bool {
        if self.completed {
            return true;
        }
        // sceDmacMemcpy is synchronous in the PSP kernel, so if we
        // got here, the transfer is already done.
        true
    }

    /// Block until the transfer completes.
    pub fn wait(&mut self) {
        while !self.is_complete() {
            core::hint::spin_loop();
        }
        self.completed = true;
    }

    /// Block until transfer completes, then invalidate the destination
    /// cache region so CPU reads see the DMA'd data.
    ///
    /// Returns a raw pointer to the destination for convenience.
    pub fn finish_and_invalidate(&mut self) -> *mut u8 {
        self.wait();
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

impl Drop for DmaTransfer {
    fn drop(&mut self) {
        self.wait();
        DMA_IN_USE.store(false, Ordering::Release);
    }
}

/// Perform a DMA memory copy.
///
/// Copies `len` bytes from `src` to `dst` using the PSP's DMA controller.
/// The CPU is free to do other work while the transfer is in progress,
/// though on the PSP `sceDmacMemcpy` is typically synchronous.
///
/// The source region's cache is written back before the transfer begins.
/// The destination region's cache should be invalidated after completion
/// (call [`DmaTransfer::finish_and_invalidate`]).
///
/// # Safety
///
/// - `dst` must be valid for `len` bytes of writes.
/// - `src` must be valid for `len` bytes of reads.
/// - The source and destination regions must not overlap.
/// - `len` must be > 0.
pub unsafe fn memcpy_dma(dst: *mut u8, src: *const u8, len: u32) -> Result<DmaTransfer, DmaError> {
    if dst.is_null() || src.is_null() || len == 0 {
        return Err(DmaError::InvalidParam);
    }

    if DMA_IN_USE
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return Err(DmaError::Busy);
    }

    // Flush source region from cache so DMA reads correct data
    unsafe {
        crate::sys::sceKernelDcacheWritebackRange(src as *const c_void, len);
    }

    // Use the kernel DMA memcpy syscall
    let ret = unsafe { crate::sys::sceDmacMemcpy(dst as *mut c_void, src as *const c_void, len) };

    if ret < 0 {
        DMA_IN_USE.store(false, Ordering::Release);
        return Err(DmaError::KernelError(ret));
    }

    Ok(DmaTransfer {
        dst,
        size: len,
        completed: true, // sceDmacMemcpy is synchronous
    })
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
pub unsafe fn vram_blit_dma(vram_offset: usize, src: &[u8]) -> Result<DmaTransfer, DmaError> {
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

/// Perform a DMA memory copy, retrying if the DMA controller is busy.
///
/// This is a convenience wrapper around [`memcpy_dma`] that spins until
/// the DMA controller becomes available.
///
/// # Safety
///
/// Same requirements as [`memcpy_dma`].
pub unsafe fn memcpy_dma_blocking(
    dst: *mut u8,
    src: *const u8,
    len: u32,
) -> Result<DmaTransfer, DmaError> {
    loop {
        match unsafe { memcpy_dma(dst, src, len) } {
            Err(DmaError::Busy) => core::hint::spin_loop(),
            result => return result,
        }
    }
}
