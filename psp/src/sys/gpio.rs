//! PSP GPIO driver (sceGpio_driver) — kernel-mode API.
//!
//! The PSP's GPIO controller at `0xBE240000` provides pin-level I/O for
//! controlling hardware peripherals (LCD backlight, USB VBUS MOSFET, etc.).
//!
//! # Register Map (TA-090v2, PSP-3001)
//!
//! | Offset | Register            | Notes                                    |
//! |--------|---------------------|------------------------------------------|
//! | +0x00  | Port 0 Read         | Pin state readback                       |
//! | +0x04  | Port 1 Read         | Pin state readback                       |
//! | +0x08  | Port 1 Set          | Write 1 to set output bits               |
//! | +0x0C  | Port 1 Clear        | Write 1 to clear output bits             |
//! | +0x10  | Port 0 Direction    | 0=input, 1=output                        |
//! | +0x14  | Port 0 Set          | Write 1 to set output bits               |
//! | +0x18  | Port 0 Clear        | Write 1 to clear output bits             |
//! | +0x1C  | Port 1 Direction    |                                          |
//! | +0x20  | Interrupt Status    | Read by `sceGpioGetCapturePort`          |
//! | +0x24  | Output Enable       | **Silicon-locked on TA-090v2**           |
//! | +0x40  | Port 0 AltFunc      | **Silicon-locked on TA-090v2**           |
//! | +0x48  | Port 1 AltFunc      | Polled for busy flag (bits 0-1)          |
//!
//! # Output Path
//!
//! From `sceGpioSetPortMode2` decompilation, enabling GPIO output requires
//! four steps. If any step fails, the output does not latch:
//!
//! ```text
//! 1. BC10007C |= (1 << pin)       // sceSysreg port enable
//! 2. +0x10 Direction |= (1 << pin)  // output mode
//! 3. +0x24 OutputEnable = (1 << pin) // output MUX (LOCKED on some HW)
//! 4. +0x14 Set = (1 << pin)         // drive high
//! ```
//!
//! # Silicon Lock
//!
//! On TA-090v2 (PSP-3001), the Output Enable register (+0x24) and AltFunc
//! register (+0x40) are locked by the Tachyon mask ROM during the earliest
//! boot phase. No kernel-level code can unlock them. This prevents software
//! control of pin 23 (USB VBUS MOSFET) on this hardware revision.
//!
//! # Known Pin Functions (PSP-3001)
//!
//! | Pin | Function          | Notes                              |
//! |-----|-------------------|------------------------------------|
//! | 3   | LCD backlight     | Toggling turns off screen          |
//! | 4   | Critical (crash)  | Unknown function                   |
//! | 19  | USB PHY           | Disrupts USB transceiver           |
//! | 23  | VBUS MOSFET       | Controls 5V USB power output       |
//! | 24  | Critical (crash)  | Unknown function                   |
//! | 26  | Critical (crash)  | Crashes during SetPortMode         |
//!
//! # Kernel Mode Required
//!
//! All functions require `feature = "kernel"` and the module must be declared
//! with `psp::module_kernel!()`.
//!
//! # NIDs
//!
//! Resolved from decrypted `usb.prx` on PSP-3001 6.61. The `sceGpio_driver`
//! library exports these functions for kernel-mode callers.

psp_extern! {
    #![name = "sceGpio_driver"]
    #![flags = 0x4001]
    #![version = (0x00, 0x00)]

    #[psp(0x317D9D2C)]
    /// Set the mode of a GPIO pin.
    ///
    /// # Parameters
    ///
    /// - `pin`: GPIO pin number (0-31)
    /// - `mode`: Pin mode (0 = disable output, 2 = enable output)
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    ///
    /// # Note
    ///
    /// This function writes to the Output Enable register (+0x24). On TA-090v2
    /// hardware, this register is silicon-locked and writes are silently
    /// discarded for most pins.
    pub fn sceGpioSetPortMode(pin: i32, mode: i32) -> i32;

    #[psp(0x310F0CCF)]
    /// Set GPIO output pins (drive high).
    ///
    /// # Parameters
    ///
    /// - `mask`: Bitmask of pins to set (e.g., `1 << 23` for pin 23)
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceGpioPortSet(mask: i32) -> i32;

    #[psp(0x103C3EB2)]
    /// Clear GPIO output pins (drive low).
    ///
    /// # Parameters
    ///
    /// - `mask`: Bitmask of pins to clear (e.g., `1 << 23` for pin 23)
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceGpioPortClear(mask: i32) -> i32;

    #[psp(0x4250D44A)]
    /// Read the current state of GPIO port 0.
    ///
    /// # Return Value
    ///
    /// 32-bit value with each bit representing a pin state (1=high, 0=low).
    pub fn sceGpioPortRead() -> i32;

    #[psp(0xC6928224)]
    /// Read the interrupt/capture status of GPIO pins.
    ///
    /// # Return Value
    ///
    /// 32-bit interrupt status value.
    pub fn sceGpioGetCapturePort() -> i32;
}
