//! Power and clock management for the PSP.
//!
//! Provides clock speed control, battery monitoring, and AC power
//! detection. Wraps `scePower*` syscalls into safe, ergonomic functions.

/// CPU and bus clock frequencies in MHz.
#[derive(Debug, Clone, Copy)]
pub struct ClockFrequency {
    pub cpu_mhz: i32,
    pub bus_mhz: i32,
}

/// Error from a power operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PowerError(pub i32);

impl core::fmt::Debug for PowerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PowerError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for PowerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "power error {:#010x}", self.0 as u32)
    }
}

/// Battery status information.
#[derive(Debug, Clone, Copy)]
pub struct BatteryInfo {
    /// Whether the battery is currently charging.
    pub is_charging: bool,
    /// Whether a battery is physically present.
    pub is_present: bool,
    /// Whether the battery level is low.
    pub is_low: bool,
    /// Battery charge percentage (0-100), or -1 on error.
    pub percent: i32,
    /// Estimated remaining battery life in minutes, or -1 on error.
    pub lifetime_minutes: i32,
    /// Battery voltage in millivolts.
    pub voltage_mv: i32,
    /// Battery temperature (units depend on PSP firmware).
    pub temperature: i32,
}

/// Get the current CPU and bus clock frequencies.
pub fn get_clock() -> ClockFrequency {
    ClockFrequency {
        cpu_mhz: unsafe { crate::sys::scePowerGetCpuClockFrequency() },
        bus_mhz: unsafe { crate::sys::scePowerGetBusClockFrequency() },
    }
}

/// Set the CPU and bus clock frequencies.
///
/// `cpu_mhz`: 1-333, `bus_mhz`: 1-166.
/// The PLL frequency is set equal to `cpu_mhz`.
///
/// Returns the new clock frequencies on success.
pub fn set_clock(cpu_mhz: i32, bus_mhz: i32) -> Result<ClockFrequency, PowerError> {
    let ret = unsafe { crate::sys::scePowerSetClockFrequency(cpu_mhz, cpu_mhz, bus_mhz) };
    if ret < 0 {
        return Err(PowerError(ret));
    }
    Ok(get_clock())
}

/// Set CPU, bus, and GPU clock frequencies independently.
///
/// `cpu`: 1-333, `bus`: 1-166, `gpu` (PLL): 19-333.
/// Constraints: `cpu <= gpu`, `bus*2 <= gpu`.
pub fn set_clock_frequency(cpu: i32, bus: i32, gpu: i32) -> Result<(), PowerError> {
    let ret = unsafe { crate::sys::scePowerSetClockFrequency(gpu, cpu, bus) };
    if ret < 0 {
        Err(PowerError(ret))
    } else {
        Ok(())
    }
}

/// Query battery status in a single call.
pub fn battery_info() -> BatteryInfo {
    BatteryInfo {
        is_charging: unsafe { crate::sys::scePowerIsBatteryCharging() } == 1,
        is_present: unsafe { crate::sys::scePowerIsBatteryExist() } == 1,
        is_low: unsafe { crate::sys::scePowerIsLowBattery() } == 1,
        percent: unsafe { crate::sys::scePowerGetBatteryLifePercent() },
        lifetime_minutes: unsafe { crate::sys::scePowerGetBatteryLifeTime() },
        voltage_mv: unsafe { crate::sys::scePowerGetBatteryVolt() },
        temperature: unsafe { crate::sys::scePowerGetBatteryTemp() },
    }
}

/// Check if the PSP is running on AC (mains) power.
pub fn is_ac_power() -> bool {
    (unsafe { crate::sys::scePowerIsPowerOnline() }) == 1
}
