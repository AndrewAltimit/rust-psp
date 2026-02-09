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
//! # High-Level API
//!
//! The [`MeExecutor`] provides a safe, high-level task submission API that
//! handles uncached memory allocation, shared synchronization state, and
//! cache management internally.
//!
//! ```ignore
//! use psp::me::MeExecutor;
//!
//! unsafe extern "C" fn my_task(arg: i32) -> i32 { arg * 2 }
//!
//! let mut executor = MeExecutor::new(4096).unwrap();
//! let handle = unsafe { executor.submit(my_task, 21) };
//! let result = executor.wait(&handle); // returns 42
//! ```
//!
//! # Kernel Mode Required
//!
//! All functions in this module require `feature = "kernel"` and the module
//! must be declared with `psp::module_kernel!()`.

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

// ── MeExecutor ──────────────────────────────────────────────────────

/// Status values for ME task slots, stored in uncached shared memory.
#[cfg(feature = "kernel")]
mod status {
    /// Slot is available for a new task.
    pub const IDLE: u32 = 0;
    /// Task has been submitted and is running on the ME.
    pub const RUNNING: u32 = 1;
    /// Task has completed; result is available.
    pub const DONE: u32 = 2;
}

/// Shared state between the main CPU and ME for a single task.
///
/// This struct lives in uncached memory. The ME writes `status` and
/// `result` when the task completes; the main CPU reads them.
///
/// `real_task` and `real_arg` are written by the main CPU before booting
/// the ME. The ME wrapper reads them from here rather than from
/// `boot_params`, avoiding a race where `boot_params` would need to be
/// written twice.
#[cfg(feature = "kernel")]
#[repr(C)]
struct MeSharedState {
    /// Task status (see [`status`] module).
    status: u32,
    /// Task return value (valid when `status == DONE`).
    result: i32,
    /// The actual user task, stored separately from boot_params.
    real_task: MeTask,
    /// The actual user argument, stored separately from boot_params.
    real_arg: i32,
    /// Boot parameters for the ME (always points to the wrapper).
    boot_params: MeBootParams,
}

/// An opaque handle to a submitted ME task.
///
/// Use with [`MeExecutor::poll`] or [`MeExecutor::wait`] to retrieve
/// the result.
#[cfg(feature = "kernel")]
#[derive(Debug, Clone, Copy)]
pub struct MeHandle {
    /// Index into the shared state — currently always 0 since the ME
    /// can only run one task at a time.
    _slot: u32,
}

/// High-level Media Engine task executor.
///
/// Manages uncached memory allocation, ME boot parameters, and
/// synchronization internally. Submit tasks with [`submit`](Self::submit),
/// then poll or wait for results.
///
/// # Example
///
/// ```ignore
/// use psp::me::MeExecutor;
///
/// unsafe extern "C" fn double(arg: i32) -> i32 { arg * 2 }
///
/// let mut executor = MeExecutor::new(4096).unwrap();
/// let handle = unsafe { executor.submit(double, 21) };
/// assert_eq!(executor.wait(&handle), 42);
/// ```
#[cfg(feature = "kernel")]
pub struct MeExecutor {
    /// Pointer to the shared state in uncached memory.
    shared: *mut MeSharedState,
    /// Block ID for the shared state allocation.
    shared_block: crate::sys::SceUid,
    /// Pointer to the ME stack in uncached memory.
    stack_base: *mut u8,
    /// Block ID for the stack allocation.
    stack_block: crate::sys::SceUid,
    /// Size of the ME stack.
    stack_size: u32,
}

#[cfg(feature = "kernel")]
impl MeExecutor {
    /// Create a new `MeExecutor` with the given ME stack size.
    ///
    /// Allocates shared state and stack memory in ME-accessible partition 3.
    /// `stack_size` should be at least 4096 bytes for most tasks.
    ///
    /// # Errors
    ///
    /// Returns the PSP error code if memory allocation fails.
    pub fn new(stack_size: u32) -> Result<Self, i32> {
        let shared_size = core::mem::size_of::<MeSharedState>() as u32;

        // SAFETY: Kernel mode is required. We allocate from partition 3.
        let (shared_ptr, shared_block) =
            unsafe { me_alloc(shared_size, b"MeExecState\0".as_ptr()) }?;
        let shared = shared_ptr as *mut MeSharedState;

        let (stack_base, stack_block) =
            match unsafe { me_alloc(stack_size, b"MeExecStack\0".as_ptr()) } {
                Ok(v) => v,
                Err(e) => {
                    // Clean up the shared state allocation
                    unsafe {
                        crate::sys::sceKernelFreePartitionMemory(shared_block);
                    }
                    return Err(e);
                },
            };

        // Initialize shared state to idle
        // SAFETY: shared is a valid uncached pointer.
        unsafe {
            core::ptr::write_volatile(&raw mut (*shared).status, status::IDLE);
            core::ptr::write_volatile(&raw mut (*shared).result, 0);
        }

        Ok(Self {
            shared,
            shared_block,
            stack_base,
            stack_block,
            stack_size,
        })
    }

