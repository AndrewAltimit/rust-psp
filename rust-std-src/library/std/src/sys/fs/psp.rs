use crate::ffi::OsString;
use crate::fmt;
use crate::fs::TryLockError;
use crate::hash::{Hash, Hasher};
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut, SeekFrom};
use crate::path::{Path, PathBuf};
pub use crate::sys::fs::common::Dir;
use crate::sys::time::SystemTime;
use crate::sys::unsupported;
use crate::time::Duration;

unsafe extern "C" {
    fn __psp_io_open(file: *const u8, flags: i32, mode: i32) -> i32;
    fn __psp_io_close(fd: i32) -> i32;
    fn __psp_io_read(fd: i32, data: *mut u8, size: u32) -> i32;
    fn __psp_io_write(fd: i32, data: *const u8, size: u32) -> i32;
    fn __psp_io_lseek(fd: i32, offset: i64, whence: i32) -> i64;
    fn __psp_io_remove(file: *const u8) -> i32;
    fn __psp_io_mkdir(dir: *const u8, mode: i32) -> i32;
    fn __psp_io_rmdir(dir: *const u8) -> i32;
    fn __psp_io_rename(old: *const u8, new: *const u8) -> i32;
    fn __psp_io_getstat(file: *const u8, stat: *mut PspStat) -> i32;
    fn __psp_io_dopen(dir: *const u8) -> i32;
    fn __psp_io_dread(fd: i32, entry: *mut PspDirent) -> i32;
    fn __psp_io_dclose(fd: i32) -> i32;
}

// PSP I/O flags (mirrors IoOpenFlags in psp crate)
const PSP_O_RDONLY: i32 = 0x0001;
const PSP_O_WRONLY: i32 = 0x0002;
const PSP_O_RDWR: i32 = 0x0003;
const PSP_O_APPEND: i32 = 0x0100;
const PSP_O_CREAT: i32 = 0x0200;
const PSP_O_TRUNC: i32 = 0x0400;
const PSP_O_EXCL: i32 = 0x0800;

// PSP seek modes
const PSP_SEEK_SET: i32 = 0;
const PSP_SEEK_CUR: i32 = 1;
const PSP_SEEK_END: i32 = 2;

// PSP stat mode bits
const PSP_FIO_S_IFDIR: u32 = 0x1000;
const PSP_FIO_S_IFREG: u32 = 0x2000;

/// Mirror of SceIoStat from PSP OS
#[repr(C)]
#[derive(Clone)]
struct PspStat {
    mode: u32,     // SceMode (permissions + type)
    attr: u32,     // IoStatAttr
    size: i64,     // SceOff
    ctime: PspDateTime,
    atime: PspDateTime,
    mtime: PspDateTime,
    _private: [u32; 6],
}

/// Mirror of ScePspDateTime
#[repr(C)]
#[derive(Clone, Copy)]
struct PspDateTime {
    year: u16,
    month: u16,
    day: u16,
    hour: u16,
    minutes: u16,
    seconds: u16,
    microseconds: u32,
}

/// Mirror of SceIoDirent
#[repr(C)]
struct PspDirent {
    stat: PspStat,
    name: [u8; 256],
    _private: u32,
    _dummy: i32,
}

impl Default for PspStat {
    fn default() -> Self {
        PspStat {
            mode: 0,
            attr: 0,
            size: 0,
            ctime: PspDateTime::default(),
            atime: PspDateTime::default(),
            mtime: PspDateTime::default(),
            _private: [0; 6],
        }
    }
}

impl Default for PspDateTime {
    fn default() -> Self {
        PspDateTime {
            year: 0,
            month: 0,
            day: 0,
            hour: 0,
            minutes: 0,
            seconds: 0,
            microseconds: 0,
        }
    }
}

impl Default for PspDirent {
    fn default() -> Self {
        PspDirent {
            stat: PspStat::default(),
            name: [0; 256],
            _private: 0,
            _dummy: 0,
        }
    }
}

