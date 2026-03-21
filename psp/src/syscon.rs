//! High-level Syscon (System Controller) access for kernel-mode PSP
//! applications.
//!
//! The Syscon is a secondary microcontroller on the PSP motherboard that
//! manages power, battery, temperature, and other low-level hardware. This
//! module provides safe wrappers for querying hardware state.
//!
//! # Kernel Mode Required
//!
//! All functions require `feature = "kernel"` and the module must be declared
//! with `psp::module_kernel!()`.
//!
//! # Example
//!
//! ```ignore
//! use psp::syscon;
//!
//! let version = syscon::baryon_version();
//! let battery = syscon::battery_percent().unwrap_or(0);
//! let ac = syscon::is_ac_connected();
//! ```

/// Error from a Syscon operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SysconError(pub i32);

impl core::fmt::Debug for SysconError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "SysconError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for SysconError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Syscon error {:#010x}", self.0 as u32)
    }
}

/// Read the Baryon (Syscon) hardware version.
///
/// # Returns
///
/// Version as a 32-bit value (e.g., `0x00040600` for PSP-3001).
pub fn baryon_version() -> u32 {
    let val = unsafe { crate::sys::sceSysconGetBaryonVersion() };
    val as u32
}

/// Read the battery remaining capacity as a percentage (0-100).
pub fn battery_percent() -> Result<i32, SysconError> {
    let mut percent: i32 = 0;
    let ret = unsafe { crate::sys::sceSysconGetBatteryRemain(&mut percent) };
    if ret < 0 { Err(SysconError(ret)) } else { Ok(percent) }
}

/// Read the battery voltage in millivolts.
pub fn battery_voltage() -> Result<i32, SysconError> {
    let mut mv: i32 = 0;
    let ret = unsafe { crate::sys::sceSysconGetBatteryVolt(&mut mv) };
    if ret < 0 { Err(SysconError(ret)) } else { Ok(mv) }
}

/// Read the battery temperature in degrees Celsius.
pub fn battery_temp() -> Result<i32, SysconError> {
    let mut temp: i32 = 0;
    let ret = unsafe { crate::sys::sceSysconGetBatteryTemp(&mut temp) };
    if ret < 0 { Err(SysconError(ret)) } else { Ok(temp) }
}

/// Read the power supply status word.
pub fn power_status() -> Result<i32, SysconError> {
    let mut status: i32 = 0;
    let ret = unsafe { crate::sys::sceSysconGetPowerStatus(&mut status) };
    if ret < 0 { Err(SysconError(ret)) } else { Ok(status) }
}

/// Check if the AC adapter is connected.
pub fn is_ac_connected() -> bool {
    let ret = unsafe { crate::sys::sceSysconIsAcSupplied() };
    ret == 1
}

/// Send a raw Syscon command and read the response.
///
/// This provides low-level access to the Syscon SPI interface for commands
/// not covered by the high-level API.
///
/// # Parameters
///
/// - `cmd`: Syscon command byte
/// - `response`: Output buffer for the response bytes
///
/// # Returns
///
/// Number of response bytes read on success.
///
/// # Warning
///
/// Some commands are dangerous:
/// - `0x34`: Hard crash
/// - `0x45`: Immediate shutdown/reboot
pub fn raw_read(cmd: u8, response: &mut [u8]) -> Result<i32, SysconError> {
    let ret = unsafe {
        crate::sys::sceSysconCommonRead(
            cmd as i32,
            response.as_mut_ptr(),
            response.len() as i32,
        )
    };
    if ret < 0 { Err(SysconError(ret)) } else { Ok(ret) }
}

/// Send a raw Syscon SET command with data.
///
/// # Parameters
///
/// - `cmd`: Syscon command byte (SET commands, e.g., 0x47)
/// - `data`: Command data bytes
///
/// # Warning
///
/// Some commands are dangerous:
/// - `0x34`: Hard crash
/// - `0x45`: Immediate shutdown/reboot
pub fn raw_write(cmd: u8, data: &[u8]) -> Result<(), SysconError> {
    let ret = unsafe {
        crate::sys::sceSysconCommonWrite(
            cmd as i32,
            data.as_ptr(),
            data.len() as i32,
        )
    };
    if ret < 0 { Err(SysconError(ret)) } else { Ok(()) }
}
