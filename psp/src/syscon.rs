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
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

/// Stores a resolved function pointer as an `AtomicUsize` (0 = not resolved).
struct AtomicFnPtr(AtomicUsize);

impl AtomicFnPtr {
    const fn new() -> Self {
        Self(AtomicUsize::new(0))
    }

    fn store(&self, addr: *mut u8) {
        self.0.store(addr as usize, Ordering::Release);
    }

    fn load(&self) -> Option<usize> {
        let v = self.0.load(Ordering::Acquire);
        if v == 0 { None } else { Some(v) }
    }
}

// SAFETY: Function pointers are resolved once in init() and then only read.
unsafe impl Sync for AtomicFnPtr {}

static BARYON_VERSION: AtomicFnPtr = AtomicFnPtr::new();
static GET_POWER_STATUS: AtomicFnPtr = AtomicFnPtr::new();
static GET_BATTERY_REMAIN: AtomicFnPtr = AtomicFnPtr::new();
static GET_BATTERY_VOLT: AtomicFnPtr = AtomicFnPtr::new();
static GET_BATTERY_TEMP: AtomicFnPtr = AtomicFnPtr::new();
static IS_AC_SUPPLIED: AtomicFnPtr = AtomicFnPtr::new();
static COMMON_WRITE: AtomicFnPtr = AtomicFnPtr::new();
static COMMON_READ: AtomicFnPtr = AtomicFnPtr::new();
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

    macro_rules! try_resolve {
        ($nid:expr, $slot:expr) => {
            if let Some(addr) = unsafe { resolve($nid) } {
                $slot.store(addr);
                count += 1;
            }
        };
    }

    try_resolve!(nids::NID_SYSCON_GET_BARYON_VERSION, BARYON_VERSION);
    try_resolve!(nids::NID_SYSCON_GET_POWER_STATUS, GET_POWER_STATUS);
    try_resolve!(nids::NID_SYSCON_GET_BATTERY_REMAIN, GET_BATTERY_REMAIN);
    try_resolve!(nids::NID_SYSCON_GET_BATTERY_VOLT, GET_BATTERY_VOLT);
    try_resolve!(nids::NID_SYSCON_GET_BATTERY_TEMP, GET_BATTERY_TEMP);
    try_resolve!(nids::NID_SYSCON_IS_AC_SUPPLIED, IS_AC_SUPPLIED);
    try_resolve!(nids::NID_SYSCON_COMMON_WRITE, COMMON_WRITE);
    try_resolve!(nids::NID_SYSCON_COMMON_READ, COMMON_READ);

    INITIALIZED.store(true, Ordering::Release);
    count
}

/// Read the Baryon (Syscon) hardware version.
pub fn baryon_version() -> Option<u32> {
    let f: BaryonVersionFn = unsafe { core::mem::transmute(BARYON_VERSION.load()?) };
    Some(unsafe { f() } as u32)
}

/// Read battery remaining capacity as a percentage (0-100).
pub fn battery_percent() -> Option<Result<i32, SysconError>> {
    let f: GetI32Fn = unsafe { core::mem::transmute(GET_BATTERY_REMAIN.load()?) };
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
    let f: GetI32Fn = unsafe { core::mem::transmute(GET_BATTERY_VOLT.load()?) };
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
    let f: GetI32Fn = unsafe { core::mem::transmute(GET_BATTERY_TEMP.load()?) };
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
    let f: GetI32Fn = unsafe { core::mem::transmute(GET_POWER_STATUS.load()?) };
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
    let f: IsAcFn = unsafe { core::mem::transmute(IS_AC_SUPPLIED.load()?) };
    Some(unsafe { f() } == 1)
}

/// Send a raw Syscon GET command and read the response.
///
/// # Warning
///
/// Command 0x34 causes hard crash. Command 0x45 causes shutdown.
pub fn raw_read(cmd: u8, response: &mut [u8]) -> Option<Result<i32, SysconError>> {
    let f: CommonReadFn = unsafe { core::mem::transmute(COMMON_READ.load()?) };
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
    let f: CommonWriteFn = unsafe { core::mem::transmute(COMMON_WRITE.load()?) };
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