/// Convert a PspDateTime to a SystemTime.
///
/// Computes seconds since the Unix epoch from the datetime fields, then
/// adds that offset to UNIX_EPOCH to get a SystemTime.
fn psp_datetime_to_system_time(dt: &PspDateTime) -> SystemTime {
    use crate::sys::time::UNIX_EPOCH;

    // If the datetime is all zeros, it's unset -- return UNIX_EPOCH as a default.
    if dt.year == 0 {
        return UNIX_EPOCH;
    }

    // Convert to days since Unix epoch (1970-01-01) using calendar arithmetic.
    let y = dt.year as i64;
    let m = dt.month as i64;
    let d = dt.day as i64;

    // Days from 1970-01-01 to the given date.
    // Use the algorithm: shift March-based year, compute total days.
    let (y_adj, m_adj) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let days_from_epoch_0 = {
        // Days from 0000-03-01 to the given date
        let era_days =
            365 * y_adj + y_adj / 4 - y_adj / 100 + y_adj / 400 + (m_adj * 306 + 5) / 10 + d - 1;
        // Subtract days from 0000-03-01 to 1970-01-01 (719468)
        era_days - 719_468
    };

    let secs_from_epoch =
        days_from_epoch_0 * 86400 + dt.hour as i64 * 3600 + dt.minutes as i64 * 60
            + dt.seconds as i64;
    let us = dt.microseconds as u64;

    if secs_from_epoch >= 0 {
        UNIX_EPOCH
            .checked_add_duration(&Duration::new(secs_from_epoch as u64, (us * 1000) as u32))
            .unwrap_or(UNIX_EPOCH)
    } else {
        UNIX_EPOCH
            .checked_sub_duration(&Duration::new((-secs_from_epoch) as u64, 0))
            .unwrap_or(UNIX_EPOCH)
    }
}

/// Convert a Path to a null-terminated byte buffer.
fn path_to_cstr(path: &Path) -> io::Result<[u8; 256]> {
    let s = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path contains invalid UTF-8"))?;
    let bytes = s.as_bytes();
    if bytes.len() >= 256 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "path too long"));
    }
    let mut buf = [0u8; 256];
    buf[..bytes.len()].copy_from_slice(bytes);
    Ok(buf)
}

pub struct File {
    fd: i32,
}

#[derive(Clone)]
pub struct FileAttr {
    stat: PspStat,
}

pub struct ReadDir {
    fd: i32,
    done: bool,
    root: PathBuf,
}

pub struct DirEntry {
    name: OsString,
    root: PathBuf,
    stat: PspStat,
}

#[derive(Clone, Debug)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
    mode: i32,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FileTimes {}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FilePermissions {
    mode: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FileType {
    mode: u32,
}

#[derive(Debug)]
pub struct DirBuilder {
    mode: i32,
}

impl FileAttr {
    pub fn size(&self) -> u64 {
        self.stat.size as u64
    }

    pub fn perm(&self) -> FilePermissions {
        FilePermissions { mode: self.stat.mode }
    }

    pub fn file_type(&self) -> FileType {
        FileType { mode: self.stat.mode }
    }

    pub fn modified(&self) -> io::Result<SystemTime> {
        Ok(psp_datetime_to_system_time(&self.stat.mtime))
    }

    pub fn accessed(&self) -> io::Result<SystemTime> {
        Ok(psp_datetime_to_system_time(&self.stat.atime))
    }

    pub fn created(&self) -> io::Result<SystemTime> {
        Ok(psp_datetime_to_system_time(&self.stat.ctime))
    }
}

impl FilePermissions {
    pub fn readonly(&self) -> bool {
        // Check if write bits are not set
        (self.mode & 0x0002) == 0
    }

    pub fn set_readonly(&mut self, readonly: bool) {
        if readonly {
            self.mode &= !0x0002;
        } else {
            self.mode |= 0x0002;
        }
    }
}

impl FileTimes {
    pub fn set_accessed(&mut self, _t: SystemTime) {}
    pub fn set_modified(&mut self, _t: SystemTime) {}
}

impl FileType {
    pub fn is_dir(&self) -> bool {
        (self.mode & PSP_FIO_S_IFDIR) != 0
    }

    pub fn is_file(&self) -> bool {
        (self.mode & PSP_FIO_S_IFREG) != 0
    }

    pub fn is_symlink(&self) -> bool {
        false // PSP doesn't have symlinks
    }
}

impl fmt::Debug for ReadDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadDir").field("fd", &self.fd).finish()
    }
}

