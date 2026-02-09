use crate::sys::{self, IoOpenFlags, IoWhence, SceIoDirent, SceIoStat, SceUid};
use core::ffi::c_void;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_io_open(file: *const u8, flags: i32, mode: i32) -> i32 {
    unsafe { sys::sceIoOpen(file, IoOpenFlags::from_bits_truncate(flags), mode) }.0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_io_close(fd: i32) -> i32 {
    unsafe { sys::sceIoClose(SceUid(fd)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_io_read(fd: i32, data: *mut u8, size: u32) -> i32 {
    unsafe { sys::sceIoRead(SceUid(fd), data as *mut c_void, size) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_io_write(fd: i32, data: *const u8, size: u32) -> i32 {
    unsafe { sys::sceIoWrite(SceUid(fd), data as *const c_void, size as usize) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_io_lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    let w = match whence {
        0 => IoWhence::Set,
        1 => IoWhence::Cur,
        2 => IoWhence::End,
        _ => IoWhence::Set,
    };
    unsafe { sys::sceIoLseek(SceUid(fd), offset, w) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_io_remove(file: *const u8) -> i32 {
    unsafe { sys::sceIoRemove(file) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_io_mkdir(dir: *const u8, mode: i32) -> i32 {
    unsafe { sys::sceIoMkdir(dir, mode) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_io_rmdir(dir: *const u8) -> i32 {
    unsafe { sys::sceIoRmdir(dir) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_io_rename(old: *const u8, new: *const u8) -> i32 {
    unsafe { sys::sceIoRename(old, new) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_io_getstat(file: *const u8, stat: *mut SceIoStat) -> i32 {
    unsafe { sys::sceIoGetstat(file, stat) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_io_dopen(dir: *const u8) -> i32 {
    unsafe { sys::sceIoDopen(dir) }.0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_io_dread(fd: i32, entry: *mut SceIoDirent) -> i32 {
    unsafe { sys::sceIoDread(SceUid(fd), entry) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __psp_io_dclose(fd: i32) -> i32 {
    unsafe { sys::sceIoDclose(SceUid(fd)) }
}
