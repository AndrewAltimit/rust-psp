//! USB management for the PSP.
//!
//! Provides bus driver control and an RAII handle for USB mass storage mode.
//! When [`UsbStorageMode`] is dropped, the storage driver is deactivated
//! and stopped automatically.

use crate::sys::UsbState;
use core::ffi::c_void;

/// Error from a USB operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct UsbError(pub i32);

impl core::fmt::Debug for UsbError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "UsbError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for UsbError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "USB error {:#010x}", self.0 as u32)
    }
}

/// Memory Stick storage mode product ID.
pub const USB_STOR_PID: u32 = 0x1c8;

/// Start the USB bus driver. Required before any USB mode.
pub fn start_bus() -> Result<(), UsbError> {
    let ret = unsafe {
        crate::sys::sceUsbStart(
            b"USBBusDriver\0".as_ptr(),
            0,
            core::ptr::null_mut::<c_void>(),
        )
    };
    if ret < 0 { Err(UsbError(ret)) } else { Ok(()) }
}

/// Stop the USB bus driver.
pub fn stop_bus() -> Result<(), UsbError> {
    let ret = unsafe {
        crate::sys::sceUsbStop(
            b"USBBusDriver\0".as_ptr(),
            0,
            core::ptr::null_mut::<c_void>(),
        )
    };
    if ret < 0 { Err(UsbError(ret)) } else { Ok(()) }
}

/// Get current USB state flags.
pub fn state() -> UsbState {
    unsafe { crate::sys::sceUsbGetState() }
}

/// Check if a USB cable is physically connected.
pub fn is_connected() -> bool {
    state().contains(UsbState::CONNECTED)
}

/// Check if the USB connection is fully established (host mounted).
pub fn is_established() -> bool {
    state().contains(UsbState::ESTABLISHED)
}

/// RAII handle for USB storage mode.
///
/// When dropped, deactivates USB and stops the storage driver.
pub struct UsbStorageMode {
    _private: (),
}

impl UsbStorageMode {
    /// Enter USB storage mode.
    ///
    /// Starts the USBStor_Driver and activates with PID 0x1c8. The PSP
    /// appears as a mass storage device to the host.
    ///
    /// The USB bus driver must be started first via [`start_bus`].
    pub fn activate() -> Result<Self, UsbError> {
        let ret = unsafe {
            crate::sys::sceUsbStart(
                b"USBStor_Driver\0".as_ptr(),
                0,
                core::ptr::null_mut::<c_void>(),
            )
        };
        if ret < 0 {
            return Err(UsbError(ret));
        }

        let ret = unsafe { crate::sys::sceUsbActivate(USB_STOR_PID) };
        if ret < 0 {
            // Clean up: stop the driver we just started.
            unsafe {
                crate::sys::sceUsbStop(
                    b"USBStor_Driver\0".as_ptr(),
                    0,
                    core::ptr::null_mut::<c_void>(),
                );
            }
            return Err(UsbError(ret));
        }

        Ok(Self { _private: () })
    }

    /// Check if the USB storage is mounted by the host.
    pub fn is_mounted(&self) -> bool {
        is_established()
    }
}

impl Drop for UsbStorageMode {
    fn drop(&mut self) {
        unsafe {
            crate::sys::sceUsbDeactivate(USB_STOR_PID);
            crate::sys::sceUsbStop(
                b"USBStor_Driver\0".as_ptr(),
                0,
                core::ptr::null_mut::<c_void>(),
            );
        }
    }
}