impl Iterator for ReadDir {
    type Item = io::Result<DirEntry>;

    fn next(&mut self) -> Option<io::Result<DirEntry>> {
        if self.done {
            return None;
        }

        let mut entry = PspDirent::default();
        let ret = unsafe { __psp_io_dread(self.fd, &mut entry) };

        if ret <= 0 {
            self.done = true;
            if ret < 0 {
                return Some(Err(io::Error::from_raw_os_error(-ret)));
            }
            return None;
        }

        // Extract name from null-terminated buffer
        let name_len = entry.name.iter().position(|&b| b == 0).unwrap_or(entry.name.len());
        let name = OsString::from(core::str::from_utf8(&entry.name[..name_len]).unwrap_or(""));

        Some(Ok(DirEntry {
            name,
            root: self.root.clone(),
            stat: entry.stat,
        }))
    }
}

impl Drop for ReadDir {
    fn drop(&mut self) {
        if self.fd >= 0 {
            unsafe { __psp_io_dclose(self.fd) };
        }
    }
}

impl DirEntry {
    pub fn path(&self) -> PathBuf {
        self.root.join(&self.name)
    }

    pub fn file_name(&self) -> OsString {
        self.name.clone()
    }

    pub fn metadata(&self) -> io::Result<FileAttr> {
        Ok(FileAttr { stat: self.stat.clone() })
    }

    pub fn file_type(&self) -> io::Result<FileType> {
        Ok(FileType { mode: self.stat.mode })
    }
}

impl OpenOptions {
    pub fn new() -> OpenOptions {
        OpenOptions {
            read: false,
            write: false,
            append: false,
            truncate: false,
            create: false,
            create_new: false,
            mode: 0o777,
        }
    }

    pub fn read(&mut self, read: bool) {
        self.read = read;
    }
    pub fn write(&mut self, write: bool) {
        self.write = write;
    }
    pub fn append(&mut self, append: bool) {
        self.append = append;
    }
    pub fn truncate(&mut self, truncate: bool) {
        self.truncate = truncate;
    }
    pub fn create(&mut self, create: bool) {
        self.create = create;
    }
    pub fn create_new(&mut self, create_new: bool) {
        self.create_new = create_new;
    }
}

impl File {
    pub fn open(path: &Path, opts: &OpenOptions) -> io::Result<File> {
        let buf = path_to_cstr(path)?;

        let mut flags: i32 = 0;
        if opts.read && opts.write {
            flags |= PSP_O_RDWR;
        } else if opts.write {
            flags |= PSP_O_WRONLY;
        } else {
            flags |= PSP_O_RDONLY;
        }
        if opts.append {
            flags |= PSP_O_APPEND;
        }
        if opts.truncate {
            flags |= PSP_O_TRUNC;
        }
        if opts.create {
            flags |= PSP_O_CREAT;
        }
        if opts.create_new {
            flags |= PSP_O_CREAT | PSP_O_EXCL;
        }

        let fd = unsafe { __psp_io_open(buf.as_ptr(), flags, opts.mode) };
        if fd < 0 {
            Err(io::Error::from_raw_os_error(-fd))
        } else {
            Ok(File { fd })
        }
    }

