//! Media Engine (ME) boot and task management.
//!
//! The PSP contains a second MIPS R4000 core (the "Media Engine") running
//! at up to 333MHz. In kernel mode, applications can boot the ME, submit
//! tasks to it, and use it for hardware-accelerated media decoding.
//!
//! # Architecture
//!
//! The ME does not have a formal syscall API. The traditional approach is:
//! 1. Allocate memory in an ME-accessible partition (partition 3 or 10)
//! 2. Write task code + data to that memory
//! 3. Boot the ME with a jump to the task entry point
//! 4. Synchronize via shared memory flags
//!
//! All shared memory between the main CPU and ME must use **uncached**
//! addresses (OR'd with `0x4000_0000`). The ME cannot access cached main
//! RAM coherently.
//!
//! # Kernel Mode Required
//!
//! All functions in this module require `feature = "kernel"` and the module
//! must be declared with `psp::module_kernel!()`.

use core::ffi::c_void;

/// Uncached memory address mask.
///
/// OR any physical address with this value to get the uncached equivalent.
/// Required for all memory shared between the main CPU and the ME.
pub const UNCACHED_MASK: u32 = 0x4000_0000;

/// ME task function signature.
///
/// The ME executes this function on its own core with its own stack.
/// The function receives a single `i32` argument and returns an `i32` result.
pub type MeTask = unsafe extern "C" fn(arg: i32) -> i32;

/// Parameters passed to the ME boot entry point.
///
/// This struct is placed in ME-accessible (uncached) memory and its address
/// is passed to `_me_boot_entry` in `$a0`.
#[repr(C)]
pub struct MeBootParams {
    /// Pointer to the task function to execute on the ME.
    pub task: MeTask,
    /// Argument passed to the task function.
    pub arg: i32,
    /// Top of the ME stack (stack grows downward).
    pub stack_top: *mut u8,
}

// The ME boot entry point is defined in asm/me.S
#[cfg(all(target_os = "psp", feature = "kernel"))]
core::arch::global_asm!(include_str!("asm/me.S"));

#[cfg(all(target_os = "psp", feature = "kernel"))]
unsafe extern "C" {
    fn _me_boot_entry(params: *const MeBootParams);
}

/// Boot the Media Engine and execute a task on it.
///
/// This function writes the boot parameters to ME-accessible memory and
/// triggers the ME to start executing the given task.
///
/// # Safety
///
/// - `task` must be a valid function pointer in ME-accessible memory
///   (partition 3 or 10, uncached address with `UNCACHED_MASK`).
/// - `stack_top` must point to the top of a valid stack in ME-accessible
///   memory. The stack must be large enough for the task's needs.
/// - The caller must be running in kernel mode.
/// - Only one ME task can run at a time.
/// - The caller must ensure the task code is flushed from the data cache
///   and the instruction cache is invalidated before calling this function.
#[cfg(all(target_os = "psp", feature = "kernel"))]
pub unsafe fn me_boot(params: &MeBootParams) {
    // Flush data cache to ensure the ME can see the parameters
    unsafe {
        crate::sys::sceKernelDcacheWritebackInvalidateAll();
    }

    // Boot the ME by calling into the assembly entry point
    unsafe {
        _me_boot_entry(params as *const MeBootParams);
    }
}

/// Convert a cached address to its uncached equivalent.
///
/// The ME cannot access cached main RAM coherently. All memory shared
/// between the main CPU and the ME must use uncached addresses.
///
/// # Example
///
/// ```ignore
/// let cached_ptr: *mut u8 = /* allocated pointer */;
/// let uncached_ptr = psp::me::to_uncached(cached_ptr);
/// ```
#[inline(always)]
pub fn to_uncached<T>(ptr: *mut T) -> *mut T {
    (ptr as u32 | UNCACHED_MASK) as *mut T
}

/// Convert an uncached address back to its cached equivalent.
#[inline(always)]
pub fn to_cached<T>(ptr: *mut T) -> *mut T {
    (ptr as u32 & !UNCACHED_MASK) as *mut T
}

/// Allocate memory in the ME kernel partition (partition 3).
///
/// Returns an uncached pointer suitable for ME access. The caller is
/// responsible for freeing the allocation via
/// `sceKernelFreePartitionMemory`.
///
/// # Safety
///
/// - Caller must be in kernel mode.
/// - The returned memory block ID must be retained for later deallocation.
///
/// # Errors
///
/// Returns the negative PSP error code if allocation fails.
#[cfg(feature = "kernel")]
pub unsafe fn me_alloc(size: u32, name: *const u8) -> Result<(*mut u8, crate::sys::SceUid), i32> {
    use crate::sys::{
        SceSysMemBlockTypes, SceSysMemPartitionId, sceKernelAllocPartitionMemory,
        sceKernelGetBlockHeadAddr,
    };

    let block_id = unsafe {
        sceKernelAllocPartitionMemory(
            SceSysMemPartitionId::SceKernelOtherKernelPartition1,
            name,
            SceSysMemBlockTypes::Low,
            size,
            core::ptr::null_mut(),
        )
    };

    if block_id.0 < 0 {
        return Err(block_id.0);
    }

    let ptr = unsafe { sceKernelGetBlockHeadAddr(block_id) } as *mut u8;
    let uncached_ptr = to_uncached(ptr);

    Ok((uncached_ptr, block_id))
}
