//! Timer and alarm abstractions for the PSP.
//!
//! Provides one-shot alarms with closure support and virtual timers
//! with RAII cleanup.

use crate::sys::{SceKernelVTimerHandlerWide, SceUid};
use core::ffi::c_void;

/// Error from a timer operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TimerError(pub i32);

impl core::fmt::Debug for TimerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "TimerError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for TimerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "timer error {:#010x}", self.0 as u32)
    }
}

// ── Alarm ────────────────────────────────────────────────────────────

struct AlarmData {
    handler: Option<alloc::boxed::Box<dyn FnOnce() + Send>>,
}

/// One-shot alarm that fires a closure after a delay.
///
/// The alarm is automatically cancelled on drop if it hasn't fired yet.
/// The callback runs in interrupt context — keep it brief.
pub struct Alarm {
    id: SceUid,
    data: *mut AlarmData,
}

// Alarm is Send because it only holds an SceUid and a pointer whose
// ownership is transferred. The closure itself is Send.
unsafe impl Send for Alarm {}

impl Alarm {
    /// Schedule `f` to run after `delay_us` microseconds.
    ///
    /// The closure runs in interrupt context and must complete quickly.
    pub fn after_micros<F: FnOnce() + Send + 'static>(
        delay_us: u32,
        f: F,
    ) -> Result<Self, TimerError> {
        let data = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(AlarmData {
            handler: Some(alloc::boxed::Box::new(f)),
        }));

        let id = unsafe {
            crate::sys::sceKernelSetAlarm(delay_us, alarm_trampoline, data as *mut c_void)
        };

        if id.0 < 0 {
            // Failed — reclaim the data.
            unsafe {
                let _ = alloc::boxed::Box::from_raw(data);
            }
            return Err(TimerError(id.0));
        }

        Ok(Alarm { id, data })
    }

    /// Cancel the alarm explicitly.
    ///
    /// Returns `Ok(())` if cancelled before firing, or `Err` if
    /// the alarm already fired or another error occurred.
    pub fn cancel(self) -> Result<(), TimerError> {
        let ret = unsafe { crate::sys::sceKernelCancelAlarm(self.id) };
        if ret == 0 {
            // Successfully cancelled — free the data.
            unsafe {
                let _ = alloc::boxed::Box::from_raw(self.data);
            }
        }
        // Prevent Drop from double-cancelling.
        core::mem::forget(self);
        if ret < 0 {
            Err(TimerError(ret))
        } else {
            Ok(())
        }
    }
}

impl Drop for Alarm {
    fn drop(&mut self) {
        let ret = unsafe { crate::sys::sceKernelCancelAlarm(self.id) };
        if ret == 0 {
            // Successfully cancelled — the trampoline never ran, so we own the data.
            unsafe {
                let _ = alloc::boxed::Box::from_raw(self.data);
            }
        }
        // If ret != 0, the alarm already fired and the trampoline consumed the data.
    }
}

unsafe extern "C" fn alarm_trampoline(common: *mut c_void) -> u32 {
    let data = unsafe { &mut *(common as *mut AlarmData) };
    if let Some(f) = data.handler.take() {
        f();
    }
    // Free the AlarmData.
    unsafe {
        let _ = alloc::boxed::Box::from_raw(common as *mut AlarmData);
    }
    0 // Don't reschedule.
}

// ── VTimer ───────────────────────────────────────────────────────────

/// Virtual timer with RAII cleanup.
///
/// The timer is deleted on drop. Any registered handler is cancelled first.
pub struct VTimer {
    id: SceUid,
}

impl VTimer {
    /// Create a new virtual timer.
    ///
    /// `name` must be a null-terminated byte string.
    pub fn new(name: &[u8]) -> Result<Self, TimerError> {
        let id = unsafe { crate::sys::sceKernelCreateVTimer(name.as_ptr(), core::ptr::null_mut()) };
        if id.0 < 0 {
            Err(TimerError(id.0))
        } else {
            Ok(Self { id })
        }
    }

    /// Start the timer.
    pub fn start(&self) -> Result<(), TimerError> {
        let ret = unsafe { crate::sys::sceKernelStartVTimer(self.id) };
        if ret < 0 {
            Err(TimerError(ret))
        } else {
            Ok(())
        }
    }

    /// Stop the timer.
    pub fn stop(&self) -> Result<(), TimerError> {
        let ret = unsafe { crate::sys::sceKernelStopVTimer(self.id) };
        if ret < 0 {
            Err(TimerError(ret))
        } else {
            Ok(())
        }
    }

    /// Set a wide (64-bit) timer handler.
    ///
    /// The handler runs in interrupt context. Return non-zero to reschedule,
    /// 0 to stop.
    ///
    /// # Safety
    ///
    /// `handler` must be a valid function pointer. `common` must remain valid
    /// for the lifetime of the handler registration.
    pub unsafe fn set_handler_wide(
        &self,
        delay_us: i64,
        handler: SceKernelVTimerHandlerWide,
        common: *mut c_void,
    ) -> Result<(), TimerError> {
        let ret = unsafe {
            crate::sys::sceKernelSetVTimerHandlerWide(self.id, delay_us, handler, common)
        };
        if ret < 0 {
            Err(TimerError(ret))
        } else {
            Ok(())
        }
    }

    /// Cancel the current handler.
    pub fn cancel_handler(&self) -> Result<(), TimerError> {
        let ret = unsafe { crate::sys::sceKernelCancelVTimerHandler(self.id) };
        if ret < 0 {
            Err(TimerError(ret))
        } else {
            Ok(())
        }
    }

    /// Get the current timer time in microseconds.
    pub fn time_us(&self) -> i64 {
        unsafe { crate::sys::sceKernelGetVTimerTimeWide(self.id) }
    }
}

impl Drop for VTimer {
    fn drop(&mut self) {
        unsafe {
            let _ = crate::sys::sceKernelCancelVTimerHandler(self.id);
            let _ = crate::sys::sceKernelStopVTimer(self.id);
            let _ = crate::sys::sceKernelDeleteVTimer(self.id);
        }
    }
}
