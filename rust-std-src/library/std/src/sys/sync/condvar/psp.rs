use crate::sync::atomic::{AtomicI32, Ordering};
use crate::sys::sync::Mutex;
use crate::time::Duration;

unsafe extern "C" {
    fn __psp_evflag_create(name: *const u8, attr: u32, bits: u32) -> i32;
    fn __psp_evflag_delete(id: i32) -> i32;
    fn __psp_evflag_wait(id: i32, bits: u32, wait: i32, out_bits: *mut u32, timeout: *mut u32)
        -> i32;
    fn __psp_evflag_set(id: i32, bits: u32) -> i32;
    fn __psp_evflag_clear(id: i32, bits: u32) -> i32;
}

// Wait mode flags for sceKernelWaitEventFlag
const WAIT_OR: i32 = 0x01;
const WAIT_CLEAR: i32 = 0x20;

// Signal bit used for condvar notification
const NOTIFY_BIT: u32 = 0x01;

pub struct Condvar {
    // Event flag ID for signaling waiters
    evflag_id: AtomicI32,
}

unsafe impl Send for Condvar {}
unsafe impl Sync for Condvar {}

impl Condvar {
    pub const fn new() -> Condvar {
        Condvar {
            evflag_id: AtomicI32::new(-1),
        }
    }

    fn ensure_init(&self) -> i32 {
        let id = self.evflag_id.load(Ordering::Acquire);
        if id >= 0 {
            return id;
        }

        // Try to create the event flag
        let name = b"std_cv\0";
        // MULTI wait mode allows multiple threads to wait
        let new_id = unsafe { __psp_evflag_create(name.as_ptr(), 0x200, 0) };

        if new_id >= 0 {
            match self
                .evflag_id
                .compare_exchange(-1, new_id, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => new_id,
                Err(existing) => {
                    // Another thread created it first, delete ours
                    unsafe { __psp_evflag_delete(new_id) };
                    existing
                }
            }
        } else {
            // Creation failed
            -1
        }
    }

    pub fn notify_one(&self) {
        let id = self.ensure_init();
        if id >= 0 {
            // Set the notification bit -- one waiter will pick it up and clear it
            unsafe { __psp_evflag_set(id, NOTIFY_BIT) };
        }
    }

    pub fn notify_all(&self) {
        // Same as notify_one on PSP -- all waiters check the bit
        self.notify_one();
    }

    pub unsafe fn wait(&self, mutex: &Mutex) {
        let id = self.ensure_init();
        if id < 0 {
            return;
        }

        // Unlock the mutex, wait for signal, re-lock
        unsafe { mutex.unlock() };

        let mut out_bits: u32 = 0;
        unsafe {
            __psp_evflag_wait(
                id,
                NOTIFY_BIT,
                WAIT_OR | WAIT_CLEAR,
                &mut out_bits,
                core::ptr::null_mut(), // infinite timeout
            );
        }

        mutex.lock();
    }

    pub unsafe fn wait_timeout(&self, mutex: &Mutex, dur: Duration) -> bool {
        let id = self.ensure_init();
        if id < 0 {
            return false;
        }

        unsafe { mutex.unlock() };

        let us = dur.as_micros();
        let mut timeout = if us > u32::MAX as u128 { u32::MAX } else { us as u32 };
        let mut out_bits: u32 = 0;

        let ret = unsafe {
            __psp_evflag_wait(id, NOTIFY_BIT, WAIT_OR | WAIT_CLEAR, &mut out_bits, &mut timeout)
        };

        mutex.lock();

        // Return true if we were signaled (not timed out)
        ret >= 0
    }
}

impl Drop for Condvar {
    fn drop(&mut self) {
        let id = *self.evflag_id.get_mut();
        if id >= 0 {
            unsafe { __psp_evflag_delete(id) };
        }
    }
}
