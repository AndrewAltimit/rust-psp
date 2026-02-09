use crate::sys;

#[unsafe(no_mangle)]
pub extern "C" fn __psp_abort() -> ! {
    unsafe { sys::sceKernelExitGame() };
    // sceKernelExitGame shouldn't return, but if it does:
    loop {
        core::hint::spin_loop();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __psp_exit_game() {
    unsafe { sys::sceKernelExitGame() };
}

#[unsafe(no_mangle)]
pub extern "C" fn __psp_io_chdir(path: *const u8) -> i32 {
    unsafe { sys::sceIoChdir(path) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __psp_get_thread_id() -> i32 {
    unsafe { sys::sceKernelGetThreadId() }
}