    pub fn file_attr(&self) -> io::Result<FileAttr> {
        // PSP doesn't have fstat -- we can't get attrs from an fd alone
        // Return a minimal default
        unsupported()
    }

    pub fn fsync(&self) -> io::Result<()> {
        // PSP doesn't have fsync -- writes are typically synchronous
        Ok(())
    }

    pub fn datasync(&self) -> io::Result<()> {
        Ok(())
    }

    pub fn lock(&self) -> io::Result<()> {
        unsupported()
    }

    pub fn lock_shared(&self) -> io::Result<()> {
        unsupported()
    }

    pub fn try_lock(&self) -> Result<(), TryLockError> {
        Err(TryLockError::Error(io::Error::UNSUPPORTED_PLATFORM))
    }

    pub fn try_lock_shared(&self) -> Result<(), TryLockError> {
        Err(TryLockError::Error(io::Error::UNSUPPORTED_PLATFORM))
    }

    pub fn unlock(&self) -> io::Result<()> {
        unsupported()
    }

    pub fn truncate(&self, _size: u64) -> io::Result<()> {
        unsupported()
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let ret = unsafe { __psp_io_read(self.fd, buf.as_mut_ptr(), buf.len() as u32) };
        if ret < 0 {
            Err(io::Error::from_raw_os_error(-ret))
        } else {
            Ok(ret as usize)
        }
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        // Read into first non-empty buffer
        for buf in bufs {
            if !buf.is_empty() {
                return self.read(buf);
            }
        }
        Ok(0)
    }

    pub fn is_read_vectored(&self) -> bool {
        false
    }

    pub fn read_buf(&self, mut cursor: BorrowedCursor<'_>) -> io::Result<()> {
        let buf = cursor.ensure_init();
        let n = self.read(buf.init_mut())?;
        unsafe { cursor.advance_unchecked(n) };
        Ok(())
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let ret = unsafe { __psp_io_write(self.fd, buf.as_ptr(), buf.len() as u32) };
        if ret < 0 {
            Err(io::Error::from_raw_os_error(-ret))
        } else {
            Ok(ret as usize)
        }
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        // Write from first non-empty buffer
        for buf in bufs {
            if !buf.is_empty() {
                return self.write(buf);
            }
        }
        Ok(0)
    }

    pub fn is_write_vectored(&self) -> bool {
        false
    }

    pub fn flush(&self) -> io::Result<()> {
        Ok(())
    }

    pub fn seek(&self, pos: SeekFrom) -> io::Result<u64> {
        let (offset, whence) = match pos {
            SeekFrom::Start(n) => (n as i64, PSP_SEEK_SET),
            SeekFrom::Current(n) => (n, PSP_SEEK_CUR),
            SeekFrom::End(n) => (n, PSP_SEEK_END),
        };
        let ret = unsafe { __psp_io_lseek(self.fd, offset, whence) };
        if ret < 0 {
            Err(io::Error::from_raw_os_error(-(ret as i32)))
        } else {
            Ok(ret as u64)
        }
    }

    pub fn size(&self) -> Option<io::Result<u64>> {
        None
    }

    pub fn tell(&self) -> io::Result<u64> {
        self.seek(SeekFrom::Current(0))
    }

    pub fn duplicate(&self) -> io::Result<File> {
        // PSP has no dup/dup2
        unsupported()
    }

    pub fn set_permissions(&self, _perm: FilePermissions) -> io::Result<()> {
        unsupported()
    }

    pub fn set_times(&self, _times: FileTimes) -> io::Result<()> {
        unsupported()
    }
}

impl Drop for File {
    fn drop(&mut self) {
        unsafe { __psp_io_close(self.fd) };
    }
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("File").field("fd", &self.fd).finish()
    }
}

impl DirBuilder {
    pub fn new() -> DirBuilder {
        DirBuilder { mode: 0o777 }
    }

