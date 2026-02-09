//! PSP-specific raw type definitions.

#![stable(feature = "raw_ext", since = "1.1.0")]

/// Raw file descriptor type (SceUid on PSP).
#[stable(feature = "raw_ext", since = "1.1.0")]
pub type RawFd = i32;
