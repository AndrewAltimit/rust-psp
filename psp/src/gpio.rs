//! High-level GPIO access for kernel-mode PSP applications.
//!
//! Resolves GPIO driver functions at runtime via `psp::hook::find_function()`.
//! Call [`init()`] once before using any other function.
//!
//! # Why runtime resolution?
//!
//! `sceGpio_driver` is a kernel driver library. `psp_extern!` import stubs
//! use the syscall table which doesn't work correctly for kernel driver
//! calls from kernel-mode modules. `sceGpioPortSet` via import stubs crashes
//! on pins 29-31; the same NID via `find_function()` + direct call works.
//!
//! # Example
//!
//! ```ignore
//! use psp::gpio;
//!
//! unsafe { gpio::init(); }
//! let pins = gpio::read_port().unwrap_or(0);
//! ```

use crate::sys::gpio as nids;
use core::sync::atomic::{AtomicBool, Ordering};

/// Error from a GPIO operation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct GpioError(pub i32);

impl core::fmt::Debug for GpioError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "GpioError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for GpioError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "GPIO error {:#010x}", self.0 as u32)
    }
}

/// Known GPIO pin assignments on PSP-3001 (TA-090v2).
pub mod pins {
    /// LCD backlight control. Toggling this pin turns off the screen.
    pub const LCD_BACKLIGHT: u32 = 3;
    /// USB PHY transceiver. Disrupts USB communication if toggled.
    pub const USB_PHY: u32 = 19;
    /// USB VBUS MOSFET gate. Controls 5V power output on the USB port.
    /// Output is silicon-locked on TA-090v2.
    pub const USB_VBUS: u32 = 23;
}

// Function pointer types matching the kernel driver signatures.
type PortReadFn = unsafe extern "C" fn() -> u32;
type PortSetFn = unsafe extern "C" fn(mask: u32) -> i32;
type PortClearFn = unsafe extern "C" fn(mask: u32) -> i32;
type SetPortModeFn = unsafe extern "C" fn(pin: u32, mode: u32) -> i32;
type GetCaptureFn = unsafe extern "C" fn() -> u32;

static mut PORT_READ: Option<PortReadFn> = None;
static mut PORT_SET: Option<PortSetFn> = None;
static mut PORT_CLEAR: Option<PortClearFn> = None;
static mut SET_PORT_MODE: Option<SetPortModeFn> = None;
static mut SET_PORT_MODE2: Option<SetPortModeFn> = None;
static mut GET_CAPTURE: Option<GetCaptureFn> = None;
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Resolve GPIO driver NIDs. Call once before using other functions.
///
/// Returns the number of successfully resolved functions (0-6).
///
/// # Safety
///
/// Must be called from kernel mode.
pub unsafe fn init() -> u32 {
    let m = nids::GPIO_MODULE.as_ptr();
    let l = nids::GPIO_LIBRARY.as_ptr();
    let mut count = 0u32;

    unsafe {
        if let Some(addr) = crate::hook::find_function(m, l, nids::NID_GPIO_PORT_READ) {
            PORT_READ = Some(core::mem::transmute(addr));
            count += 1;
        }
        if let Some(addr) = crate::hook::find_function(m, l, nids::NID_GPIO_PORT_SET) {
            PORT_SET = Some(core::mem::transmute(addr));
            count += 1;
        }
        if let Some(addr) = crate::hook::find_function(m, l, nids::NID_GPIO_PORT_CLEAR) {
            PORT_CLEAR = Some(core::mem::transmute(addr));
            count += 1;
        }
        if let Some(addr) = crate::hook::find_function(m, l, nids::NID_GPIO_SET_PORT_MODE) {
            SET_PORT_MODE = Some(core::mem::transmute(addr));
            count += 1;
        }
        if let Some(addr) = crate::hook::find_function(m, l, nids::NID_GPIO_SET_PORT_MODE2) {
            SET_PORT_MODE2 = Some(core::mem::transmute(addr));
            count += 1;
        }
        if let Some(addr) = crate::hook::find_function(m, l, nids::NID_GPIO_GET_CAPTURE_PORT) {
            GET_CAPTURE = Some(core::mem::transmute(addr));
            count += 1;
        }
    }

    INITIALIZED.store(true, Ordering::Release);
    count
}

/// Read the state of all GPIO port 0 pins.
///
/// Returns `None` if [`init()`] has not been called or NID was not resolved.
pub fn read_port() -> Option<u32> {
    let f = unsafe { PORT_READ }?;
    Some(unsafe { f() })
}

/// Read the state of a single GPIO pin (0-31).
///
/// Returns `None` if `pin >= 32`, [`init()`] has not been called,
/// or the NID was not resolved.
pub fn read_pin(pin: u32) -> Option<bool> {
    if pin >= 32 {
        return None;
    }
    read_port().map(|v| v & (1 << pin) != 0)
}

/// Read the GPIO interrupt/capture status.
pub fn capture_status() -> Option<u32> {
    let f = unsafe { GET_CAPTURE }?;
    Some(unsafe { f() })
}

/// Set basic GPIO pin direction (input/output).
///
/// Uses `sceGpioSetPortMode` (NID 0xFBC85E74). Mode: 0=input, 1=output.
///
/// **Warning:** Actually drives pins. Crashes on pins 29-31+ on TA-090v2.
pub fn set_pin_mode(pin: u32, mode: i32) -> Option<i32> {
    let f = unsafe { SET_PORT_MODE }?;
    Some(unsafe { f(pin, mode as u32) })
}

/// Set full GPIO pin output mode (direction + output enable MUX).
///
/// Uses `sceGpioSetPortMode2` (NID 0x317D9D2C). Mode: 0=disable, 2=enable.
/// Safe for probing — Output Enable register is silicon-locked on TA-090v2.
pub fn set_pin_mode2(pin: u32, mode: i32) -> Option<i32> {
    let f = unsafe { SET_PORT_MODE2 }?;
    Some(unsafe { f(pin, mode as u32) })
}

/// Drive GPIO pins high.
pub fn set_pins(mask: u32) -> Option<i32> {
    let f = unsafe { PORT_SET }?;
    Some(unsafe { f(mask) })
}

/// Drive GPIO pins low.
pub fn clear_pins(mask: u32) -> Option<i32> {
    let f = unsafe { PORT_CLEAR }?;
    Some(unsafe { f(mask) })
}

/// Check if GPIO functions have been initialized.
pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}
