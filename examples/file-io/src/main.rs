#![no_std]
#![no_main]

use core::ffi::c_void;

use psp::sys::{self, IoOpenFlags, IoPermissions, SceUid};

psp::module!("file_io_example", 1, 1);

fn psp_main() {
    psp::enable_home_button();

    let path = b"host0:/test_output.txt\0";
    let message = b"Hello from rust-psp file I/O!";

    // Write a message to a file.
    let fd: SceUid = unsafe {
        sys::sceIoOpen(
            path.as_ptr(),
            IoOpenFlags::WR_ONLY | IoOpenFlags::CREAT | IoOpenFlags::TRUNC,
            0o644 as IoPermissions,
        )
    };

    if fd.0 < 0 {
        psp::dprintln!("Failed to open file for writing: {}", fd.0);
        return;
    }

    let written = unsafe { sys::sceIoWrite(fd, message.as_ptr() as *const c_void, message.len()) };
    psp::dprintln!("Wrote {} bytes", written);
    unsafe { sys::sceIoClose(fd) };

    // Read the file back.
    let fd: SceUid =
        unsafe { sys::sceIoOpen(path.as_ptr(), IoOpenFlags::RD_ONLY, 0 as IoPermissions) };

    if fd.0 < 0 {
        psp::dprintln!("Failed to open file for reading: {}", fd.0);
        return;
    }

    let mut buf = [0u8; 128];
    let read = unsafe { sys::sceIoRead(fd, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) };
    unsafe { sys::sceIoClose(fd) };

    if read > 0 {
        let text = core::str::from_utf8(&buf[..read as usize]).unwrap_or("<invalid utf8>");
        psp::dprintln!("Read back: {}", text);
    } else {
        psp::dprintln!("Failed to read file: {}", read);
    }
}
