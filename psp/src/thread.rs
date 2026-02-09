//! Thread spawning and management for the PSP.
//!
//! Provides a closure-based [`spawn()`] function and [`JoinHandle`] for
//! waiting on thread completion, similar to `std::thread` but tailored
//! to the PSP's threading model.
//!
//! # Example
//!
//! ```ignore
//! use psp::thread;
//!
//! let handle = thread::spawn(b"worker\0", || {
//!     // do background work
//!     42
//! }).unwrap();
//!
//! let result = handle.join().unwrap();
//! assert_eq!(result, 42);
//! ```

use crate::sys::{
    SceUid, ThreadAttributes, sceKernelCreateThread, sceKernelDelayThread, sceKernelDeleteThread,
    sceKernelGetThreadExitStatus, sceKernelGetThreadId, sceKernelSleepThread, sceKernelStartThread,
    sceKernelTerminateDeleteThread, sceKernelWaitThreadEnd,
};
use alloc::boxed::Box;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

// ── ThreadError ─────────────────────────────────────────────────────

/// Error from a PSP thread operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ThreadError(pub i32);

impl ThreadError {
    pub fn code(self) -> i32 {
        self.0
    }
}

impl core::fmt::Debug for ThreadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ThreadError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for ThreadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "thread error {:#010x}", self.0 as u32)
    }
}

// ── ThreadBuilder ───────────────────────────────────────────────────

/// Builder for configuring and spawning threads.
///
/// # Example
///
/// ```ignore
/// use psp::thread::ThreadBuilder;
/// use psp::sys::ThreadAttributes;
///
/// let handle = ThreadBuilder::new(b"my_thread\0")
///     .priority(48)
///     .stack_size(64 * 1024)
///     .attributes(ThreadAttributes::USER | ThreadAttributes::VFPU)
///     .spawn(|| 0)
///     .unwrap();
/// ```
pub struct ThreadBuilder {
    name: &'static [u8],
    priority: i32,
    stack_size: i32,
    attributes: ThreadAttributes,
}

impl ThreadBuilder {
    /// Create a new builder. `name` must be a null-terminated byte string.
    pub fn new(name: &'static [u8]) -> Self {
        Self {
            name,
            priority: 32,
            stack_size: 64 * 1024,
            attributes: ThreadAttributes::USER | ThreadAttributes::VFPU,
        }
    }

    /// Set the initial thread priority (lower = higher priority).
    pub fn priority(mut self, prio: i32) -> Self {
        self.priority = prio;
        self
    }

    /// Set the thread stack size in bytes.
    pub fn stack_size(mut self, size: i32) -> Self {
        self.stack_size = size;
        self
    }

    /// Set thread attributes.
    pub fn attributes(mut self, attr: ThreadAttributes) -> Self {
        self.attributes = attr;
        self
    }

    /// Spawn the thread, running `f` on it.
    ///
    /// The closure must be `Send + 'static` because it runs on a different
    /// thread. It returns an `i32` which becomes the thread's exit status.
    pub fn spawn<F: FnOnce() -> i32 + Send + 'static>(
        self,
        f: F,
    ) -> Result<JoinHandle, ThreadError> {
        spawn_inner(
            self.name,
            self.priority,
            self.stack_size,
            self.attributes,
            f,
        )
    }
}

// ── ThreadPayload ───────────────────────────────────────────────────

/// Shared state between the trampoline and `JoinHandle` to prevent
/// double-free of the closure when a thread finishes between the
/// zero-timeout wait check and `sceKernelTerminateDeleteThread` in Drop.
struct ThreadPayload {
    closure: Option<Box<dyn FnOnce() -> i32 + Send + 'static>>,
    /// Set to `true` by the trampoline after consuming the closure.
    consumed: AtomicBool,
}

// ── spawn ───────────────────────────────────────────────────────────

/// Spawn a thread with default settings.
///
/// Equivalent to `ThreadBuilder::new(name).spawn(f)`.
///
/// - Priority: 32
/// - Stack size: 64 KiB
/// - Attributes: USER | VFPU
pub fn spawn<F: FnOnce() -> i32 + Send + 'static>(
    name: &'static [u8],
    f: F,
) -> Result<JoinHandle, ThreadError> {
    ThreadBuilder::new(name).spawn(f)
}

/// Internal spawn implementation.
fn spawn_inner<F: FnOnce() -> i32 + Send + 'static>(
    name: &'static [u8],
    priority: i32,
    stack_size: i32,
    attributes: ThreadAttributes,
    f: F,
) -> Result<JoinHandle, ThreadError> {
    // Validate null termination — the PSP kernel expects a C string.
    // Without this check, safe code could cause out-of-bounds reads.
    if name.last() != Some(&0) {
        return Err(ThreadError(-1));
    }

    // Box the closure into a ThreadPayload with an atomic flag.
    let payload = Box::into_raw(Box::new(ThreadPayload {
        closure: Some(Box::new(f)),
        consumed: AtomicBool::new(false),
    }));

    let thid = unsafe {
        sceKernelCreateThread(
            name.as_ptr(),
            trampoline,
            priority,
            stack_size,
            attributes,
            core::ptr::null_mut(),
        )
    };

    if thid.0 < 0 {
        // Thread creation failed — reclaim the payload.
        unsafe {
            drop(Box::from_raw(payload));
        }
        return Err(ThreadError(thid.0));
    }

    // Start the thread, passing the payload pointer as the argument.
    let ret = unsafe {
        sceKernelStartThread(
            thid,
            core::mem::size_of::<*mut c_void>(),
            &payload as *const _ as *mut c_void,
        )
    };

    if ret < 0 {
        // Start failed — clean up the thread and payload.
        unsafe {
            sceKernelDeleteThread(thid);
            drop(Box::from_raw(payload));
        }
        return Err(ThreadError(ret));
    }

    Ok(JoinHandle {
        thid,
        joined: false,
        payload,
    })
}

