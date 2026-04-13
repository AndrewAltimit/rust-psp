use crate::sys;

/// Return current system time (microseconds since boot) as `i64`.
///
/// **NOTE**: This used to delegate directly to
/// `sceKernelGetSystemTimeWide()`, which *documents* a 64-bit return
/// value via the o32 `$v0:$v1` register pair. On real PSP hardware
/// (Allegrex, firmware 6.61 PRO-C/ARK-4) that syscall hard-crashes
/// the calling thread — observed as an immediate EBOOT watchdog
/// reset the first time `std::time::Instant::now()` is called.
/// PPSSPP does not reproduce the crash because its HLE short-circuits
/// the function. Bug surfaced during oasis-os PSP bring-up,
/// 2026-04-13; no fix found for the Wide variant, so we compose the
/// same value from the 2×32-bit `sceKernelGetSystemTime` out-pointer
/// form which is stable on both hardware and emulator.
#[unsafe(no_mangle)]
pub extern "C" fn __psp_get_system_time_wide() -> i64 {
    let mut clock = sys::SceKernelSysClock { low: 0, hi: 0 };
    let rc = unsafe { sys::sceKernelGetSystemTime(&mut clock) };
    if rc < 0 {
        return 0;
    }
    ((clock.hi as i64) << 32) | (clock.low as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn __psp_rtc_get_current_tick(tick: *mut u64) -> i32 {
    unsafe { sys::sceRtcGetCurrentTick(tick) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __psp_rtc_get_tick_resolution() -> u32 {
    unsafe { sys::sceRtcGetTickResolution() }
}

#[unsafe(no_mangle)]
pub extern "C" fn __psp_get_system_time_low() -> u32 {
    unsafe { sys::sceKernelGetSystemTimeLow() }
}
