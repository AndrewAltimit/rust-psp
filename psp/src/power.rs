//! Power and clock management for the PSP.
//!
//! Provides clock speed control, battery monitoring, AC power detection,
//! power event callbacks, and idle-timer control. Wraps `scePower*`
//! syscalls into safe, ergonomic functions.

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

// ── Power event callbacks ────────────────────────────────────────────

/// Register a power event callback.
///
/// Spawns a callback thread that sleeps with callback processing enabled.
/// The `handler` is called when power events occur (suspend, resume, AC
/// state changes, battery level changes, etc.).
///
/// The handler signature matches `sceKernelCreateCallback`'s expected
/// callback: `fn(count: i32, power_info: i32, common: *mut c_void) -> i32`.
/// The `power_info` parameter contains [`crate::sys::PowerInfo`] flags.
///
/// Returns a handle that unregisters the callback on drop.
#[cfg(not(feature = "stub-only"))]
pub fn on_power_event(
    handler: unsafe extern "C" fn(i32, i32, *mut core::ffi::c_void) -> i32,
) -> Result<PowerCallbackHandle, PowerError> {
    use core::ffi::c_void;

    let cbid = unsafe {
        crate::sys::sceKernelCreateCallback(b"power_cb\0".as_ptr(), handler, core::ptr::null_mut())
    };
    if cbid.0 < 0 {
        return Err(PowerError(cbid.0));
    }

    let slot = unsafe { crate::sys::scePowerRegisterCallback(-1, cbid) };
    if slot < 0 {
        return Err(PowerError(slot));
    }

    // Spawn a thread that sleeps with CB processing enabled.
    unsafe extern "C" fn sleep_thread(_args: usize, _argp: *mut c_void) -> i32 {
        unsafe { crate::sys::sceKernelSleepThreadCB() };
        0
    }

    let thid = unsafe {
        crate::sys::sceKernelCreateThread(
            b"power_cb_thread\0".as_ptr(),
            sleep_thread,
            crate::DEFAULT_THREAD_PRIORITY,
            4096,
            crate::sys::ThreadAttributes::empty(),
            core::ptr::null_mut(),
        )
    };
    if thid.0 < 0 {
        unsafe {
            crate::sys::scePowerUnregisterCallback(slot);
            crate::sys::sceKernelDeleteCallback(cbid);
        }
        return Err(PowerError(thid.0));
    }

    let ret = unsafe { crate::sys::sceKernelStartThread(thid, 0, core::ptr::null_mut()) };
    if ret < 0 {
        unsafe {
            crate::sys::scePowerUnregisterCallback(slot);
            crate::sys::sceKernelDeleteThread(thid);
            crate::sys::sceKernelDeleteCallback(cbid);
        }
        return Err(PowerError(ret));
    }

    Ok(PowerCallbackHandle {
        slot,
        cb_id: cbid,
        thread_id: thid,
    })
}

/// RAII handle for a registered power callback.
///
/// Unregisters the callback and terminates the background thread on drop.
#[cfg(not(feature = "stub-only"))]
pub struct PowerCallbackHandle {
    slot: i32,
    cb_id: crate::sys::SceUid,
    thread_id: crate::sys::SceUid,
}

#[cfg(not(feature = "stub-only"))]
impl Drop for PowerCallbackHandle {
    fn drop(&mut self) {
        unsafe {
            crate::sys::scePowerUnregisterCallback(self.slot);
            crate::sys::sceKernelTerminateDeleteThread(self.thread_id);
            crate::sys::sceKernelDeleteCallback(self.cb_id);
        }
    }
}

/// Reset the idle timer to prevent the PSP from auto-sleeping.
///
/// Call this once per frame in your main loop.
pub fn prevent_sleep() {
    unsafe { crate::sys::scePowerTick(crate::sys::PowerTick::All) };
}

/// Reset the display idle timer to prevent the screen from turning off.
pub fn prevent_display_off() {
    unsafe { crate::sys::scePowerTick(crate::sys::PowerTick::Display) };
}
