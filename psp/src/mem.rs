//! Typed memory partition allocators for the PSP.
//!
//! The PSP has multiple memory partitions with different access
//! characteristics. Passing a pointer from the wrong partition to
//! hardware (e.g., giving a main-RAM pointer to the ME) causes silent
//! corruption. This module provides typed allocators that make partition
//! misuse a compile-time error.
//!
//! # Partitions
//!
//! | Partition | ID | Access        | Use Case                          |
//! |-----------|----|---------------|-----------------------------------|
//! | User      | 2  | Main CPU only | General-purpose allocations       |
//! | ME Kernel | 3  | CPU + ME      | Shared state, ME task stacks      |
//! | ME User   | 7  | CPU + ME      | ME-accessible user memory         |
//!
//! # Kernel Mode Required
//!
//! Partitions 1, 3-5, 8-12 require kernel mode. Partition 2 is available
//! in user mode.

use crate::sys::{
    SceSysMemBlockTypes, SceSysMemPartitionId, SceUid, sceKernelAllocPartitionMemory,
    sceKernelFreePartitionMemory, sceKernelGetBlockHeadAddr,
};
use core::marker::PhantomData;

/// Marker trait for a memory partition.
///
/// Sealed — cannot be implemented outside this module.
pub trait Partition: sealed::Sealed {
    /// The PSP partition ID.
    const ID: SceSysMemPartitionId;
    /// Human-readable name for debug output.
    const NAME: &'static str;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::UserPartition {}
    impl Sealed for super::MePartition {}
}

/// Marker type for user-mode partition 2 (main RAM, CPU only).
pub struct UserPartition;
impl Partition for UserPartition {
    const ID: SceSysMemPartitionId = SceSysMemPartitionId::SceKernelPrimaryUserPartition;
    const NAME: &'static str = "User";
}

/// Marker type for ME kernel partition 3 (ME-accessible, kernel only).
pub struct MePartition;
impl Partition for MePartition {
    const ID: SceSysMemPartitionId = SceSysMemPartitionId::SceKernelOtherKernelPartition1;
    const NAME: &'static str = "ME";
}

/// A typed allocation from a specific memory partition.
///
/// The partition type parameter `P` ensures at compile time that you
/// cannot pass a `PartitionAlloc<UserPartition, T>` where a
/// `PartitionAlloc<MePartition, T>` is expected.
///
/// # Example
///
/// ```ignore
/// use psp::mem::{PartitionAlloc, UserPartition, MePartition};
///
/// // User-mode allocation
/// let user_buf = PartitionAlloc::<UserPartition, [u8; 1024]>::new(
///     [0u8; 1024], b"mybuf\0"
/// ).unwrap();
///
/// // ME-accessible allocation (kernel mode required)
/// let me_buf = PartitionAlloc::<MePartition, u32>::new(
///     0u32, b"mebuf\0"
/// ).unwrap();
/// ```
pub struct PartitionAlloc<P: Partition, T> {
    ptr: *mut T,
    block_id: SceUid,
    /// Whether the value at `ptr` has been initialized and needs dropping.
    /// Set to `false` for `new_uninit()` allocations to prevent UB from
    /// calling `drop_in_place` on uninitialized memory.
    initialized: bool,
    _partition: PhantomData<P>,
}

/// Convenience alias for user-mode partition 2 allocations.
pub type Partition2Alloc<T> = PartitionAlloc<UserPartition, T>;

/// Convenience alias for ME kernel partition 3 allocations.
#[cfg(feature = "kernel")]
pub type Partition3Alloc<T> = PartitionAlloc<MePartition, T>;

impl<P: Partition, T> PartitionAlloc<P, T> {
    /// Allocate memory in partition `P` and initialize it with `val`.
    ///
    /// `name` must be a null-terminated byte string used by the kernel
    /// for identification (e.g., `b"myalloc\0"`).
    ///
    /// # Errors
    ///
    /// Returns the negative PSP error code if allocation fails.
    pub fn new(val: T, name: &[u8]) -> Result<Self, i32> {
        let size = core::mem::size_of::<T>().max(1) as u32;

        let block_id = unsafe {
            sceKernelAllocPartitionMemory(
                P::ID,
                name.as_ptr(),
                SceSysMemBlockTypes::Low,
                size,
                core::ptr::null_mut(),
            )
        };

        if block_id.0 < 0 {
            return Err(block_id.0);
        }

        let ptr = unsafe { sceKernelGetBlockHeadAddr(block_id) } as *mut T;

        // SAFETY: ptr is valid and properly sized
        unsafe {
            core::ptr::write(ptr, val);
        }

        Ok(Self {
            ptr,
            block_id,
            initialized: true,
            _partition: PhantomData,
        })
    }

