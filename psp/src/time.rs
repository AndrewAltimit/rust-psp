//! Time and clock abstractions for the PSP.
//!
//! Provides monotonic timing ([`Instant`], [`Duration`]), wall-clock
//! date/time ([`DateTime`]), and a frame-rate tracker ([`FrameTimer`]).
//!
//! # Example
//!
//! ```ignore
//! use psp::time::{Instant, FrameTimer};
//!
//! let start = Instant::now();
//! // ... do work ...
//! let elapsed = start.elapsed();
//! psp::dprintln!("took {} ms", elapsed.as_millis());
//!
//! let mut timer = FrameTimer::new();
//! loop {
//!     let dt = timer.tick();
//!     // dt is seconds since last frame
//! }
//! ```

/// Error type for time operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeError(pub i32);

impl core::fmt::Display for TimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "TimeError({:#010x})", self.0 as u32)
    }
}

// ── Duration ────────────────────────────────────────────────────────

/// A span of time in microseconds.
///
/// The PSP's tick counter runs at 1 MHz, so microseconds are the native
/// resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Duration {
    micros: u64,
}

impl Duration {
    /// Zero duration.
    pub const ZERO: Self = Self { micros: 0 };

    /// Create a duration from microseconds.
    pub const fn from_micros(us: u64) -> Self {
        Self { micros: us }
    }

    /// Create a duration from milliseconds.
    pub const fn from_millis(ms: u64) -> Self {
        Self { micros: ms * 1000 }
    }

    /// Create a duration from whole seconds.
    pub const fn from_secs(s: u64) -> Self {
        Self {
            micros: s * 1_000_000,
        }
    }

    /// Return the total number of microseconds.
    pub const fn as_micros(&self) -> u64 {
        self.micros
    }

    /// Return the total number of whole milliseconds.
    pub const fn as_millis(&self) -> u64 {
        self.micros / 1000
    }

    /// Return the duration as fractional seconds.
    pub fn as_secs_f32(&self) -> f32 {
        self.micros as f32 / 1_000_000.0
    }
}

// ── Instant ─────────────────────────────────────────────────────────

/// A monotonic timestamp from the PSP's tick counter.
///
/// Created via [`Instant::now()`]. Useful for measuring elapsed time
/// without wall-clock concerns (daylight saving, NTP adjustments, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant {
    tick: u64,
}

impl Instant {
    /// Capture the current tick counter.
    pub fn now() -> Self {
        let mut tick: u64 = 0;
        unsafe {
            crate::sys::sceRtcGetCurrentTick(&mut tick);
        }
        Self { tick }
    }

    /// Time elapsed since this instant was captured.
    pub fn elapsed(&self) -> Duration {
        let now = Self::now();
        self.duration_to(now)
    }

    /// Duration from `self` to a later instant.
    ///
    /// If `later` is actually earlier (e.g. due to wrapping), returns
    /// `Duration::ZERO`.
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        earlier.duration_to(*self)
    }

    /// Raw tick value.
    pub fn as_ticks(&self) -> u64 {
        self.tick
    }

    fn duration_to(self, later: Instant) -> Duration {
        let ticks = later.tick.saturating_sub(self.tick);
        let resolution = unsafe { crate::sys::sceRtcGetTickResolution() } as u64;
        if resolution == 0 {
            return Duration::ZERO;
        }
        // Convert ticks to microseconds:  ticks * 1_000_000 / resolution
        // PSP resolution is typically 1_000_000 (1 MHz), so this is usually
        // a no-op, but we handle other values correctly.
        let micros = ticks * 1_000_000 / resolution;
        Duration::from_micros(micros)
    }
}

// ── DateTime ────────────────────────────────────────────────────────

/// Wall-clock date and time from the PSP's RTC.
#[derive(Debug, Clone, Copy)]
pub struct DateTime {
    inner: crate::sys::ScePspDateTime,
}

impl DateTime {
    /// Get the current local date and time.
    pub fn now() -> Result<Self, TimeError> {
        let mut dt = crate::sys::ScePspDateTime::default();
        let ret = unsafe { crate::sys::sceRtcGetCurrentClockLocalTime(&mut dt) };
        if ret < 0 {
            Err(TimeError(ret))
        } else {
            Ok(Self { inner: dt })
        }
    }

    pub fn year(&self) -> u16 {
        self.inner.year
    }
    pub fn month(&self) -> u16 {
        self.inner.month
    }
    pub fn day(&self) -> u16 {
        self.inner.day
    }
    pub fn hour(&self) -> u16 {
        self.inner.hour
    }
    pub fn minute(&self) -> u16 {
        self.inner.minutes
    }
    pub fn second(&self) -> u16 {
        self.inner.seconds
    }
    pub fn microsecond(&self) -> u32 {
        self.inner.microseconds
    }
}

// ── FrameTimer ──────────────────────────────────────────────────────

/// Tracks frame timing for game loops.
///
/// Call [`tick()`](Self::tick) once per frame to get the delta time in
/// seconds.  [`fps()`](Self::fps) returns the estimated frames per second
/// based on the most recent delta.
///
/// # Example
///
/// ```ignore
/// let mut timer = FrameTimer::new();
/// loop {
///     let dt = timer.tick();
///     update_game(dt);
///     render();
/// }
/// ```
pub struct FrameTimer {
    last: Instant,
    delta: f32,
}

impl FrameTimer {
    /// Create a new `FrameTimer` starting from now.
    pub fn new() -> Self {
        Self {
            last: Instant::now(),
            delta: 1.0 / 60.0, // assume 60 FPS initially
        }
    }

    /// Advance one frame and return the delta time in seconds.
    pub fn tick(&mut self) -> f32 {
        let now = Instant::now();
        self.delta = self.last.duration_to(now).as_secs_f32();
        self.last = now;
        self.delta
    }

    /// Estimated frames per second based on the last delta.
    ///
    /// Returns `f32::INFINITY` if the last delta was zero.
    pub fn fps(&self) -> f32 {
        if self.delta > 0.0 {
            1.0 / self.delta
        } else {
            f32::INFINITY
        }
    }

    /// The delta time from the most recent `tick()` call, in seconds.
    pub fn last_delta(&self) -> f32 {
        self.delta
    }
}
