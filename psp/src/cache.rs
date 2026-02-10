//! Type-safe cache control for PSP memory.
//!
//! The PSP uses a MIPS R4000 CPU with separate instruction and data caches.
//! When sharing memory between the main CPU and the Media Engine (ME), or
//! when using DMA, the data cache must be explicitly managed.
//!
//! This module provides pointer wrapper types that enforce cache coherency
//! at the type level — passing a cached pointer where an uncached one is
//! required becomes a compile error instead of runtime corruption.
//!
//! # Address Model
//!
//! On the PSP, physical addresses can be accessed through two virtual
//! address windows:
//!
//! - **Cached** (`0x0000_0000..0x3FFF_FFFF`): Normal access through the
//!   CPU data cache. Fast for repeated access, but the ME and DMA
//!   controller cannot see cached data.
//!
//! - **Uncached** (`0x4000_0000..0x7FFF_FFFF`): Bypasses the CPU data
//!   cache. Every read/write goes directly to RAM. Required for all
//!   ME-shared memory and DMA source/destination addresses.
//!
//! The conversion between cached and uncached is done by setting or
//! clearing bit 30 of the address.

use core::ffi::c_void;
use core::marker::PhantomData;

/// Bitmask to convert a cached address to uncached (set bit 30).
pub const UNCACHED_MASK: u32 = 0x4000_0000;

// ── CachedPtr ───────────────────────────────────────────────────────

/// A pointer to data in the CPU's cached address space.
///
/// This is a zero-cost wrapper that tags a raw pointer as "cached."
/// To share this data with the ME or DMA, you must explicitly convert
/// it via [`flush_to_uncached`](CachedPtr::flush_to_uncached), which
/// writes back the data cache and returns an [`UncachedPtr`].
#[derive(Copy, Clone)]
pub struct CachedPtr<T> {
    ptr: *mut T,
    _marker: PhantomData<T>,
}

impl<T> CachedPtr<T> {
    /// Wrap a raw pointer as a `CachedPtr`.
    ///
    /// # Safety
    ///
    /// `ptr` must be in the cached address range (`< 0x4000_0000` or the
    /// cached KSEG0 equivalent). The pointer must be valid for the
    /// intended access pattern.
    pub unsafe fn new(ptr: *mut T) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Get the raw cached pointer.
    pub fn as_ptr(&self) -> *mut T {
        self.ptr
    }

    /// Flush the data cache for this region and return an uncached pointer.
    ///
    /// This writes back all dirty cache lines covering `[ptr, ptr+size)`,
    /// then invalidates them so subsequent cached reads will fetch fresh
    /// data from RAM. Returns an [`UncachedPtr`] to the same physical
    /// memory, accessed through the uncached window.
    ///
    /// # Safety
    ///
    /// - The memory region `[ptr, ptr+size)` must be valid.
    /// - `size` must cover the full extent of data to be shared.
    /// - Caller must ensure the uncached pointer is not used concurrently
    ///   with cached writes to the same region.
    pub unsafe fn flush_to_uncached(&self, size: u32) -> UncachedPtr<T> {
        unsafe {
            crate::sys::sceKernelDcacheWritebackInvalidateRange(self.ptr as *const c_void, size);
        }
        UncachedPtr {
            ptr: (self.ptr as u32 | UNCACHED_MASK) as *mut T,
            _marker: PhantomData,
        }
    }

    /// Flush the entire data cache and return an uncached pointer.
    ///
    /// Prefer [`flush_to_uncached`](CachedPtr::flush_to_uncached) with a
    /// size for better performance. This is a convenience for when the
    /// exact size is unknown.
    ///
    /// # Safety
    ///
    /// Same as `flush_to_uncached`.
    pub unsafe fn flush_all_to_uncached(&self) -> UncachedPtr<T> {
        unsafe {
            crate::sys::sceKernelDcacheWritebackInvalidateAll();
        }
        UncachedPtr {
            ptr: (self.ptr as u32 | UNCACHED_MASK) as *mut T,
            _marker: PhantomData,
        }
    }
}

impl<T> core::fmt::Debug for CachedPtr<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CachedPtr({:p})", self.ptr)
    }
}

// ── UncachedPtr ─────────────────────────────────────────────────────

/// A pointer to data in the uncached address space.
///
/// All ME-shared memory and DMA addresses must use uncached pointers.
/// To read DMA'd data through the cache (for performance), convert back
/// via [`invalidate_to_cached`](UncachedPtr::invalidate_to_cached).
#[derive(Copy, Clone)]
pub struct UncachedPtr<T> {
    ptr: *mut T,
    _marker: PhantomData<T>,
}

