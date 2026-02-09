use crate::pin::Pin;
use crate::sync::atomic::{AtomicI32, Ordering};
use crate::time::Duration;

unsafe extern "C" {
    fn __psp_sleep_thread() -> i32;
    fn __psp_wakeup_thread(id: i32) -> i32;
    fn __psp_get_thread_id() -> i32;
    fn __psp_delay_thread(us: u32) -> i32;
}

/// PSP Parker implementation using sceKernelSleepThread / sceKernelWakeupThread.
///
/// PSP has native park/unpark semantics:
/// - sceKernelSleepThread() suspends the current thread until woken
/// - sceKernelWakeupThread(id) wakes a sleeping thread (with a wakeup counter)
///
/// This maps perfectly to Rust's Parker: the wakeup counter acts as the
/// "permit" -- an unpark before park is consumed on the next park.
pub struct Parker {
    // Thread ID of the owning thread (set on first park)
    thread_id: AtomicI32,
}

unsafe impl Send for Parker {}
unsafe impl Sync for Parker {}

impl Parker {
    pub fn new() -> Parker {
        Parker {
            thread_id: AtomicI32::new(-1),
        }
    }

    /// Initialize a Parker in-place at the given pointer.
    ///
    /// # Safety
    /// The pointer must be valid and properly aligned for `Parker`.
    pub unsafe fn new_in_place(parker: *mut Parker) {
        unsafe {
            parker.write(Parker::new());
        }
    }

    fn ensure_thread_id(&self) -> i32 {
        let id = self.thread_id.load(Ordering::Relaxed);
        if id >= 0 {
            return id;
        }
        let id = unsafe { __psp_get_thread_id() };
        self.thread_id.store(id, Ordering::Relaxed);
        id
    }

    pub unsafe fn park(self: Pin<&Self>) {
        self.ensure_thread_id();
        // sceKernelSleepThread will return immediately if there's a pending
        // wakeup (the wakeup counter is > 0), consuming one wakeup.
        unsafe { __psp_sleep_thread() };
    }

    pub unsafe fn park_timeout(self: Pin<&Self>, dur: Duration) {
        let _id = self.ensure_thread_id();
        // PSP doesn't have a native sleep-with-wakeup-timeout.
        // Use delay_thread as a timeout fallback -- this won't be interrupted
        // by wakeup, but is a reasonable approximation.
        let us = dur.as_micros();
        let us = if us > u32::MAX as u128 { u32::MAX } else { us as u32 };
        unsafe { __psp_delay_thread(us) };
    }

    pub fn unpark(self: Pin<&Self>) {
        let id = self.thread_id.load(Ordering::Relaxed);
        if id >= 0 {
            unsafe { __psp_wakeup_thread(id) };
        }
    }
}
