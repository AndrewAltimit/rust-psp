//! PSP GPIO driver (sceGpio_driver) — kernel-mode API.
//!
//! **Important:** These are kernel driver functions that must be resolved at
//! runtime via `psp::hook::find_function()`, NOT via `psp_extern!` import
//! stubs. `psp_extern!` generates syscall table stubs that don't work
//! correctly for kernel driver libraries called from kernel-mode modules.
//! The `sceGpioPortSet` NID crashes on pins 29-31 when called via import
//! stubs but works correctly via `find_function()` + direct call.
//!
//! Use the high-level `psp::gpio` module which handles resolution internally.
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
//! # sceGpioSetPortMode vs sceGpioSetPortMode2
//!
//! | Function            | NID        | Modes   | Effect                           |
//! |---------------------|------------|---------|----------------------------------|
//! | `sceGpioSetPortMode`  | 0xFBC85E74 | 0/1     | Direction register (+0x10) only   |
//! | `sceGpioSetPortMode2` | 0x317D9D2C | 0/2     | Direction + Output Enable (+0x24) |
//!
//! # Known Pin Functions (PSP-3001)
//!
//! | Pin | Function          | Notes                              |
//! |-----|-------------------|------------------------------------|
//! | 3   | LCD backlight     | Toggling turns off screen          |
//! | 4   | Critical (crash)  | Unknown function                   |
//! | 6,7 | Unknown (writable)| SetPortMode2 returns 0             |
//! | 19  | USB PHY           | Disrupts USB transceiver           |
//! | 23  | VBUS MOSFET       | Controls 5V USB power output       |
//! | 24  | Critical (crash)  | Unknown function                   |
//! | 25  | Unknown (writable)| SetPortMode2 returns 0             |
//! | 26  | Critical (crash)  | Crashes during SetPortMode         |
//! | 29-31 | Critical (crash)| Crash when driven via PortSet/SetPortMode |
//!
//! # NIDs
//!
//! Resolved from decrypted `usb.prx` on PSP-3001 6.61.

/// NID for `sceGpioSetPortMode` — basic direction control (mode 0=input, 1=output).
pub const NID_GPIO_SET_PORT_MODE: u32 = 0xFBC85E74;

/// NID for `sceGpioSetPortMode2` — full output enable (mode 0=disable, 2=enable).
/// Used by `usb.prx` for VBUS control.
pub const NID_GPIO_SET_PORT_MODE2: u32 = 0x317D9D2C;

/// NID for `sceGpioPortSet` — set output pins (write 1 bits to set).
pub const NID_GPIO_PORT_SET: u32 = 0x310F0CCF;

/// NID for `sceGpioPortClear` — clear output pins (write 1 bits to clear).
pub const NID_GPIO_PORT_CLEAR: u32 = 0x103C3EB2;

/// NID for `sceGpioPortRead` — read port 0 pin state.
pub const NID_GPIO_PORT_READ: u32 = 0x4250D44A;

/// NID for `sceGpioGetCapturePort` — read interrupt/capture status.
pub const NID_GPIO_GET_CAPTURE_PORT: u32 = 0xC6928224;

/// Module name for NID resolution.
pub const GPIO_MODULE: &[u8] = b"sceLowIO_Driver\0";
/// Library name for NID resolution.
pub const GPIO_LIBRARY: &[u8] = b"sceGpio_driver\0";
