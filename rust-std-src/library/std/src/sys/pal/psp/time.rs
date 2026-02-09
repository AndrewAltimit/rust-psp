use crate::time::Duration;

unsafe extern "C" {
    fn __psp_get_system_time_wide() -> i64;
    fn __psp_rtc_get_current_tick(tick: *mut u64) -> i32;
    fn __psp_rtc_get_tick_resolution() -> u32;
}

/// Monotonic clock based on sceKernelGetSystemTimeWide() -- microseconds since boot.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Instant(Duration);

/// Wall clock based on sceRtcGetCurrentTick() -- ticks since PSP epoch (0001-01-01).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct SystemTime(Duration);

/// PSP epoch is 0001-01-01 00:00:00 UTC.
/// Unix epoch is 1970-01-01 00:00:00 UTC.
/// Offset in seconds: 62,135,596,800
/// PSP tick resolution is 1,000,000 ticks/sec (microsecond precision).
const PSP_TO_UNIX_EPOCH_SECS: u64 = 62_135_596_800;

pub const UNIX_EPOCH: SystemTime = SystemTime(Duration::from_secs(PSP_TO_UNIX_EPOCH_SECS));

impl Instant {
    pub fn now() -> Instant {
        let us = unsafe { __psp_get_system_time_wide() } as u64;
        Instant(Duration::from_micros(us))
    }

    pub fn checked_sub_instant(&self, other: &Instant) -> Option<Duration> {
        self.0.checked_sub(other.0)
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<Instant> {
        Some(Instant(self.0.checked_add(*other)?))
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<Instant> {
        Some(Instant(self.0.checked_sub(*other)?))
    }
}

impl SystemTime {
    pub const MAX: SystemTime = SystemTime(Duration::MAX);
    pub const MIN: SystemTime = SystemTime(Duration::ZERO);

    pub fn now() -> SystemTime {
        let mut tick: u64 = 0;
        unsafe { __psp_rtc_get_current_tick(&mut tick) };
        let resolution = unsafe { __psp_rtc_get_tick_resolution() } as u64;

        // Convert ticks to Duration
        if resolution > 0 {
            let secs = tick / resolution;
            let remaining_ticks = tick % resolution;
            let nanos = (remaining_ticks * 1_000_000_000) / resolution;
            SystemTime(Duration::new(secs, nanos as u32))
        } else {
            // Fallback: assume microsecond resolution
            SystemTime(Duration::from_micros(tick))
        }
    }

    pub fn sub_time(&self, other: &SystemTime) -> Result<Duration, Duration> {
        self.0.checked_sub(other.0).ok_or_else(|| other.0 - self.0)
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<SystemTime> {
        Some(SystemTime(self.0.checked_add(*other)?))
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<SystemTime> {
        Some(SystemTime(self.0.checked_sub(*other)?))
    }
}
