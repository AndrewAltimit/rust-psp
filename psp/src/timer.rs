//! Timer and alarm abstractions for the PSP.
//!
//! Provides one-shot alarms with closure support and virtual timers
//! with RAII cleanup.

use crate::sys::{SceKernelVTimerHandlerWide, SceUid};
use core::ffi::c_void;
use core::sync::atomic::{AtomicU8, Ordering};

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

/// Alarm lifecycle states. Atomically tracks ownership of AlarmData.
const ALARM_PENDING: u8 = 0;
const ALARM_FIRED: u8 = 1;
const ALARM_CANCELLED: u8 = 2;

struct AlarmData {
    state: AtomicU8,
    /// Function pointer + opaque argument for the callback.
    /// Using a function pointer instead of `Box<dyn FnOnce()>` avoids
    /// heap allocation/deallocation in interrupt context.
    handler: Option<AlarmHandler>,
}

struct AlarmHandler {
    /// Calls the closure and frees its memory.
    call: unsafe fn(*mut c_void),
    /// Drops the closure without calling it (for cancellation).
    drop_fn: unsafe fn(*mut c_void),
    /// Raw pointer to the boxed closure.
    arg: *mut c_void,
}

// SAFETY: The *mut c_void in handler is a raw pointer to a Send type
// (the user's closure, boxed and leaked). AlarmData is only accessed
// through atomic state coordination.
unsafe impl Send for AlarmData {}
unsafe impl Sync for AlarmData {}

/// One-shot alarm that fires a callback after a delay.
///
/// The alarm is automatically cancelled on drop if it hasn't fired yet.
/// The callback runs in interrupt context — it must not allocate, sleep,
/// or take locks. Use a function pointer + opaque argument pattern.
///
/// For closures, use [`after_micros`](Self::after_micros) which boxes the
/// closure on creation and frees it outside interrupt context.
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
    /// The closure is boxed at creation time. The interrupt trampoline only
    /// calls the closure and sets a flag — deallocation happens in `Drop`
    /// or `cancel()`, never in interrupt context.
    pub fn after_micros<F: FnOnce() + Send + 'static>(
        delay_us: u32,
        f: F,
    ) -> Result<Self, TimerError> {
        // Box the closure and leak it as a raw pointer.
        let closure_ptr = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(f));

        /// Typed trampoline that calls and frees the closure.
        unsafe fn call_closure<F: FnOnce() + Send + 'static>(arg: *mut c_void) {
            let closure = unsafe { alloc::boxed::Box::from_raw(arg as *mut F) };
            closure();
        }

        /// Drop the closure without calling it.
        unsafe fn drop_closure<F: FnOnce() + Send + 'static>(arg: *mut c_void) {
            let _ = unsafe { alloc::boxed::Box::from_raw(arg as *mut F) };
        }

        let data = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(AlarmData {
            state: AtomicU8::new(ALARM_PENDING),
            handler: Some(AlarmHandler {
                call: call_closure::<F>,
                drop_fn: drop_closure::<F>,
                arg: closure_ptr as *mut c_void,
            }),
        }));

        let id = unsafe {
            crate::sys::sceKernelSetAlarm(delay_us, alarm_trampoline, data as *mut c_void)
        };

        if id.0 < 0 {
            // Failed — reclaim both the AlarmData and the closure.
            unsafe { free_alarm_data(data) };
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

        let data = unsafe { &*self.data };
        // Try to claim ownership via atomic state transition.
        let prev = data.state.compare_exchange(
            ALARM_PENDING,
            ALARM_CANCELLED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        if prev.is_ok() {
            // We won the race — free the data and the closure.
            unsafe { free_alarm_data(self.data) };
        }
        // If prev == FIRED, the trampoline already ran the callback.
        // The trampoline does NOT free AlarmData, so we still free it,
        // but the handler is already None.
        if prev == Err(ALARM_FIRED) {
            unsafe {
                let _ = alloc::boxed::Box::from_raw(self.data);
            }
        }

        // Prevent Drop from running.
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
        let _ = unsafe { crate::sys::sceKernelCancelAlarm(self.id) };

        let data = unsafe { &*self.data };
        let prev = data.state.compare_exchange(
            ALARM_PENDING,
            ALARM_CANCELLED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        if prev.is_ok() {
            // We won — free the data and un-called closure.
            unsafe { free_alarm_data(self.data) };
        } else {
            // Trampoline already fired — handler was consumed, just free AlarmData.
            unsafe {
                let _ = alloc::boxed::Box::from_raw(self.data);
            }
        }
    }
}

/// Free an AlarmData and its closure (if still present).
///
/// # Safety
///
/// `ptr` must be a valid `*mut AlarmData` from `Box::into_raw`.
unsafe fn free_alarm_data(ptr: *mut AlarmData) {
    let mut ad = unsafe { *alloc::boxed::Box::from_raw(ptr) };
    if let Some(handler) = ad.handler.take() {
        // The closure was never called — drop it without calling.
        unsafe { (handler.drop_fn)(handler.arg) };
    }
}

/// Interrupt-context trampoline for alarm callbacks.
///
/// Atomically transitions state to FIRED, then calls the handler.
/// Does NOT deallocate — deallocation happens in Drop/cancel.
unsafe extern "C" fn alarm_trampoline(common: *mut c_void) -> u32 {
    let data = unsafe { &*(common as *mut AlarmData) };

    // Try to claim the handler.
    let prev = data.state.compare_exchange(
        ALARM_PENDING,
        ALARM_FIRED,
        Ordering::AcqRel,
        Ordering::Acquire,
    );

    if prev.is_ok() {
        // We won the race — execute the handler.
        // SAFETY: We're the only accessor after winning the CAS.
        let data_mut = unsafe { &mut *(common as *mut AlarmData) };
        if let Some(handler) = data_mut.handler.take() {
            // call() both invokes and frees the closure.
            unsafe { (handler.call)(handler.arg) };
        }
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
        debug_assert!(name.last() == Some(&0), "name must be null-terminated");
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
