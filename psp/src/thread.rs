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
    // Box the closure and convert to a raw pointer for the trampoline.
    let boxed: Box<dyn FnOnce() -> i32 + Send + 'static> = Box::new(f);
    let raw = Box::into_raw(Box::new(boxed));

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
        // Thread creation failed — reclaim the closure.
        unsafe {
            drop(Box::from_raw(raw));
        }
        return Err(ThreadError(thid.0));
    }

    // Start the thread, passing the closure pointer as the argument.
    let ret = unsafe {
        sceKernelStartThread(
            thid,
            core::mem::size_of::<*mut c_void>(),
            &raw as *const _ as *mut c_void,
        )
    };

    if ret < 0 {
        // Start failed — clean up the thread and closure.
        unsafe {
            sceKernelDeleteThread(thid);
            drop(Box::from_raw(raw));
        }
        return Err(ThreadError(ret));
    }

    Ok(JoinHandle {
        thid,
        joined: false,
    })
}

/// C-callable trampoline that runs the boxed closure.
///
/// The PSP passes `argp` pointing to a buffer containing the raw pointer
/// to our `Box<dyn FnOnce() -> i32>`.
///
/// Panics are caught with `catch_unwind` to prevent unwinding across the
/// `extern "C"` boundary, which would abort the process.
unsafe extern "C" fn trampoline(_args: usize, argp: *mut c_void) -> i32 {
    let ptr_to_box = argp as *const *mut (dyn FnOnce() -> i32 + Send + 'static);
    let raw = unsafe { *ptr_to_box };
    let closure = unsafe { Box::from_raw(raw) };
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
}

impl JoinHandle {
    /// Block until the thread exits and return its exit status.
    pub fn join(mut self) -> Result<i32, ThreadError> {
        let ret = unsafe { sceKernelWaitThreadEnd(self.thid, core::ptr::null_mut()) };
        if ret < 0 {
            return Err(ThreadError(ret));
        }
        // Retrieve the actual thread exit status.
        let exit_status = unsafe { sceKernelGetThreadExitStatus(self.thid) };
        self.joined = true;
        let del = unsafe { sceKernelDeleteThread(self.thid) };
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
        if !self.joined {
            // Thread was not joined — forcibly terminate and delete it.
            unsafe {
                sceKernelTerminateDeleteThread(self.thid);
            }
        }
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
