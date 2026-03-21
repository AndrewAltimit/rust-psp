//! High-level System Register access for kernel-mode PSP applications.
//!
//! Resolves sceSysreg driver functions at runtime via
//! `psp::hook::find_function()`. Call [`init()`] once before using other
//! functions.
//!
//! # Example
//!
//! ```ignore
//! use psp::sysreg;
//!
//! unsafe { sysreg::init(); }
//! sysreg::gpio_enable();
//! sysreg::usb_enable();
//! ```

use crate::sys::sysreg as nids;
use core::sync::atomic::{AtomicBool, Ordering};

/// Error from a SysReg operation.
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

type VoidFn = unsafe extern "C" fn() -> i32;
type SetStatusFn = unsafe extern "C" fn(i32) -> i32;

static mut GPIO_CLK_ENABLE: Option<VoidFn> = None;
static mut GPIO_IO_ENABLE: Option<VoidFn> = None;
static mut USB_CLK_ENABLE: Option<VoidFn> = None;
static mut USB_CLK_DISABLE: Option<VoidFn> = None;
static mut USB_IO_ENABLE: Option<VoidFn> = None;
static mut USB_IO_DISABLE: Option<VoidFn> = None;
static mut USB_BUS_CLK_ENABLE: Option<VoidFn> = None;
static mut USB_BUS_CLK_DISABLE: Option<VoidFn> = None;
static mut USB_RESET_ENABLE: Option<VoidFn> = None;
static mut USB_RESET_DISABLE: Option<VoidFn> = None;
static mut USB_GET_CONNECT: Option<VoidFn> = None;
static mut USB_SET_CONNECT: Option<SetStatusFn> = None;
static mut USB_QUERY_INTR: Option<VoidFn> = None;
static mut USB_ACQUIRE_INTR: Option<VoidFn> = None;
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Resolve sceSysreg driver NIDs. Call once before using other functions.
///
/// Returns the number of successfully resolved functions (0-14).
///
/// # Safety
///
/// Must be called from kernel mode.
pub unsafe fn init() -> u32 {
    let m = nids::SYSREG_MODULE.as_ptr();
    let l = nids::SYSREG_LIBRARY.as_ptr();
    let mut count = 0u32;

    macro_rules! try_resolve {
        ($nid:expr, $slot:ident) => {
            if let Some(a) = crate::hook::find_function(m, l, $nid) {
                $slot = Some(core::mem::transmute(a));
                count += 1;
            }
        };
    }

    unsafe {
        try_resolve!(nids::NID_SYSREG_GPIO_CLK_ENABLE, GPIO_CLK_ENABLE);
        try_resolve!(nids::NID_SYSREG_GPIO_IO_ENABLE, GPIO_IO_ENABLE);
        try_resolve!(nids::NID_SYSREG_USB_CLK_ENABLE, USB_CLK_ENABLE);
        try_resolve!(nids::NID_SYSREG_USB_CLK_DISABLE, USB_CLK_DISABLE);
        try_resolve!(nids::NID_SYSREG_USB_IO_ENABLE, USB_IO_ENABLE);
        try_resolve!(nids::NID_SYSREG_USB_IO_DISABLE, USB_IO_DISABLE);
        try_resolve!(nids::NID_SYSREG_USB_BUS_CLK_ENABLE, USB_BUS_CLK_ENABLE);
        try_resolve!(nids::NID_SYSREG_USB_BUS_CLK_DISABLE, USB_BUS_CLK_DISABLE);
        try_resolve!(nids::NID_SYSREG_USB_RESET_ENABLE, USB_RESET_ENABLE);
        try_resolve!(nids::NID_SYSREG_USB_RESET_DISABLE, USB_RESET_DISABLE);
        try_resolve!(nids::NID_SYSREG_USB_GET_CONNECT_STATUS, USB_GET_CONNECT);
        try_resolve!(nids::NID_SYSREG_USB_SET_CONNECT_STATUS, USB_SET_CONNECT);
        try_resolve!(nids::NID_SYSREG_USB_QUERY_INTR, USB_QUERY_INTR);
        try_resolve!(nids::NID_SYSREG_USB_ACQUIRE_INTR, USB_ACQUIRE_INTR);
    }

    INITIALIZED.store(true, Ordering::Release);
    count
}

/// Enable GPIO peripheral clock and I/O access.
pub fn gpio_enable() -> Option<Result<(), SysregError>> {
    let f1 = unsafe { GPIO_CLK_ENABLE }?;
    let f2 = unsafe { GPIO_IO_ENABLE }?;
    let ret = unsafe { f1() };
    if ret < 0 { return Some(Err(SysregError(ret))); }
    let ret = unsafe { f2() };
    Some(if ret < 0 { Err(SysregError(ret)) } else { Ok(()) })
}

/// Enable USB peripheral (clock, I/O, and bus clock).
pub fn usb_enable() -> Option<Result<(), SysregError>> {
    let f1 = unsafe { USB_CLK_ENABLE }?;
    let f2 = unsafe { USB_IO_ENABLE }?;
    let f3 = unsafe { USB_BUS_CLK_ENABLE }?;
    let ret = unsafe { f1() };
    if ret < 0 { return Some(Err(SysregError(ret))); }
    let ret = unsafe { f2() };
    if ret < 0 { return Some(Err(SysregError(ret))); }
    let ret = unsafe { f3() };
    Some(if ret < 0 { Err(SysregError(ret)) } else { Ok(()) })
}

/// Disable USB peripheral.
pub fn usb_disable() -> Option<Result<(), SysregError>> {
    let f1 = unsafe { USB_BUS_CLK_DISABLE }?;
    let f2 = unsafe { USB_IO_DISABLE }?;
    let f3 = unsafe { USB_CLK_DISABLE }?;
    let ret = unsafe { f1() };
    if ret < 0 { return Some(Err(SysregError(ret))); }
    let ret = unsafe { f2() };
    if ret < 0 { return Some(Err(SysregError(ret))); }
    let ret = unsafe { f3() };
    Some(if ret < 0 { Err(SysregError(ret)) } else { Ok(()) })
}

/// Check if a USB cable is connected (system register level).
pub fn usb_is_connected() -> Option<bool> {
    let f = unsafe { USB_GET_CONNECT }?;
    Some(unsafe { f() } == 1)
}

/// Query pending USB interrupt status.
pub fn usb_query_interrupt() -> Option<i32> {
    let f = unsafe { USB_QUERY_INTR }?;
    Some(unsafe { f() })
}

/// Check if SysReg functions have been initialized.
pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}