    pub fn mkdir(&self, path: &Path) -> io::Result<()> {
        let buf = path_to_cstr(path)?;
        let ret = unsafe { __psp_io_mkdir(buf.as_ptr(), self.mode) };
        if ret < 0 {
            Err(io::Error::from_raw_os_error(-ret))
        } else {
            Ok(())
        }
    }
}

pub fn readdir(path: &Path) -> io::Result<ReadDir> {
    let buf = path_to_cstr(path)?;
    let fd = unsafe { __psp_io_dopen(buf.as_ptr()) };
    if fd < 0 {
        Err(io::Error::from_raw_os_error(-fd))
    } else {
        Ok(ReadDir {
            fd,
            done: false,
            root: path.to_path_buf(),
        })
    }
}

pub fn unlink(path: &Path) -> io::Result<()> {
    let buf = path_to_cstr(path)?;
    let ret = unsafe { __psp_io_remove(buf.as_ptr()) };
    if ret < 0 {
        Err(io::Error::from_raw_os_error(-ret))
    } else {
        Ok(())
    }
}

pub fn rename(old: &Path, new: &Path) -> io::Result<()> {
    let old_buf = path_to_cstr(old)?;
    let new_buf = path_to_cstr(new)?;
    let ret = unsafe { __psp_io_rename(old_buf.as_ptr(), new_buf.as_ptr()) };
    if ret < 0 {
        Err(io::Error::from_raw_os_error(-ret))
    } else {
        Ok(())
    }
}

pub fn set_perm(_path: &Path, _perm: FilePermissions) -> io::Result<()> {
    unsupported()
}

pub fn set_times(_path: &Path, _times: FileTimes) -> io::Result<()> {
    unsupported()
}

pub fn set_times_nofollow(_path: &Path, _times: FileTimes) -> io::Result<()> {
    unsupported()
}

pub fn rmdir(path: &Path) -> io::Result<()> {
    let buf = path_to_cstr(path)?;
    let ret = unsafe { __psp_io_rmdir(buf.as_ptr()) };
    if ret < 0 {
        Err(io::Error::from_raw_os_error(-ret))
    } else {
        Ok(())
    }
}

pub fn remove_dir_all(path: &Path) -> io::Result<()> {
    // Read directory entries and remove them recursively
    for entry in readdir(path)? {
        let entry = entry?;
        let child_path = entry.path();
        if entry.file_type()?.is_dir() {
            remove_dir_all(&child_path)?;
        } else {
            unlink(&child_path)?;
        }
    }
    rmdir(path)
}

pub fn exists(path: &Path) -> io::Result<bool> {
    match stat(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

pub fn readlink(_path: &Path) -> io::Result<PathBuf> {
    // PSP doesn't have symlinks
    unsupported()
}

pub fn symlink(_original: &Path, _link: &Path) -> io::Result<()> {
    unsupported()
}

pub fn link(_src: &Path, _dst: &Path) -> io::Result<()> {
    unsupported()
}

pub fn stat(path: &Path) -> io::Result<FileAttr> {
    let buf = path_to_cstr(path)?;
    let mut stat = PspStat::default();
    let ret = unsafe { __psp_io_getstat(buf.as_ptr(), &mut stat) };
    if ret < 0 {
        Err(io::Error::from_raw_os_error(-ret))
    } else {
        Ok(FileAttr { stat })
    }
}

pub fn lstat(path: &Path) -> io::Result<FileAttr> {
    // PSP has no symlinks, so lstat == stat
    stat(path)
}

pub fn canonicalize(_path: &Path) -> io::Result<PathBuf> {
    unsupported()
}

pub fn copy(from: &Path, to: &Path) -> io::Result<u64> {
    use crate::fs;

    let mut reader = fs::File::open(from)?;
    let mut writer = fs::File::create(to)?;
    io::copy(&mut reader, &mut writer)
}
