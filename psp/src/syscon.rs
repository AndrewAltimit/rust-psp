//! High-level Syscon (System Controller) access for kernel-mode PSP
//! applications.
//!
//! Resolves Syscon driver functions at runtime via `psp::hook::find_function()`.
//! Call [`init()`] once before using any other function.
//!
//! # Example
//!
//! ```ignore
//! use psp::syscon;
//!
//! unsafe { syscon::init(); }
//! let version = syscon::baryon_version().unwrap_or(0);
//! let battery = syscon::battery_percent().unwrap_or(0);
//! ```

use crate::sys::syscon as nids;
use core::sync::atomic::{AtomicBool, Ordering};

/// Error from a Syscon operation.
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

type BaryonVersionFn = unsafe extern "C" fn() -> i32;
type GetI32Fn = unsafe extern "C" fn(*mut i32) -> i32;
type IsAcFn = unsafe extern "C" fn() -> i32;
type CommonWriteFn = unsafe extern "C" fn(i32, *const u8, i32) -> i32;
type CommonReadFn = unsafe extern "C" fn(i32, *mut u8, i32) -> i32;

static mut BARYON_VERSION: Option<BaryonVersionFn> = None;
static mut GET_POWER_STATUS: Option<GetI32Fn> = None;
static mut GET_BATTERY_REMAIN: Option<GetI32Fn> = None;
static mut GET_BATTERY_VOLT: Option<GetI32Fn> = None;
static mut GET_BATTERY_TEMP: Option<GetI32Fn> = None;
static mut IS_AC_SUPPLIED: Option<IsAcFn> = None;
static mut COMMON_WRITE: Option<CommonWriteFn> = None;
static mut COMMON_READ: Option<CommonReadFn> = None;
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Resolve a NID from Syscon driver, trying multiple module names.
unsafe fn resolve(nid: u32) -> Option<*mut u8> {
    let modules = [nids::SYSCON_MODULE, nids::SYSCON_MODULE_ALT];
    let l = nids::SYSCON_LIBRARY.as_ptr();
    for m in &modules {
        if let Some(addr) = unsafe { crate::hook::find_function(m.as_ptr(), l, nid) } {
            return Some(addr);
        }
    }
    None
}

/// Resolve Syscon driver NIDs. Call once before using other functions.
///
/// Returns the number of successfully resolved functions.
///
/// # Safety
///
/// Must be called from kernel mode.
pub unsafe fn init() -> u32 {
    let mut count = 0u32;

    unsafe {
        if let Some(a) = resolve(nids::NID_SYSCON_GET_BARYON_VERSION) {
            BARYON_VERSION = Some(core::mem::transmute(a));
            count += 1;
        }
        if let Some(a) = resolve(nids::NID_SYSCON_GET_POWER_STATUS) {
            GET_POWER_STATUS = Some(core::mem::transmute(a));
            count += 1;
        }
        if let Some(a) = resolve(nids::NID_SYSCON_GET_BATTERY_REMAIN) {
            GET_BATTERY_REMAIN = Some(core::mem::transmute(a));
            count += 1;
        }
        if let Some(a) = resolve(nids::NID_SYSCON_GET_BATTERY_VOLT) {
            GET_BATTERY_VOLT = Some(core::mem::transmute(a));
            count += 1;
        }
        if let Some(a) = resolve(nids::NID_SYSCON_GET_BATTERY_TEMP) {
            GET_BATTERY_TEMP = Some(core::mem::transmute(a));
            count += 1;
        }
        if let Some(a) = resolve(nids::NID_SYSCON_IS_AC_SUPPLIED) {
            IS_AC_SUPPLIED = Some(core::mem::transmute(a));
            count += 1;
        }
        if let Some(a) = resolve(nids::NID_SYSCON_COMMON_WRITE) {
            COMMON_WRITE = Some(core::mem::transmute(a));
            count += 1;
        }
        if let Some(a) = resolve(nids::NID_SYSCON_COMMON_READ) {
            COMMON_READ = Some(core::mem::transmute(a));
            count += 1;
        }
    }

    INITIALIZED.store(true, Ordering::Release);
    count
}

/// Read the Baryon (Syscon) hardware version.
pub fn baryon_version() -> Option<u32> {
    let f = unsafe { BARYON_VERSION }?;
    Some(unsafe { f() } as u32)
}

/// Read battery remaining capacity as a percentage (0-100).
pub fn battery_percent() -> Option<Result<i32, SysconError>> {
    let f = unsafe { GET_BATTERY_REMAIN }?;
    let mut val: i32 = 0;
    let ret = unsafe { f(&mut val) };
    Some(if ret < 0 {
        Err(SysconError(ret))
    } else {
        Ok(val)
    })
}

/// Read battery voltage in millivolts.
pub fn battery_voltage() -> Option<Result<i32, SysconError>> {
    let f = unsafe { GET_BATTERY_VOLT }?;
    let mut val: i32 = 0;
    let ret = unsafe { f(&mut val) };
    Some(if ret < 0 {
        Err(SysconError(ret))
    } else {
        Ok(val)
    })
}

/// Read battery temperature in degrees Celsius.
pub fn battery_temp() -> Option<Result<i32, SysconError>> {
    let f = unsafe { GET_BATTERY_TEMP }?;
    let mut val: i32 = 0;
    let ret = unsafe { f(&mut val) };
    Some(if ret < 0 {
        Err(SysconError(ret))
    } else {
        Ok(val)
    })
}

/// Read the power supply status word.
pub fn power_status() -> Option<Result<i32, SysconError>> {
    let f = unsafe { GET_POWER_STATUS }?;
    let mut val: i32 = 0;
    let ret = unsafe { f(&mut val) };
    Some(if ret < 0 {
        Err(SysconError(ret))
    } else {
        Ok(val)
    })
}

/// Check if the AC adapter is connected.
pub fn is_ac_connected() -> Option<bool> {
    let f = unsafe { IS_AC_SUPPLIED }?;
    Some(unsafe { f() } == 1)
}

/// Send a raw Syscon GET command and read the response.
///
/// # Warning
///
/// Command 0x34 causes hard crash. Command 0x45 causes shutdown.
pub fn raw_read(cmd: u8, response: &mut [u8]) -> Option<Result<i32, SysconError>> {
    let f = unsafe { COMMON_READ }?;
    let ret = unsafe { f(cmd as i32, response.as_mut_ptr(), response.len() as i32) };
    Some(if ret < 0 {
        Err(SysconError(ret))
    } else {
        Ok(ret)
    })
}

/// Send a raw Syscon SET command with data.
///
/// # Warning
///
/// Command 0x34 causes hard crash. Command 0x45 causes shutdown.
pub fn raw_write(cmd: u8, data: &[u8]) -> Option<Result<(), SysconError>> {
    let f = unsafe { COMMON_WRITE }?;
    let ret = unsafe { f(cmd as i32, data.as_ptr(), data.len() as i32) };
    Some(if ret < 0 {
        Err(SysconError(ret))
    } else {
        Ok(())
    })
}

/// Check if Syscon functions have been initialized.
pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}