    /// Allocate uninitialized memory in partition `P`.
    ///
    /// # Safety
    ///
    /// The caller must initialize the memory before reading from it.
    pub unsafe fn new_uninit(size: u32, name: &[u8]) -> Result<Self, i32> {
        let block_id = unsafe {
            sceKernelAllocPartitionMemory(
                P::ID,
                name.as_ptr(),
                SceSysMemBlockTypes::Low,
                size,
                core::ptr::null_mut(),
            )
        };

        if block_id.0 < 0 {
            return Err(block_id.0);
        }

        let ptr = unsafe { sceKernelGetBlockHeadAddr(block_id) } as *mut T;

        Ok(Self {
            ptr,
            block_id,
            initialized: false,
            _partition: PhantomData,
        })
    }

    /// Mark the allocation as initialized.
    ///
    /// After calling this, `Drop` will call `drop_in_place` on the value.
    /// Call this after writing a valid `T` into the allocation.
    ///
    /// # Safety
    ///
    /// The caller must have written a valid, initialized `T` to the pointer.
    pub unsafe fn assume_init(&mut self) {
        self.initialized = true;
    }

    /// Get a raw pointer to the allocated memory.
    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }

    /// Get a mutable raw pointer to the allocated memory.
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr
    }

    /// Get a reference to the allocated value.
    ///
    /// # Safety
    ///
    /// The caller must ensure no concurrent mutable access exists.
    pub unsafe fn as_ref(&self) -> &T {
        unsafe { &*self.ptr }
    }

    /// Get a mutable reference to the allocated value.
    ///
    /// # Safety
    ///
    /// The caller must ensure exclusive access.
    pub unsafe fn as_mut(&mut self) -> &mut T {
        unsafe { &mut *self.ptr }
    }

    /// Get the kernel block ID (for manual management).
    pub fn block_id(&self) -> SceUid {
        self.block_id
    }
}

// ME partition allocations can convert to uncached pointers
#[cfg(feature = "kernel")]
impl<T> PartitionAlloc<MePartition, T> {
    /// Get an uncached pointer to this ME-accessible allocation.
    ///
    /// The ME requires uncached addresses. This method ORs the pointer
    /// with `0x4000_0000` to bypass the CPU data cache.
    pub fn as_uncached_ptr(&self) -> *mut T {
        crate::me::to_uncached(self.ptr)
    }
}

// SAFETY: PartitionAlloc owns its allocation and can be sent across threads.
unsafe impl<P: Partition, T: Send> Send for PartitionAlloc<P, T> {}

impl<P: Partition, T> Drop for PartitionAlloc<P, T> {
    fn drop(&mut self) {
        unsafe {
            // Only drop the value if it was initialized (prevents UB for new_uninit).
            if self.initialized {
                core::ptr::drop_in_place(self.ptr);
            }
            sceKernelFreePartitionMemory(self.block_id);
        }
    }
}

impl<P: Partition, T: core::fmt::Debug> core::fmt::Debug for PartitionAlloc<P, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PartitionAlloc")
            .field("partition", &P::NAME)
            .field("ptr", &self.ptr)
            .field("block_id", &self.block_id)
            .finish()
    }
}

/// Allocate a byte buffer in user partition 2.
///
/// Convenience function for the common case of allocating a byte buffer.
pub fn alloc_user_bytes(size: u32, name: &[u8]) -> Result<PartitionAlloc<UserPartition, u8>, i32> {
    // SAFETY: Byte buffers don't need initialization
    unsafe { PartitionAlloc::<UserPartition, u8>::new_uninit(size, name) }
}

/// Allocate a byte buffer in ME kernel partition 3.
///
/// Convenience function for kernel-mode ME-accessible buffer allocation.
#[cfg(feature = "kernel")]
pub fn alloc_me_bytes(size: u32, name: &[u8]) -> Result<PartitionAlloc<MePartition, u8>, i32> {
    // SAFETY: Byte buffers don't need initialization
    unsafe { PartitionAlloc::<MePartition, u8>::new_uninit(size, name) }
}
