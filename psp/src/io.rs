//! File I/O abstractions for the PSP.
//!
//! Wraps the raw `sceIo*` syscalls with RAII file handles, directory
//! iterators, and convenience functions for common operations.
//!
//! # Example
//!
//! ```ignore
//! use psp::io::{File, IoOpenFlags};
//!
//! // Write a file
//! let mut f = File::create("ms0:/data/save.bin").unwrap();
//! f.write(b"hello").unwrap();
//!
//! // Read a file
//! let mut f = File::open("ms0:/data/save.bin", IoOpenFlags::RD_ONLY).unwrap();
//! let mut buf = [0u8; 64];
//! let n = f.read(&mut buf).unwrap();
//! ```

use crate::sys::{
    IoOpenFlags, IoWhence, SceIoDirent, SceIoStat, SceUid, sceIoClose, sceIoDclose, sceIoDopen,
    sceIoDread, sceIoGetstat, sceIoLseek, sceIoMkdir, sceIoOpen, sceIoRead, sceIoRemove,
    sceIoRename, sceIoRmdir, sceIoWrite,
};
use core::ffi::c_void;
use core::marker::PhantomData;

// ── IoError ─────────────────────────────────────────────────────────

/// Error from a PSP I/O operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IoError(pub i32);

impl IoError {
    /// The raw SCE error code.
    pub fn code(self) -> i32 {
        self.0
    }
}

impl core::fmt::Debug for IoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "IoError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for IoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "I/O error {:#010x}", self.0 as u32)
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Maximum path length (including null terminator) that fits on the stack.
const MAX_PATH: usize = 256;

/// Copy a `&str` into a stack buffer with a null terminator.
///
/// Returns `Err` if the path is too long.
fn path_to_cstr(path: &str, buf: &mut [u8; MAX_PATH]) -> Result<(), IoError> {
    let bytes = path.as_bytes();
    if bytes.len() >= MAX_PATH {
        // SCE_KERNEL_ERROR_NAMETOOLONG = 0x8001005B
        return Err(IoError(0x8001_005Bu32 as i32));
    }
    buf[..bytes.len()].copy_from_slice(bytes);
    buf[bytes.len()] = 0;
    Ok(())
}

// ── File ────────────────────────────────────────────────────────────

/// An open file descriptor with RAII cleanup.
///
/// The file is automatically closed when this value is dropped.
/// `File` is `!Send + !Sync` because PSP file descriptors are not thread-safe.
pub struct File {
    fd: SceUid,
    // Make File !Send + !Sync (raw pointers are neither).
    _marker: PhantomData<*const ()>,
}

impl File {
    /// Open a file with the given flags.
    ///
    /// `path` is a PSP path, e.g. `"ms0:/data/file.txt"`.
    pub fn open(path: &str, flags: IoOpenFlags) -> Result<Self, IoError> {
        let mut buf = [0u8; MAX_PATH];
        path_to_cstr(path, &mut buf)?;
        let fd = unsafe { sceIoOpen(buf.as_ptr(), flags, 0o777) };
        if fd.0 < 0 {
            Err(IoError(fd.0))
        } else {
            Ok(Self {
                fd,
                _marker: PhantomData,
            })
        }
    }

    /// Create a file for writing (create + truncate + write-only).
    pub fn create(path: &str) -> Result<Self, IoError> {
        Self::open(
            path,
            IoOpenFlags::WR_ONLY | IoOpenFlags::CREAT | IoOpenFlags::TRUNC,
        )
    }

    /// Read bytes into `buf`. Returns the number of bytes read.
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, IoError> {
        let ret = unsafe { sceIoRead(self.fd, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) };
        if ret < 0 {
            Err(IoError(ret))
        } else {
            Ok(ret as usize)
        }
    }

    /// Write bytes from `buf`. Returns the number of bytes written.
    pub fn write(&self, buf: &[u8]) -> Result<usize, IoError> {
        let ret = unsafe { sceIoWrite(self.fd, buf.as_ptr() as *const c_void, buf.len()) };
        if ret < 0 {
            Err(IoError(ret))
        } else {
            Ok(ret as usize)
        }
    }

    /// Read until `buf` is full or EOF is reached.
    ///
    /// Returns the total number of bytes read.
    pub fn read_all(&self, buf: &mut [u8]) -> Result<usize, IoError> {
        let mut total = 0;
        while total < buf.len() {
            let n = self.read(&mut buf[total..])?;
            if n == 0 {
                break; // EOF
            }
            total += n;
        }
        Ok(total)
    }

    /// Seek to a position in the file.
    ///
    /// Returns the new absolute position.
    pub fn seek(&self, offset: i64, whence: IoWhence) -> Result<i64, IoError> {
        let pos = unsafe { sceIoLseek(self.fd, offset, whence) };
        if pos < 0 {
            Err(IoError(pos as i32))
        } else {
            Ok(pos)
        }
    }

    /// Get the size of the file in bytes.
    pub fn size(&self) -> Result<i64, IoError> {
        let old = self.seek(0, IoWhence::Cur)?;
        let end = self.seek(0, IoWhence::End)?;
        self.seek(old, IoWhence::Set)?;
        Ok(end)
    }

    /// Get the underlying file descriptor.
    pub fn fd(&self) -> SceUid {
        self.fd
    }
}

impl Drop for File {
    fn drop(&mut self) {
        unsafe {
            sceIoClose(self.fd);
        }
    }
}

// ── ReadDir ─────────────────────────────────────────────────────────

/// A directory entry returned by [`ReadDir`].
pub struct DirEntry {
    /// The raw directory entry from the PSP OS.
    pub dirent: SceIoDirent,
}

impl DirEntry {
    /// File name as a byte slice (null-terminated in the raw struct).
    pub fn name(&self) -> &[u8] {
        let name = &self.dirent.d_name;
        let len = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        &name[..len]
    }

