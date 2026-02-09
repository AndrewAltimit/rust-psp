//! Extended real-time clock operations for the PSP.
//!
//! Provides tick arithmetic, date validation, RFC 3339 formatting/parsing,
//! and UTC/local time conversion. Builds on the basic types in [`crate::time`].
//!
//! # Example
//!
//! ```ignore
//! use psp::rtc::Tick;
//!
//! let now = Tick::now().unwrap();
//! let later = now.add_seconds(60).unwrap();
//! let dt = later.to_datetime().unwrap();
//! psp::dprintln!("{}-{:02}-{:02}", dt.year(), dt.month(), dt.day());
//! ```

use crate::sys;
use crate::time::DateTime;

/// Error from an RTC operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RtcError(pub i32);

impl core::fmt::Debug for RtcError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RtcError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for RtcError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "rtc error {:#010x}", self.0 as u32)
    }
}

/// A raw RTC tick value (microseconds since epoch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tick(pub u64);

impl Tick {
    /// Get the current tick.
    pub fn now() -> Result<Self, RtcError> {
        let mut tick: u64 = 0;
        let ret = unsafe { sys::sceRtcGetCurrentTick(&mut tick) };
        if ret < 0 {
            Err(RtcError(ret))
        } else {
            Ok(Self(tick))
        }
    }

    /// Add microseconds.
    pub fn add_micros(self, us: u64) -> Result<Self, RtcError> {
        let mut dest: u64 = 0;
        let ret = unsafe { sys::sceRtcTickAddMicroseconds(&mut dest, &self.0, us) };
        if ret < 0 {
            Err(RtcError(ret))
        } else {
            Ok(Self(dest))
        }
    }

    /// Add seconds.
    pub fn add_seconds(self, secs: u64) -> Result<Self, RtcError> {
        let mut dest: u64 = 0;
        let ret = unsafe { sys::sceRtcTickAddSeconds(&mut dest, &self.0, secs) };
        if ret < 0 {
            Err(RtcError(ret))
        } else {
            Ok(Self(dest))
        }
    }

    /// Add minutes.
    pub fn add_minutes(self, mins: u64) -> Result<Self, RtcError> {
        let mut dest: u64 = 0;
        let ret = unsafe { sys::sceRtcTickAddMinutes(&mut dest, &self.0, mins) };
        if ret < 0 {
            Err(RtcError(ret))
        } else {
            Ok(Self(dest))
        }
    }

    /// Add hours.
    pub fn add_hours(self, hours: i32) -> Result<Self, RtcError> {
        let mut dest: u64 = 0;
        let ret = unsafe { sys::sceRtcTickAddHours(&mut dest, &self.0, hours) };
        if ret < 0 {
            Err(RtcError(ret))
        } else {
            Ok(Self(dest))
        }
    }

    /// Add days.
    pub fn add_days(self, days: i32) -> Result<Self, RtcError> {
        let mut dest: u64 = 0;
        let ret = unsafe { sys::sceRtcTickAddDays(&mut dest, &self.0, days) };
        if ret < 0 {
            Err(RtcError(ret))
        } else {
            Ok(Self(dest))
        }
    }

    /// Add weeks.
    pub fn add_weeks(self, weeks: i32) -> Result<Self, RtcError> {
        let mut dest: u64 = 0;
        let ret = unsafe { sys::sceRtcTickAddWeeks(&mut dest, &self.0, weeks) };
        if ret < 0 {
            Err(RtcError(ret))
        } else {
            Ok(Self(dest))
        }
    }

    /// Add months.
    pub fn add_months(self, months: i32) -> Result<Self, RtcError> {
        let mut dest: u64 = 0;
        let ret = unsafe { sys::sceRtcTickAddMonths(&mut dest, &self.0, months) };
        if ret < 0 {
            Err(RtcError(ret))
        } else {
            Ok(Self(dest))
        }
    }

    /// Add years.
    pub fn add_years(self, years: i32) -> Result<Self, RtcError> {
        let mut dest: u64 = 0;
        let ret = unsafe { sys::sceRtcTickAddYears(&mut dest, &self.0, years) };
        if ret < 0 {
            Err(RtcError(ret))
        } else {
            Ok(Self(dest))
        }
    }