impl<T> UncachedPtr<T> {
    /// Wrap a raw pointer as an `UncachedPtr`.
    ///
    /// # Safety
    ///
    /// `ptr` must be in the uncached address range (bit 30 set, i.e.
    /// `>= 0x4000_0000`). The pointer must be valid for the intended
    /// access pattern.
    pub unsafe fn new(ptr: *mut T) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Create an `UncachedPtr` from a cached address by setting the
    /// uncached bit. Does **not** flush the cache — use this only when
    /// you know the cache is already clean or you're writing new data.
    ///
    /// # Safety
    ///
    /// The caller must ensure cache coherency is maintained.
    pub unsafe fn from_cached_addr(cached_ptr: *mut T) -> Self {
        Self {
            ptr: (cached_ptr as u32 | UNCACHED_MASK) as *mut T,
            _marker: PhantomData,
        }
    }

    /// Get the raw uncached pointer.
    pub fn as_ptr(&self) -> *mut T {
        self.ptr
    }

    /// Read the value using volatile access (appropriate for uncached memory).
    ///
    /// # Safety
    ///
    /// The pointer must be valid and properly aligned. No concurrent
    /// writes may be in progress.
    pub unsafe fn read_volatile(&self) -> T {
        unsafe { core::ptr::read_volatile(self.ptr) }
    }

    /// Write a value using volatile access (appropriate for uncached memory).
    ///
    /// # Safety
    ///
    /// The pointer must be valid and properly aligned. No concurrent
    /// reads/writes may be in progress.
    pub unsafe fn write_volatile(&self, val: T) {
        unsafe { core::ptr::write_volatile(self.ptr, val) }
    }

    /// Invalidate the data cache for this region and return a cached pointer.
    ///
    /// After DMA or ME has written to this uncached region, call this to
    /// invalidate stale cache lines so that subsequent cached reads will
    /// fetch the fresh data from RAM.
    ///
    /// # Safety
    ///
    /// - The memory region `[ptr, ptr+size)` must be valid.
    /// - The DMA/ME write must have completed before calling this.
    /// - `size` must cover the full extent of data that was modified.
    pub unsafe fn invalidate_to_cached(&self, size: u32) -> CachedPtr<T> {
        let cached_addr = (self.ptr as u32 & !UNCACHED_MASK) as *mut T;
        unsafe {
            crate::sys::sceKernelDcacheInvalidateRange(cached_addr as *const c_void, size);
        }
        CachedPtr {
            ptr: cached_addr,
            _marker: PhantomData,
        }
    }

    /// Get the corresponding cached address without invalidating.
    ///
    /// # Safety
    ///
    /// The caller must manually ensure cache coherency.
    pub unsafe fn to_cached_addr(&self) -> *mut T {
        (self.ptr as u32 & !UNCACHED_MASK) as *mut T
    }
}

impl<T> core::fmt::Debug for UncachedPtr<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "UncachedPtr({:p})", self.ptr)
    }
}

// ── Convenience Functions ───────────────────────────────────────────

/// Write back the entire data cache to memory.
///
/// After this call, all cached writes are visible in RAM and can be
/// seen by the ME or DMA controller.
pub fn dcache_writeback_all() {
    unsafe {
        crate::sys::sceKernelDcacheWritebackAll();
    }
}

/// Write back and invalidate the entire data cache.
///
/// Writes back all dirty lines and then invalidates all cache entries.
/// This is the safest (but slowest) way to ensure cache coherency.
pub fn dcache_writeback_invalidate_all() {
    unsafe {
        crate::sys::sceKernelDcacheWritebackInvalidateAll();
    }
}

/// Write back a range of the data cache to memory.
///
/// # Safety
///
/// `ptr` and `size` must describe a valid memory region.
pub unsafe fn dcache_writeback_range(ptr: *const c_void, size: u32) {
    unsafe {
        crate::sys::sceKernelDcacheWritebackRange(ptr, size);
    }
}

/// Write back and invalidate a range of the data cache.
///
/// # Safety
///
/// `ptr` and `size` must describe a valid memory region.
pub unsafe fn dcache_writeback_invalidate_range(ptr: *const c_void, size: u32) {
    unsafe {
        crate::sys::sceKernelDcacheWritebackInvalidateRange(ptr, size);
    }
}

/// Invalidate a range of the data cache (discard cached data).
///
/// Use this after DMA or ME has written to a memory region to ensure
/// subsequent cached reads see the fresh data.
///
/// # Safety
///
/// - `ptr` and `size` must describe a valid memory region.
/// - Any dirty cache lines in this range will be **discarded**, not
///   written back. Ensure no pending cached writes exist in this range.
pub unsafe fn dcache_invalidate_range(ptr: *const c_void, size: u32) {
    unsafe {
        crate::sys::sceKernelDcacheInvalidateRange(ptr, size);
    }
}

/// Invalidate the entire instruction cache.
///
/// Required after writing code to memory (e.g., for ME task code
/// placed in ME-accessible memory).
pub fn icache_invalidate_all() {
    unsafe {
        crate::sys::sceKernelIcacheInvalidateAll();
    }
}
