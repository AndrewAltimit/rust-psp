use crate::sys;

#[unsafe(no_mangle)]
pub extern "C" fn __psp_stdin() -> i32 {
    unsafe { sys::sceKernelStdin() }.0
}

#[unsafe(no_mangle)]
pub extern "C" fn __psp_stdout() -> i32 {
    unsafe { sys::sceKernelStdout() }.0
}

#[unsafe(no_mangle)]
pub extern "C" fn __psp_stderr() -> i32 {
    unsafe { sys::sceKernelStderr() }.0
}
