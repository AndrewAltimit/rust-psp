#![forbid(unsafe_op_in_unsafe_fn)]

use crate::io;

unsafe extern "C" {
    fn __psp_stdout() -> i32;
    fn __psp_stderr() -> i32;
    fn __psp_stdin() -> i32;
    fn __psp_io_write(fd: i32, data: *const u8, size: u32) -> i32;
    fn __psp_io_read(fd: i32, data: *mut u8, size: u32) -> i32;
}

pub struct Stdin;
pub struct Stdout;
pub struct Stderr;

impl Stdin {
    pub const fn new() -> Stdin {
        Stdin
    }
}

impl io::Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let fd = unsafe { __psp_stdin() };
        if fd < 0 {
            return Ok(0);
        }
        let ret = unsafe { __psp_io_read(fd, buf.as_mut_ptr(), buf.len() as u32) };
        if ret < 0 {
            Err(io::Error::from_raw_os_error(-ret))
        } else {
            Ok(ret as usize)
        }
    }
}

impl Stdout {
    pub const fn new() -> Stdout {
        Stdout
    }
}

impl io::Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let fd = unsafe { __psp_stdout() };
        if fd < 0 {
            // No stdout available -- silently discard
            return Ok(buf.len());
        }
        let ret = unsafe { __psp_io_write(fd, buf.as_ptr(), buf.len() as u32) };
        if ret < 0 {
            Err(io::Error::from_raw_os_error(-ret))
        } else {
            Ok(ret as usize)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Stderr {
    pub const fn new() -> Stderr {
        Stderr
    }
}

impl io::Write for Stderr {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let fd = unsafe { __psp_stderr() };
        if fd < 0 {
            // No stderr available -- silently discard
            return Ok(buf.len());
        }
        let ret = unsafe { __psp_io_write(fd, buf.as_ptr(), buf.len() as u32) };
        if ret < 0 {
            Err(io::Error::from_raw_os_error(-ret))
        } else {
            Ok(ret as usize)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub const STDIN_BUF_SIZE: usize = 0;

pub fn is_ebadf(_err: &io::Error) -> bool {
    true
}

pub fn panic_output() -> Option<impl io::Write> {
    Some(Stderr::new())
}
