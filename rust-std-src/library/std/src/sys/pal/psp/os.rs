use super::unsupported;
use crate::ffi::{OsStr, OsString};
use crate::marker::PhantomData;
use crate::path::{self, PathBuf};
use crate::{fmt, io};

unsafe extern "C" {
    fn __psp_io_chdir(path: *const u8) -> i32;
    fn __psp_exit_game();
    fn __psp_get_thread_id() -> i32;
}

pub fn getcwd() -> io::Result<PathBuf> {
    // PSP has no getcwd syscall
    unsupported()
}

pub fn chdir(p: &path::Path) -> io::Result<()> {
    let path = p.to_str().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "path contains invalid UTF-8")
    })?;
    // Create null-terminated path on stack
    let mut buf = [0u8; 256];
    let bytes = path.as_bytes();
    if bytes.len() >= buf.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "path too long"));
    }
    buf[..bytes.len()].copy_from_slice(bytes);
    buf[bytes.len()] = 0;

    let ret = unsafe { __psp_io_chdir(buf.as_ptr()) };
    if ret < 0 {
        Err(io::Error::from_raw_os_error(-ret))
    } else {
        Ok(())
    }
}

pub struct SplitPaths<'a>(!, PhantomData<&'a ()>);

pub fn split_paths(_unparsed: &OsStr) -> SplitPaths<'_> {
    panic!("unsupported")
}

impl<'a> Iterator for SplitPaths<'a> {
    type Item = PathBuf;
    fn next(&mut self) -> Option<PathBuf> {
        self.0
    }
}

#[derive(Debug)]
pub struct JoinPathsError;

pub fn join_paths<I, T>(_paths: I) -> Result<OsString, JoinPathsError>
where
    I: Iterator<Item = T>,
    T: AsRef<OsStr>,
{
    Err(JoinPathsError)
}

impl fmt::Display for JoinPathsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "path joining not supported on PSP".fmt(f)
    }
}

impl crate::error::Error for JoinPathsError {}

pub fn current_exe() -> io::Result<PathBuf> {
    unsupported()
}

pub fn temp_dir() -> PathBuf {
    PathBuf::from("ms0:/PSP/TEMP")
}

pub fn home_dir() -> Option<PathBuf> {
    Some(PathBuf::from("ms0:/PSP/GAME"))
}

pub fn exit(code: i32) -> ! {
    let _ = code;
    unsafe { __psp_exit_game() };
    // If exit_game returns somehow, abort
    crate::intrinsics::abort()
}

pub fn getpid() -> u32 {
    unsafe { __psp_get_thread_id() as u32 }
}
