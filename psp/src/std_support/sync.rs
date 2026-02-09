use crate::sys::{self, EventFlagAttributes, EventFlagWaitTypes, SceKernelLwMutexWork, SceUid};

// LwMutex bridge functions

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_lwmutex_create(
    work: *mut u8,
    name: *const u8,
    attr: u32,
    count: i32,
) -> i32 {
    unsafe {
        sys::sceKernelCreateLwMutex(
            work as *mut SceKernelLwMutexWork,
            name,
            attr,
            count,
            core::ptr::null_mut(),
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_lwmutex_delete(work: *mut u8) -> i32 {
    unsafe { sys::sceKernelDeleteLwMutex(work as *mut SceKernelLwMutexWork) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_lwmutex_lock(work: *mut u8, count: i32, timeout: *mut u32) -> i32 {
    unsafe { sys::sceKernelLockLwMutex(work as *mut SceKernelLwMutexWork, count, timeout) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_lwmutex_unlock(work: *mut u8, count: i32) -> i32 {
    unsafe { sys::sceKernelUnlockLwMutex(work as *mut SceKernelLwMutexWork, count) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_lwmutex_trylock(work: *mut u8, count: i32) -> i32 {
    unsafe { sys::sceKernelTryLockLwMutex(work as *mut SceKernelLwMutexWork, count) }
}

// EventFlag bridge functions

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_evflag_create(name: *const u8, attr: u32, bits: u32) -> i32 {
    unsafe {
        sys::sceKernelCreateEventFlag(
            name,
            EventFlagAttributes::from_bits_truncate(attr),
            bits as i32,
            core::ptr::null_mut(),
        )
    }
    .0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_evflag_delete(id: i32) -> i32 {
    unsafe { sys::sceKernelDeleteEventFlag(SceUid(id)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_evflag_wait(
    id: i32,
    bits: u32,
    wait: i32,
    out_bits: *mut u32,
    timeout: *mut u32,
) -> i32 {
    unsafe {
        sys::sceKernelWaitEventFlag(
            SceUid(id),
            bits,
            EventFlagWaitTypes::from_bits_truncate(wait as u32),
            out_bits,
            timeout,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_evflag_set(id: i32, bits: u32) -> i32 {
    unsafe { sys::sceKernelSetEventFlag(SceUid(id), bits) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_evflag_clear(id: i32, bits: u32) -> i32 {
    unsafe { sys::sceKernelClearEventFlag(SceUid(id), bits) }
}
