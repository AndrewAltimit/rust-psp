use crate::sync::atomic::{AtomicI32, AtomicU32, Ordering};
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

/// Condvar using per-waiter event flag bits.
///
/// Each waiter claims a unique bit (0-31) from the event flag. `notify_one`
/// sets a single active waiter's bit; `notify_all` sets all active bits
/// atomically in one syscall. This avoids the fragile yield-loop approach
/// and correctly handles priority inversion on PSP's single-core scheduler.
pub struct Condvar {
    // Event flag ID for signaling waiters
    evflag_id: AtomicI32,
    // Bitmask of bits currently claimed by waiting threads
    active_waiters: AtomicU32,
}

unsafe impl Send for Condvar {}
unsafe impl Sync for Condvar {}

impl Condvar {
    pub const fn new() -> Condvar {
        Condvar {
            evflag_id: AtomicI32::new(-1),
            active_waiters: AtomicU32::new(0),
        }
    }

    fn ensure_init(&self) -> i32 {
        let id = self.evflag_id.load(Ordering::Acquire);
        if id >= 0 {
            return id;
        }

        // Try to create the event flag
        let name = b"std_cv\0";
        // MULTI wait mode (0x200) allows multiple threads to wait simultaneously
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

    /// Claim a free bit from the active_waiters bitmask.
    /// Returns the bit mask (a single set bit), or 0 if all 32 bits are in use.
    fn claim_bit(&self) -> u32 {
        loop {
            let active = self.active_waiters.load(Ordering::Acquire);
            let free = !active;
            if free == 0 {
                return 0; // all 32 bits in use
            }
            let bit = free & free.wrapping_neg(); // lowest free bit
            match self.active_waiters.compare_exchange_weak(
                active,
                active | bit,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return bit,
                Err(_) => continue,
            }
        }
    }

    /// Release a previously claimed bit.
    fn release_bit(&self, bit: u32) {
        self.active_waiters.fetch_and(!bit, Ordering::Release);
    }

    pub fn notify_one(&self) {
        let id = self.ensure_init();
        if id < 0 {
            return;
        }
        let active = self.active_waiters.load(Ordering::Acquire);
        if active != 0 {
            // Pick the lowest active bit (one arbitrary waiter)
            let one_bit = active & active.wrapping_neg();
            unsafe { __psp_evflag_set(id, one_bit) };
        }
    }

    pub fn notify_all(&self) {
        let id = self.ensure_init();
        if id < 0 {
            return;
        }
        let active = self.active_waiters.load(Ordering::Acquire);
        if active != 0 {
            // Set ALL active waiter bits atomically -- wakes every waiter
            unsafe { __psp_evflag_set(id, active) };
        }
    }

    pub unsafe fn wait(&self, mutex: &Mutex) {
        let id = self.ensure_init();
        if id < 0 {
            return;
        }

        let my_bit = self.claim_bit();
        if my_bit == 0 {
            return; // shouldn't happen in practice (32 concurrent waiters)
        }

        // Unlock the mutex, wait for our specific bit, re-lock
        unsafe { mutex.unlock() };

        let mut out_bits: u32 = 0;
        unsafe {
            __psp_evflag_wait(
                id,
                my_bit,
                WAIT_OR | WAIT_CLEAR,
                &mut out_bits,
                core::ptr::null_mut(), // infinite timeout
            );
        }

        self.release_bit(my_bit);

        mutex.lock();
    }

    pub unsafe fn wait_timeout(&self, mutex: &Mutex, dur: Duration) -> bool {
        let id = self.ensure_init();
        if id < 0 {
            return false;
        }

        let my_bit = self.claim_bit();
        if my_bit == 0 {
            return false;
        }

        unsafe { mutex.unlock() };

        let us = dur.as_micros();
        let mut timeout = if us > u32::MAX as u128 { u32::MAX } else { us as u32 };
        let mut out_bits: u32 = 0;

        let ret = unsafe {
            __psp_evflag_wait(id, my_bit, WAIT_OR | WAIT_CLEAR, &mut out_bits, &mut timeout)
        };

        self.release_bit(my_bit);

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
