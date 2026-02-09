//! System callback management for the PSP.
//!
//! The most common use is handling the Home button: when the user presses
//! Home, the PSP invokes the registered exit callback. Without one, the
//! Home button does nothing.
//!
//! # Example
//!
//! ```ignore
//! fn psp_main() {
//!     psp::callback::setup_exit_callback().unwrap();
//!     // ... main loop ...
//! }
//! ```

use core::ffi::c_void;
use core::ptr;

use crate::sys::{
    SceUid, ThreadAttributes, sceKernelCreateCallback, sceKernelRegisterExitCallback,
};

/// Error from a callback operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CallbackError(pub i32);

impl core::fmt::Debug for CallbackError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CallbackError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for CallbackError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "callback error {:#010x}", self.0 as u32)
    }
}

/// Set up the standard exit callback.
///
/// Spawns a background thread that sleeps with callback processing
/// enabled. When the Home button is pressed, `sceKernelExitGame()`
/// is called, cleanly exiting the application.
///
/// Call this once at the start of your program. Equivalent to the
/// boilerplate found in most PSPSDK examples.
pub fn setup_exit_callback() -> Result<(), CallbackError> {
    unsafe extern "C" fn exit_callback(_arg1: i32, _arg2: i32, _arg: *mut c_void) -> i32 {
        unsafe { crate::sys::sceKernelExitGame() };
        0
    }

    unsafe extern "C" fn exit_thread(_args: usize, _argp: *mut c_void) -> i32 {
        let cbid = unsafe {
            sceKernelCreateCallback(b"exit_callback\0".as_ptr(), exit_callback, ptr::null_mut())
        };
        if cbid.0 >= 0 {
            unsafe { sceKernelRegisterExitCallback(cbid) };
        }
        unsafe { crate::sys::sceKernelSleepThreadCB() };
        0
    }

    let thid = unsafe {
        crate::sys::sceKernelCreateThread(
            b"exit_thread\0".as_ptr(),
            exit_thread,
            crate::DEFAULT_THREAD_PRIORITY,
            4096,
            ThreadAttributes::empty(),
            ptr::null_mut(),
        )
    };

    if thid.0 < 0 {
        return Err(CallbackError(thid.0));
    }

    let ret = unsafe { crate::sys::sceKernelStartThread(thid, 0, ptr::null_mut()) };
    if ret < 0 {
        unsafe { crate::sys::sceKernelDeleteThread(thid) };
        return Err(CallbackError(ret));
    }

    Ok(())
}

/// Register a custom exit callback function.
///
/// The handler is invoked when the user presses the Home button.
/// Unlike [`setup_exit_callback`], this does **not** spawn a callback
/// thread — you must already have a thread sleeping with
/// `sceKernelSleepThreadCB` for the callback to fire.
///
/// Returns the callback UID on success.
pub fn register_exit_callback(
    handler: unsafe extern "C" fn(i32, i32, *mut c_void) -> i32,
) -> Result<SceUid, CallbackError> {
    let cbid = unsafe { sceKernelCreateCallback(b"exit_cb\0".as_ptr(), handler, ptr::null_mut()) };

    if cbid.0 < 0 {
        return Err(CallbackError(cbid.0));
    }

    let ret = unsafe { sceKernelRegisterExitCallback(cbid) };
    if ret < 0 {
        unsafe { crate::sys::sceKernelDeleteCallback(cbid) };
        return Err(CallbackError(ret));
    }

    Ok(cbid)
}