    /// File status.
    pub fn stat(&self) -> &SceIoStat {
        &self.dirent.d_stat
    }

    /// Returns `true` if this entry is a directory.
    pub fn is_dir(&self) -> bool {
        use crate::sys::IoStatMode;
        self.dirent.d_stat.st_mode.contains(IoStatMode::IFDIR)
    }

    /// Returns `true` if this entry is a regular file.
    pub fn is_file(&self) -> bool {
        use crate::sys::IoStatMode;
        self.dirent.d_stat.st_mode.contains(IoStatMode::IFREG)
    }
}

/// An iterator over directory entries.
///
/// Created by [`read_dir()`]. Automatically closes the directory
/// handle on drop.
pub struct ReadDir {
    fd: SceUid,
    done: bool,
    _marker: PhantomData<*const ()>,
}

impl Iterator for ReadDir {
    type Item = Result<DirEntry, IoError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        // SAFETY: SceIoDirent is repr(C) and zeroed is a valid initial state.
        let mut dirent: SceIoDirent = unsafe { core::mem::zeroed() };
        let ret = unsafe { sceIoDread(self.fd, &mut dirent) };

        if ret < 0 {
            self.done = true;
            Some(Err(IoError(ret)))
        } else if ret == 0 {
            self.done = true;
            None
        } else {
            Some(Ok(DirEntry { dirent }))
        }
    }
}

impl Drop for ReadDir {
    fn drop(&mut self) {
        unsafe {
            sceIoDclose(self.fd);
        }
    }
}

/// Open a directory for iteration.
///
/// # Example
///
/// ```ignore
/// for entry in psp::io::read_dir("ms0:/PSP/GAME").unwrap() {
///     let entry = entry.unwrap();
///     psp::dprintln!("{}", core::str::from_utf8(entry.name()).unwrap_or("?"));
/// }
/// ```
pub fn read_dir(path: &str) -> Result<ReadDir, IoError> {
    let mut buf = [0u8; MAX_PATH];
    path_to_cstr(path, &mut buf)?;
    let fd = unsafe { sceIoDopen(buf.as_ptr()) };
    if fd.0 < 0 {
        Err(IoError(fd.0))
    } else {
        Ok(ReadDir {
            fd,
            done: false,
            _marker: PhantomData,
        })
    }
}

// ── Convenience functions ───────────────────────────────────────────

/// Read an entire file into a `Vec<u8>`.
#[cfg(not(feature = "stub-only"))]
pub fn read_to_vec(path: &str) -> Result<alloc::vec::Vec<u8>, IoError> {
    let f = File::open(path, IoOpenFlags::RD_ONLY)?;
    let size = f.size()? as usize;
    let mut data = alloc::vec![0u8; size];
    f.read_all(&mut data)?;
    Ok(data)
}

/// Write bytes to a file (create/truncate).
pub fn write_bytes(path: &str, data: &[u8]) -> Result<(), IoError> {
    let f = File::create(path)?;
    let mut written = 0;
    while written < data.len() {
        let n = f.write(&data[written..])?;
        if n == 0 {
            return Err(IoError(-1));
        }
        written += n;
    }
    Ok(())
}

/// Get file status without opening the file.
pub fn stat(path: &str) -> Result<SceIoStat, IoError> {
    let mut buf = [0u8; MAX_PATH];
    path_to_cstr(path, &mut buf)?;
    let mut st: SceIoStat = unsafe { core::mem::zeroed() };
    let ret = unsafe { sceIoGetstat(buf.as_ptr(), &mut st) };
    if ret < 0 { Err(IoError(ret)) } else { Ok(st) }
}

/// Create a directory.
pub fn create_dir(path: &str) -> Result<(), IoError> {
    let mut buf = [0u8; MAX_PATH];
    path_to_cstr(path, &mut buf)?;
    let ret = unsafe { sceIoMkdir(buf.as_ptr(), 0o777) };
    if ret < 0 { Err(IoError(ret)) } else { Ok(()) }
}

/// Remove a file.
pub fn remove_file(path: &str) -> Result<(), IoError> {
    let mut buf = [0u8; MAX_PATH];
    path_to_cstr(path, &mut buf)?;
    let ret = unsafe { sceIoRemove(buf.as_ptr()) };
    if ret < 0 { Err(IoError(ret)) } else { Ok(()) }
}

/// Remove a directory (must be empty).
pub fn remove_dir(path: &str) -> Result<(), IoError> {
    let mut buf = [0u8; MAX_PATH];
    path_to_cstr(path, &mut buf)?;
    let ret = unsafe { sceIoRmdir(buf.as_ptr()) };
    if ret < 0 { Err(IoError(ret)) } else { Ok(()) }
}

/// Rename a file or directory.
pub fn rename(from: &str, to: &str) -> Result<(), IoError> {
    let mut from_buf = [0u8; MAX_PATH];
    let mut to_buf = [0u8; MAX_PATH];
    path_to_cstr(from, &mut from_buf)?;
    path_to_cstr(to, &mut to_buf)?;
    let ret = unsafe { sceIoRename(from_buf.as_ptr(), to_buf.as_ptr()) };
    if ret < 0 { Err(IoError(ret)) } else { Ok(()) }
}
