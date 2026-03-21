//! High-level GPIO access for kernel-mode PSP applications.
//!
//! Provides safe wrappers around the PSP's GPIO controller for reading pin
//! states and controlling output pins (where hardware allows).
//!
//! # Hardware Limitations
//!
//! The GPIO Output Enable register is silicon-locked on some hardware
//! revisions (confirmed on TA-090v2/PSP-3001). On these models, only pin
//! reading is reliable — output control via [`set_pin`] and [`clear_pin`]
//! may silently fail because the output MUX never latches.
//!
//! # Kernel Mode Required
//!
//! All functions require `feature = "kernel"` and the module must be declared
//! with `psp::module_kernel!()`.
//!
//! # Example
//!
//! ```ignore
//! use psp::gpio;
//!
//! // Read all GPIO pin states
//! let pins = gpio::read_port();
//! let pin23_high = pins & (1 << 23) != 0;
//!
//! // Check a specific pin
//! if gpio::read_pin(23) {
//!     // USB VBUS MOSFET gate is high
//! }
//! ```

/// Error from a GPIO operation, wrapping the raw SCE error code.
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
    /// Output is silicon-locked on TA-090v2 — reads work, writes don't latch.
    pub const USB_VBUS: u32 = 23;
}

/// Read the state of all GPIO port 0 pins.
///
/// Returns a 32-bit value where each bit represents a pin (1=high, 0=low).
pub fn read_port() -> u32 {
    let val = unsafe { crate::sys::sceGpioPortRead() };
    val as u32
}

/// Read the state of a single GPIO pin.
///
/// # Parameters
///
/// - `pin`: Pin number (0-31)
///
/// # Returns
///
/// `true` if the pin is high, `false` if low.
pub fn read_pin(pin: u32) -> bool {
    read_port() & (1 << pin) != 0
}

/// Read the GPIO interrupt/capture status.
pub fn capture_status() -> u32 {
    let val = unsafe { crate::sys::sceGpioGetCapturePort() };
    val as u32
}

/// Set a GPIO pin mode (enable/disable output).
///
/// # Parameters
///
/// - `pin`: Pin number (0-31)
/// - `mode`: `0` = disable output, `2` = enable output
///
/// # Warning
///
/// On TA-090v2 hardware, the Output Enable register is silicon-locked.
/// This function may return success but the output MUX won't actually
/// latch for locked pins (e.g., pin 23).
pub fn set_pin_mode(pin: u32, mode: i32) -> Result<(), GpioError> {
    let ret = unsafe { crate::sys::sceGpioSetPortMode(pin as i32, mode) };
    if ret < 0 { Err(GpioError(ret)) } else { Ok(()) }
}

/// Drive a GPIO pin high.
///
/// # Parameters
///
/// - `pin`: Pin number (0-31)
///
/// # Note
///
/// Requires the pin to be configured for output via [`set_pin_mode`] and
/// the Output Enable register to be writable (hardware-dependent).
pub fn set_pin(pin: u32) -> Result<(), GpioError> {
    let ret = unsafe { crate::sys::sceGpioPortSet(1i32 << pin) };
    if ret < 0 { Err(GpioError(ret)) } else { Ok(()) }
}

/// Drive a GPIO pin low.
///
/// # Parameters
///
/// - `pin`: Pin number (0-31)
pub fn clear_pin(pin: u32) -> Result<(), GpioError> {
    let ret = unsafe { crate::sys::sceGpioPortClear(1i32 << pin) };
    if ret < 0 { Err(GpioError(ret)) } else { Ok(()) }
}
