use crate::io::ErrorKind;

/// PSP error codes are negative values. The kernel returns them as the
/// negative of an internal error code.
pub fn errno() -> i32 {
    // PSP doesn't have a global errno; errors are returned directly from syscalls.
    0
}

pub fn is_interrupted(_code: i32) -> bool {
    false
}

pub fn decode_error_kind(code: i32) -> ErrorKind {
    // PSP error codes (from the SCE error definitions).
    // These are the positive equivalents of common kernel error returns.
    let code = if code < 0 { -code } else { code };

    match code {
        // SCE_KERNEL_ERROR_NOFILE / ENOENT equivalent
        0x80010002 | 2 => ErrorKind::NotFound,
        // SCE_KERNEL_ERROR_ERRNO_EACCES
        0x8001000D | 13 => ErrorKind::PermissionDenied,
        // SCE_KERNEL_ERROR_ERRNO_EEXIST
        0x80010011 | 17 => ErrorKind::AlreadyExists,
        // SCE_KERNEL_ERROR_ERRNO_ENODEV
        0x80010013 | 19 => ErrorKind::NotFound,
        // SCE_KERNEL_ERROR_ERRNO_EINVAL
        0x80010016 | 22 => ErrorKind::InvalidInput,
        // SCE_KERNEL_ERROR_ERRNO_ENOMEM
        0x8001000C | 12 => ErrorKind::OutOfMemory,
        // SCE_KERNEL_ERROR_ERRNO_ENOSPC
        0x8001001C | 28 => ErrorKind::StorageFull,
        _ => ErrorKind::Uncategorized,
    }
}

pub fn error_string(errno: i32) -> String {
    format!("PSP error code 0x{:08X}", errno as u32)
}
