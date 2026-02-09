use crate::sys;

#[unsafe(no_mangle)]
pub extern "C" fn __psp_get_system_time_wide() -> i64 {
    unsafe { sys::sceKernelGetSystemTimeWide() }
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
