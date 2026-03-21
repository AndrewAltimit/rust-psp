//! PSP System Register driver (sceSysreg_driver) — kernel-mode API.
//!
//! **Important:** These are kernel driver functions that must be resolved at
//! runtime via `psp::hook::find_function()`. See `psp::sysreg` for the
//! high-level API.
//!
//! All 14 USB-related NIDs resolved from decrypted `usb.prx` on PSP-3001 6.61.

/// NID for `sceSysregGpioClkEnable`.
pub const NID_SYSREG_GPIO_CLK_ENABLE: u32 = 0xEC03F6E2;
/// NID for `sceSysregGpioIoEnable`.
pub const NID_SYSREG_GPIO_IO_ENABLE: u32 = 0x72C1CA96;
/// NID for `sceSysregUsbClkEnable`.
pub const NID_SYSREG_USB_CLK_ENABLE: u32 = 0x1561BCD2;
/// NID for `sceSysregUsbClkDisable`.
pub const NID_SYSREG_USB_CLK_DISABLE: u32 = 0x1D233EF9;
/// NID for `sceSysregUsbIoEnable`.
pub const NID_SYSREG_USB_IO_ENABLE: u32 = 0x9306F27B;
/// NID for `sceSysregUsbIoDisable`.
pub const NID_SYSREG_USB_IO_DISABLE: u32 = 0xE2A5D1EE;
/// NID for `sceSysregUsbBusClkEnable`.
pub const NID_SYSREG_USB_BUS_CLK_ENABLE: u32 = 0x9A6E7BB8;
/// NID for `sceSysregUsbBusClkDisable`.
pub const NID_SYSREG_USB_BUS_CLK_DISABLE: u32 = 0xD7AD9705;
/// NID for `sceSysregUsbResetEnable`.
pub const NID_SYSREG_USB_RESET_ENABLE: u32 = 0x84A279A4;
/// NID for `sceSysregUsbResetDisable`.
pub const NID_SYSREG_USB_RESET_DISABLE: u32 = 0x6F3B6D7D;
/// NID for `sceSysregUsbGetConnectStatus`.
pub const NID_SYSREG_USB_GET_CONNECT_STATUS: u32 = 0x87B61303;
/// NID for `sceSysregUsbSetConnectStatus`.
pub const NID_SYSREG_USB_SET_CONNECT_STATUS: u32 = 0x9275DD37;
/// NID for `sceSysregUsbQueryIntr`.
pub const NID_SYSREG_USB_QUERY_INTR: u32 = 0x30C0A141;
/// NID for `sceSysregUsbAcquireIntr`.
pub const NID_SYSREG_USB_ACQUIRE_INTR: u32 = 0x6C0EE043;

/// Module name for NID resolution.
pub const SYSREG_MODULE: &[u8] = b"sceLowIO_Driver\0";
/// Library name for NID resolution.
pub const SYSREG_LIBRARY: &[u8] = b"sceSysreg_driver\0";