/// C-callable trampoline that runs the boxed closure.
///
/// The PSP passes `argp` pointing to a buffer containing the raw pointer
/// to our `ThreadPayload`. The payload holds the closure and an atomic
/// flag that we set after consuming the closure, preventing the
/// `JoinHandle::drop` from double-freeing it.
///
/// Panics are caught with `catch_unwind` to prevent unwinding across the
/// `extern "C"` boundary, which would abort the process.
unsafe extern "C" fn trampoline(_args: usize, argp: *mut c_void) -> i32 {
    // `argp` points to a buffer containing a pointer to ThreadPayload.
    let ptr_to_payload = argp as *const *mut ThreadPayload;
    let payload = unsafe { &mut **ptr_to_payload };
    // Take the closure out of the payload.
    let closure = payload.closure.take().unwrap();
    // Mark as consumed BEFORE running, so Drop won't try to free it
    // even if the thread is terminated mid-execution.
    payload.consumed.store(true, Ordering::Release);
    match crate::catch_unwind(core::panic::AssertUnwindSafe(closure)) {
        Ok(code) => code,
        Err(_) => -0x7FFF_FFFF, // panic sentinel
    }
}

// ── JoinHandle ──────────────────────────────────────────────────────

/// A handle to a spawned thread.
///
/// Can be used to wait for the thread to finish. If dropped without
/// calling [`join()`](Self::join), the thread is terminated and deleted.
pub struct JoinHandle {
    thid: SceUid,
    joined: bool,
    /// Shared payload containing the closure and a "consumed" flag.
    /// The trampoline sets `consumed` after taking the closure, so
    /// Drop can safely check whether it needs to free the closure.
    payload: *mut ThreadPayload,
}

// SAFETY: The payload pointer is only accessed after the thread is
// terminated (in drop) or after it has finished (in join). The handle
// itself can safely be sent to another thread.
unsafe impl Send for JoinHandle {}

impl JoinHandle {
    /// Block until the thread exits and return its exit status.
    pub fn join(mut self) -> Result<i32, ThreadError> {
        let ret = unsafe { sceKernelWaitThreadEnd(self.thid, core::ptr::null_mut()) };
        if ret < 0 {
            return Err(ThreadError(ret));
        }
        self.joined = true;
        // Retrieve the actual thread exit status.
        let exit_status = unsafe { sceKernelGetThreadExitStatus(self.thid) };
        let del = unsafe { sceKernelDeleteThread(self.thid) };
        // Free the payload (closure was already consumed by trampoline).
        unsafe { drop(Box::from_raw(self.payload)) };
        self.payload = core::ptr::null_mut();
        if del < 0 {
            return Err(ThreadError(del));
        }
        Ok(exit_status)
    }

    /// Get the thread's kernel UID.
    pub fn id(&self) -> SceUid {
        self.thid
    }
}

impl Drop for JoinHandle {
    fn drop(&mut self) {
        if self.joined || self.payload.is_null() {
            return;
        }
        // Forcibly terminate and delete the thread. This is synchronous:
        // after it returns the thread is dead.
        unsafe { sceKernelTerminateDeleteThread(self.thid) };
        // Check the atomic flag to determine if the trampoline already
        // consumed the closure. This prevents a double-free race where
        // the thread finishes between the wait-check and terminate.
        let payload = unsafe { Box::from_raw(self.payload) };
        if payload.consumed.load(Ordering::Acquire) {
            // Trampoline already took the closure — nothing more to free.
            // The payload Box itself is freed when `payload` drops here.
        }
        // If !consumed, the closure is still in payload.closure and will
        // be dropped when `payload` drops here.
    }
}

// ── Free functions ──────────────────────────────────────────────────

/// Sleep the current thread for `ms` milliseconds.
pub fn sleep_ms(ms: u32) {
    let us = (ms as u64 * 1000).min(u32::MAX as u64) as u32;
    unsafe {
        sceKernelDelayThread(us);
    }
}

/// Put the current thread to sleep (woken by `sceKernelWakeupThread`).
pub fn sleep_thread() {
    unsafe {
        sceKernelSleepThread();
    }
}

/// Get the UID of the current thread.
pub fn current_thread_id() -> SceUid {
    let id = unsafe { sceKernelGetThreadId() };
    SceUid(id)
}
