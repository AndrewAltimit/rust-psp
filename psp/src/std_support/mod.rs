//! FFI bridge between Rust std's PSP PAL and the psp crate's syscall bindings.
//!
//! These `#[no_mangle] extern "C"` functions are exported by the psp crate and
//! linked at compile time by the PSP PAL modules inside std's `sys/` directory.
//!
//! This module is only compiled when `feature = "std"` is enabled.

pub mod alloc;
pub mod fs;
pub mod os;
pub mod random;
pub mod stdio;
pub mod sync;
pub mod thread;
pub mod time;
