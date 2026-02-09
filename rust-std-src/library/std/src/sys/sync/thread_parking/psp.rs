use crate::pin::Pin;
use crate::sync::atomic::{AtomicI32, Ordering};
use crate::time::Duration;

unsafe extern "C" {
    fn __psp_evflag_create(name: *const u8, attr: u32, bits: u32) -> i32;
    fn __psp_evflag_delete(id: i32) -> i32;
    fn __psp_evflag_wait(id: i32, bits: u32, wait: i32, out_bits: *mut u32, timeout: *mut u32)
        -> i32;
    fn __psp_evflag_set(id: i32, bits: u32) -> i32;
}

// Wait mode flags for sceKernelWaitEventFlag
const WAIT_OR: i32 = 0x01;
const WAIT_CLEAR: i32 = 0x20;

// Signal bit used for park/unpark
const PARK_BIT: u32 = 0x01;

/// PSP Parker implementation using event flags.
///
/// Uses a per-Parker event flag for signaling:
/// - `park()` waits on the event flag bit (infinite timeout)
/// - `park_timeout()` waits on the event flag bit with a timeout
/// - `unpark()` sets the event flag bit
///
/// The event flag bit acts as the "permit": setting it before a wait
/// causes the wait to return immediately, matching Rust's park/unpark semantics.
pub struct Parker {
    // Event flag ID (lazily initialized)
    evflag_id: AtomicI32,
}

unsafe impl Send for Parker {}
unsafe impl Sync for Parker {}

impl Parker {
    pub fn new() -> Parker {
        Parker {
            evflag_id: AtomicI32::new(-1),
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

    fn ensure_init(&self) -> i32 {
        let id = self.evflag_id.load(Ordering::Acquire);
        if id >= 0 {
            return id;
        }

        let name = b"std_park\0";
        // Create event flag with no initial bits set
        let new_id = unsafe { __psp_evflag_create(name.as_ptr(), 0x200, 0) };

        if new_id >= 0 {
            match self
                .evflag_id
                .compare_exchange(-1, new_id, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => new_id,
                Err(existing) => {
                    unsafe { __psp_evflag_delete(new_id) };
                    existing
                }
            }
        } else {
            -1
        }
    }

    pub unsafe fn park(self: Pin<&Self>) {
        let id = self.ensure_init();
        if id < 0 {
            return;
        }
        // Wait for the park bit, clearing it atomically when received.
        // If unpark was called before park, the bit is already set and
        // this returns immediately (consuming the permit).
        let mut out_bits: u32 = 0;
        unsafe {
            __psp_evflag_wait(
                id,
                PARK_BIT,
                WAIT_OR | WAIT_CLEAR,
                &mut out_bits,
                core::ptr::null_mut(), // infinite timeout
            );
        }
    }

    pub unsafe fn park_timeout(self: Pin<&Self>, dur: Duration) {
        let id = self.ensure_init();
        if id < 0 {
            return;
        }
        let us = dur.as_micros();
        let mut timeout = if us > u32::MAX as u128 { u32::MAX } else { us as u32 };
        let mut out_bits: u32 = 0;
        // Wait for the park bit with timeout. If unpark signals during the wait,
        // we wake early. On timeout, the bit is not cleared.
        unsafe {
            __psp_evflag_wait(
                id,
                PARK_BIT,
                WAIT_OR | WAIT_CLEAR,
                &mut out_bits,
                &mut timeout,
            );
        }
    }

    pub fn unpark(self: Pin<&Self>) {
        let id = self.ensure_init();
        if id >= 0 {
            // Set the park bit. If the thread is waiting, it wakes up.
            // If not yet parked, the bit persists as a permit for the next park.
            unsafe { __psp_evflag_set(id, PARK_BIT) };
        }
    }
}

impl Drop for Parker {
    fn drop(&mut self) {
        let id = *self.evflag_id.get_mut();
        if id >= 0 {
            unsafe { __psp_evflag_delete(id) };
        }
    }
}
