use crate::sys::{self, SceKernelUtilsMt19937Context};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_mt19937_init(ctx: *mut u8, seed: u32) {
    unsafe { sys::sceKernelUtilsMt19937Init(ctx as *mut SceKernelUtilsMt19937Context, seed) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_mt19937_uint(ctx: *mut u8) -> u32 {
    unsafe { sys::sceKernelUtilsMt19937UInt(ctx as *mut SceKernelUtilsMt19937Context) }
}
