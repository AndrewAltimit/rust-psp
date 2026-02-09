use crate::sys::{self, SceUid, ThreadAttributes};
use core::ffi::c_void;

/// PSP thread entry function type.
type PspThreadEntry = unsafe extern "C" fn(args: usize, argp: *mut c_void) -> i32;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_create_thread(
    name: *const u8,
    entry: PspThreadEntry,
    priority: i32,
    stack_size: i32,
    attr: u32,
    opt: *mut u8,
) -> i32 {
    unsafe {
        sys::sceKernelCreateThread(
            name,
            entry,
            priority,
            stack_size,
            ThreadAttributes::from_bits_truncate(attr),
            opt as *mut _,
        )
    }
    .0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_start_thread(id: i32, arg_len: u32, argp: *mut u8) -> i32 {
    unsafe { sys::sceKernelStartThread(SceUid(id), arg_len as usize, argp as *mut c_void) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_wait_thread_end(id: i32, timeout: *mut u32) -> i32 {
    unsafe { sys::sceKernelWaitThreadEnd(SceUid(id), timeout) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_delete_thread(id: i32) -> i32 {
    unsafe { sys::sceKernelDeleteThread(SceUid(id)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_delay_thread(us: u32) -> i32 {
    unsafe { sys::sceKernelDelayThread(us) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __psp_sleep_thread() -> i32 {
    unsafe { sys::sceKernelSleepThread() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_wakeup_thread(id: i32) -> i32 {
    unsafe { sys::sceKernelWakeupThread(SceUid(id)) }
}
