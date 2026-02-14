//! Syscall hook helpers for CFW kernel-mode plugins.
//!
//! Wraps the `sctrlHEN*` APIs into a safe-ish pattern for hooking PSP system
//! functions. Used by overlay plugins to intercept `sceDisplaySetFrameBuf`
//! and similar functions.
//!
//! # Example
//!
//! ```ignore
//! use psp::hook::SyscallHook;
//!
//! static mut DISPLAY_HOOK: Option<SyscallHook> = None;
//!
//! unsafe {
//!     DISPLAY_HOOK = SyscallHook::install(
//!         b"sceDisplay_Service\0",
//!         b"sceDisplay\0",
//!         0x289D82FE, // sceDisplaySetFrameBuf NID
//!         my_hook as *mut u8,
//!     );
//! }
//! ```

use core::ptr;

/// A handle to a patched syscall, storing the original function pointer.
pub struct SyscallHook {
    /// Pointer to the original function before patching.
    original: *mut u8,
}

impl SyscallHook {
    /// Install a syscall hook by finding a function NID and patching it.
    ///
    /// Returns `Some(SyscallHook)` on success, `None` if the function was
    /// not found or the patch failed.
    ///
    /// # Parameters
    ///
    /// - `module_name`: Null-terminated module name (e.g. `b"sceDisplay_Service\0"`)
    /// - `library_name`: Null-terminated library name (e.g. `b"sceDisplay\0"`)
    /// - `nid`: Numeric ID of the function to hook
    /// - `replacement`: Pointer to the replacement function
    ///
    /// # Safety
    ///
    /// - Must be called from kernel mode.
    /// - `replacement` must have the same signature as the original function.
    /// - The replacement function must remain valid for the lifetime of the hook.
    /// - Caller must flush icache after installation (e.g. `sceKernelIcacheClearAll()`).
    pub unsafe fn install(
        module_name: *const u8,
        library_name: *const u8,
        nid: u32,
        replacement: *mut u8,
    ) -> Option<Self> {
        // SAFETY: Caller guarantees kernel mode and valid module/library names.
        let original = unsafe { crate::sys::sctrlHENFindFunction(module_name, library_name, nid) };
        if original.is_null() {
            return None;
        }

        // SAFETY: original is a valid function pointer from sctrlHENFindFunction.
        let ret = unsafe { crate::sys::sctrlHENPatchSyscall(original, replacement) };
        if ret < 0 {
            return None;
        }

        Some(Self { original })
    }

    /// Get the original function pointer for calling through the hook.
    ///
    /// # Safety
    ///
    /// The caller must cast this to the correct function signature and call
    /// it with valid arguments matching the original function's ABI.
    #[inline]
    pub unsafe fn original_ptr(&self) -> *mut u8 {
        self.original
    }
}

// SAFETY: SyscallHook is a thin wrapper around a raw pointer to a system
// function. The pointer itself is stable (syscall table entry) and the hook
// is typically stored in a static. Send+Sync are needed for static storage.
unsafe impl Send for SyscallHook {}
unsafe impl Sync for SyscallHook {}
