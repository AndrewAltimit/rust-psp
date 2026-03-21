//! High-level System Register access for kernel-mode PSP applications.
//!
//! The System Register block at `0xBC100000` controls peripheral clocks,
//! bus gates, and USB configuration. This module provides safe wrappers
//! for enabling/disabling hardware peripherals.
//!
//! # Kernel Mode Required
//!
//! All functions require `feature = "kernel"` and the module must be declared
//! with `psp::module_kernel!()`.
//!
//! # Example
//!
//! ```ignore
//! use psp::sysreg;
//!
//! // Enable GPIO hardware access
//! sysreg::gpio_enable().unwrap();
//!
//! // Enable USB hardware
//! sysreg::usb_enable().unwrap();
//!
//! // Check USB connection
//! if sysreg::usb_is_connected() {
//!     // USB cable detected
//! }
//! ```

/// Error from a SysReg operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SysregError(pub i32);

impl core::fmt::Debug for SysregError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "SysregError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for SysregError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "SysReg error {:#010x}", self.0 as u32)
    }
}

/// Enable GPIO peripheral clock and I/O access.
///
/// Must be called before any GPIO operations.
pub fn gpio_enable() -> Result<(), SysregError> {
    let ret = unsafe { crate::sys::sceSysregGpioClkEnable() };
    if ret < 0 {
        return Err(SysregError(ret));
    }
    let ret = unsafe { crate::sys::sceSysregGpioIoEnable() };
    if ret < 0 { Err(SysregError(ret)) } else { Ok(()) }
}

/// Enable USB peripheral (clock, I/O, and bus clock).
///
/// Must be called before USB operations. Typically called before
/// `psp::usb::start_bus()`.
pub fn usb_enable() -> Result<(), SysregError> {
    let ret = unsafe { crate::sys::sceSysregUsbClkEnable() };
    if ret < 0 {
        return Err(SysregError(ret));
    }
    let ret = unsafe { crate::sys::sceSysregUsbIoEnable() };
    if ret < 0 {
        return Err(SysregError(ret));
    }
    let ret = unsafe { crate::sys::sceSysregUsbBusClkEnable() };
    if ret < 0 { Err(SysregError(ret)) } else { Ok(()) }
}

/// Disable USB peripheral (clock, I/O, and bus clock).
pub fn usb_disable() -> Result<(), SysregError> {
    let ret = unsafe { crate::sys::sceSysregUsbBusClkDisable() };
    if ret < 0 {
        return Err(SysregError(ret));
    }
    let ret = unsafe { crate::sys::sceSysregUsbIoDisable() };
    if ret < 0 {
        return Err(SysregError(ret));
    }
    let ret = unsafe { crate::sys::sceSysregUsbClkDisable() };
    if ret < 0 { Err(SysregError(ret)) } else { Ok(()) }
}

/// Assert USB reset (hold the USB controller in reset).
pub fn usb_reset_enable() -> Result<(), SysregError> {
    let ret = unsafe { crate::sys::sceSysregUsbResetEnable() };
    if ret < 0 { Err(SysregError(ret)) } else { Ok(()) }
}

/// Deassert USB reset (release the USB controller from reset).
pub fn usb_reset_disable() -> Result<(), SysregError> {
    let ret = unsafe { crate::sys::sceSysregUsbResetDisable() };
    if ret < 0 { Err(SysregError(ret)) } else { Ok(()) }
}

/// Check if a USB cable is connected (system register level).
pub fn usb_is_connected() -> bool {
    let ret = unsafe { crate::sys::sceSysregUsbGetConnectStatus() };
    ret == 1
}

/// Query pending USB interrupt status.
///
/// Returns 0 if no interrupts are pending.
pub fn usb_query_interrupt() -> i32 {
    unsafe { crate::sys::sceSysregUsbQueryIntr() }
}

/// Acknowledge (acquire) a pending USB interrupt.
pub fn usb_acquire_interrupt() -> Result<i32, SysregError> {
    let ret = unsafe { crate::sys::sceSysregUsbAcquireIntr() };
    if ret < 0 { Err(SysregError(ret)) } else { Ok(ret) }
}