    /// Submit a task to the Media Engine.
    ///
    /// The ME will execute `task(arg)` on its own core. Use the returned
    /// [`MeHandle`] with [`poll`](Self::poll) or [`wait`](Self::wait) to
    /// retrieve the result.
    ///
    /// # Safety
    ///
    /// - Only one task can run at a time. Calling `submit` while a
    ///   previous task is still running is undefined behavior.
    /// - `task` must be safe to execute on the ME core (no syscalls,
    ///   no cached memory access, no floating-point context sharing).
    /// - The caller must be in kernel mode.
    #[cfg(all(target_os = "psp", feature = "kernel"))]
    pub unsafe fn submit(&mut self, task: MeTask, arg: i32) -> MeHandle {
        // Wrapper that reads the real task from shared state, executes it,
        // then writes the result and status. The ME cannot call PSP syscalls,
        // so the wrapper writes directly to the uncached shared state.
        unsafe extern "C" fn me_wrapper(shared_addr: i32) -> i32 {
            let shared = shared_addr as *mut MeSharedState;
            let task: MeTask = core::ptr::read_volatile(&raw const (*shared).real_task);
            let arg = core::ptr::read_volatile(&raw const (*shared).real_arg);

            let result = task(arg);

            // Write result and mark as done (uncached memory, visible immediately)
            core::ptr::write_volatile(&raw mut (*shared).result, result);
            core::ptr::write_volatile(&raw mut (*shared).status, status::DONE);

            result
        }

        // Stack grows downward — point to the top
        let stack_top = self.stack_base.add(self.stack_size as usize);

        // Write the real task and arg to dedicated fields first
        unsafe {
            core::ptr::write_volatile(&raw mut (*self.shared).status, status::RUNNING);
            core::ptr::write_volatile(&raw mut (*self.shared).real_task, task);
            core::ptr::write_volatile(&raw mut (*self.shared).real_arg, arg);
        }

        // Write boot_params once with the wrapper — no second write needed
        unsafe {
            core::ptr::write_volatile(
                &raw mut (*self.shared).boot_params,
                MeBootParams {
                    task: me_wrapper,
                    arg: self.shared as i32,
                    stack_top,
                },
            );
        }

        // Boot the ME
        // SAFETY: All params are in uncached memory, kernel mode is required
        unsafe {
            me_boot(&(*self.shared).boot_params);
        }

        MeHandle { _slot: 0 }
    }

    /// Poll for task completion without blocking.
    ///
    /// Returns `Some(result)` if the task has completed, `None` if it's
    /// still running.
    pub fn poll(&self, _handle: &MeHandle) -> Option<i32> {
        // SAFETY: Reading from uncached memory — volatile access
        let st = unsafe { core::ptr::read_volatile(&raw const (*self.shared).status) };
        if st == status::DONE {
            let result = unsafe { core::ptr::read_volatile(&raw const (*self.shared).result) };
            Some(result)
        } else {
            None
        }
    }

    /// Block until the task completes and return its result.
    pub fn wait(&self, handle: &MeHandle) -> i32 {
        loop {
            if let Some(result) = self.poll(handle) {
                return result;
            }
            core::hint::spin_loop();
        }
    }

    /// Check if the executor is idle (no task running).
    pub fn is_idle(&self) -> bool {
        let st = unsafe { core::ptr::read_volatile(&raw const (*self.shared).status) };
        st != status::RUNNING
    }

    /// Reset the executor state to idle.
    ///
    /// Call this after retrieving a result to allow submitting new tasks.
    pub fn reset(&mut self) {
        unsafe {
            core::ptr::write_volatile(&raw mut (*self.shared).status, status::IDLE);
        }
    }
}

#[cfg(feature = "kernel")]
impl Drop for MeExecutor {
    fn drop(&mut self) {
        // SAFETY: We own these allocations
        unsafe {
            crate::sys::sceKernelFreePartitionMemory(self.stack_block);
            crate::sys::sceKernelFreePartitionMemory(self.shared_block);
        }
    }
}
