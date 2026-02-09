//! System parameter queries for the PSP.
//!
//! Read system-level settings like language, nickname, date/time format,
//! timezone, and daylight saving status. These are configured by the user
//! in the PSP's System Settings menu.
//!
//! # Example
//!
//! ```ignore
//! use psp::system_param;
//!
//! let lang = system_param::language();
//! let tz = system_param::timezone_offset();
//! psp::dprintln!("Language: {:?}, TZ offset: {} min", lang, tz);
//! ```

use crate::sys::{
    SystemParamDateFormat, SystemParamDaylightSavings, SystemParamId, SystemParamLanguage,
    SystemParamTimeFormat, sceUtilityGetSystemParamInt, sceUtilityGetSystemParamString,
};

/// Error from a system parameter operation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ParamError(pub i32);

impl core::fmt::Debug for ParamError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ParamError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for ParamError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "system param error {:#010x}", self.0 as u32)
    }
}

fn get_int(id: SystemParamId) -> Result<i32, ParamError> {
    let mut value: i32 = 0;
    let ret = unsafe { sceUtilityGetSystemParamInt(id, &mut value) };
    if ret < 0 {
        Err(ParamError(ret))
    } else {
        Ok(value)
    }
}

/// Get the system language setting.
pub fn language() -> Result<SystemParamLanguage, ParamError> {
    let val = get_int(SystemParamId::Language)?;
    // Transmute is safe because SystemParamLanguage covers all valid firmware values.
    Ok(unsafe { core::mem::transmute::<i32, SystemParamLanguage>(val) })
}

/// Get the user's nickname (up to 128 bytes, null-terminated).
pub fn nickname() -> Result<[u8; 128], ParamError> {
    let mut buf = [0u8; 128];
    let ret = unsafe {
        sceUtilityGetSystemParamString(SystemParamId::StringNickname, buf.as_mut_ptr(), 128)
    };
    if ret < 0 {
        Err(ParamError(ret))
    } else {
        Ok(buf)
    }
}

/// Get the date format preference.
pub fn date_format() -> Result<SystemParamDateFormat, ParamError> {
    let val = get_int(SystemParamId::DateFormat)?;
    Ok(unsafe { core::mem::transmute::<i32, SystemParamDateFormat>(val) })
}

/// Get the time format preference (12-hour or 24-hour).
pub fn time_format() -> Result<SystemParamTimeFormat, ParamError> {
    let val = get_int(SystemParamId::TimeFormat)?;
    Ok(unsafe { core::mem::transmute::<i32, SystemParamTimeFormat>(val) })
}

/// Get the timezone offset in minutes from UTC.
pub fn timezone_offset() -> Result<i32, ParamError> {
    get_int(SystemParamId::Timezone)
}

/// Check if daylight saving time is enabled.
pub fn daylight_saving() -> Result<bool, ParamError> {
    let val = get_int(SystemParamId::DaylightSavings)?;
    Ok(val == SystemParamDaylightSavings::Dst as i32)
}
