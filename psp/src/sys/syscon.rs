//! PSP Syscon driver (sceSyscon_driver) — kernel-mode API.
//!
//! **Important:** These are kernel driver functions that must be resolved at
//! runtime via `psp::hook::find_function()`. See `psp::syscon` for the
//! high-level API.
//!
//! # NID Resolution Caveats
//!
//! `sceSysconCtrlUsbPower` (NID 0xC8D97773) resolves to a getter stub on
//! PSP-3001 6.61 — it reads a cached value but does not control USB power.
//! Use raw Syscon SPI commands for actual control.

/// NID for `sceSysconGetBaryonVersion`.
pub const NID_SYSCON_GET_BARYON_VERSION: u32 = 0xE7E87741;

/// NID for `sceSysconGetPowerStatus`.
pub const NID_SYSCON_GET_POWER_STATUS: u32 = 0x8CBC8B50;

/// NID for `sceSysconGetBatteryRemain`.
pub const NID_SYSCON_GET_BATTERY_REMAIN: u32 = 0x3B657A27;

/// NID for `sceSysconGetBatteryVolt`.
pub const NID_SYSCON_GET_BATTERY_VOLT: u32 = 0x71135D7D;

/// NID for `sceSysconGetBatteryTemp`.
pub const NID_SYSCON_GET_BATTERY_TEMP: u32 = 0x4C539345;

/// NID for `sceSysconIsAcSupplied`.
pub const NID_SYSCON_IS_AC_SUPPLIED: u32 = 0xE0DDFE18;

/// NID for `sceSysconCtrlUsbPower` — **resolves to getter stub on 6.61**.
pub const NID_SYSCON_CTRL_USB_POWER: u32 = 0xC8D97773;

/// NID for `sceSysconCommonWrite` — raw Syscon SPI SET command.
pub const NID_SYSCON_COMMON_WRITE: u32 = 0x7EC5A957;

/// NID for `sceSysconCommonRead` — raw Syscon SPI GET command.
pub const NID_SYSCON_COMMON_READ: u32 = 0x3AC3D2A4;

/// Module name for NID resolution.
pub const SYSCON_MODULE: &[u8] = b"sceSyscon_Driver\0";
/// Library name for NID resolution.
pub const SYSCON_LIBRARY: &[u8] = b"sceSyscon_driver\0";
/// Alternative module name (some firmware versions).
pub const SYSCON_MODULE_ALT: &[u8] = b"sceSYSCON_Driver\0";