    /// Convert this tick to a [`DateTime`].
    pub fn to_datetime(self) -> Result<DateTime, RtcError> {
        let mut dt = sys::ScePspDateTime::default();
        let ret = unsafe { sys::sceRtcSetTick(&mut dt, &self.0) };
        if ret < 0 {
            Err(RtcError(ret))
        } else {
            Ok(DateTime::from_raw(dt))
        }
    }

    /// Compare two ticks. Returns -1, 0, or 1.
    pub fn compare(self, other: Tick) -> i32 {
        unsafe { sys::sceRtcCompareTick(&self.0, &other.0) }
    }
}

/// Convert a [`DateTime`] to a [`Tick`].
pub fn datetime_to_tick(dt: &DateTime) -> Result<Tick, RtcError> {
    let mut tick: u64 = 0;
    let ret = unsafe { sys::sceRtcGetTick(dt.as_raw(), &mut tick) };
    if ret < 0 {
        Err(RtcError(ret))
    } else {
        Ok(Tick(tick))
    }
}

/// Format a tick as an RFC 3339 string.
///
/// Returns a null-terminated string in a 32-byte buffer.
/// `tz_minutes` is the timezone offset from UTC in minutes.
pub fn format_rfc3339(tick: &Tick, tz_minutes: i32) -> Result<[u8; 32], RtcError> {
    let mut buf = [0u8; 32];
    let ret = unsafe { sys::sceRtcFormatRFC3339(buf.as_mut_ptr(), &tick.0, tz_minutes) };
    if ret < 0 { Err(RtcError(ret)) } else { Ok(buf) }
}

/// Format a tick as an RFC 3339 string using local time.
pub fn format_rfc3339_local(tick: &Tick) -> Result<[u8; 32], RtcError> {
    let mut buf = [0u8; 32];
    let ret = unsafe { sys::sceRtcFormatRFC3339LocalTime(buf.as_mut_ptr(), &tick.0) };
    if ret < 0 { Err(RtcError(ret)) } else { Ok(buf) }
}

/// Parse an RFC 3339 date string into a tick.
///
/// `s` must be a null-terminated byte string.
pub fn parse_rfc3339(s: &[u8]) -> Result<Tick, RtcError> {
    let mut tick: u64 = 0;
    let ret = unsafe { sys::sceRtcParseRFC3339(&mut tick, s.as_ptr()) };
    if ret < 0 {
        Err(RtcError(ret))
    } else {
        Ok(Tick(tick))
    }
}

/// Convert a UTC tick to local time.
pub fn to_local(utc_tick: &Tick) -> Result<Tick, RtcError> {
    let mut local: u64 = 0;
    let ret = unsafe { sys::sceRtcConvertUtcToLocalTime(&utc_tick.0, &mut local) };
    if ret < 0 {
        Err(RtcError(ret))
    } else {
        Ok(Tick(local))
    }
}

/// Convert a local-time tick to UTC.
pub fn to_utc(local_tick: &Tick) -> Result<Tick, RtcError> {
    let mut utc: u64 = 0;
    let ret = unsafe { sys::sceRtcConvertLocalTimeToUTC(&local_tick.0, &mut utc) };
    if ret < 0 {
        Err(RtcError(ret))
    } else {
        Ok(Tick(utc))
    }
}

/// Get the number of days in the given month (1-12).
pub fn days_in_month(year: i32, month: i32) -> i32 {
    unsafe { sys::sceRtcGetDaysInMonth(year, month) }
}

/// Get the day of week (0=Monday, 6=Sunday).
pub fn day_of_week(year: i32, month: i32, day: i32) -> i32 {
    unsafe { sys::sceRtcGetDayOfWeek(year, month, day) }
}

/// Check if the given year is a leap year.
pub fn is_leap_year(year: i32) -> bool {
    (unsafe { sys::sceRtcIsLeapYear(year) }) != 0
}

/// Validate a DateTime's fields.
///
/// Returns `Ok(())` if valid, or `Err` with the error code.
pub fn check_valid(dt: &DateTime) -> Result<(), RtcError> {
    let ret = unsafe { sys::sceRtcCheckValid(dt.as_raw()) };
    if ret < 0 { Err(RtcError(ret)) } else { Ok(()) }
}
