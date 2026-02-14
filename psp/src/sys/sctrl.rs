//! SystemCtrlForKernel -- CFW (ARK-4/PRO) syscall hooking API.
//!
//! These functions are provided by custom firmware's SystemControl module
//! and allow kernel-mode PRX plugins to find and patch syscall table entries.
//! Used to hook system functions like `sceDisplaySetFrameBuf` for overlay
//! rendering.

psp_extern! {
    #![name = "SystemCtrlForKernel"]
    #![flags = 0x4001]
    #![version = (0, 0)]

    #[psp(0x159AF5CC)]
    /// Find a function NID in a loaded module's library.
    ///
    /// # Parameters
    ///
    /// - `module_name`: Null-terminated module name (e.g. `b"sceDisplay_Service\0"`)
    /// - `library_name`: Null-terminated library name (e.g. `b"sceDisplay\0"`)
    /// - `nid`: Numeric ID of the function to find
    ///
    /// # Return Value
    ///
    /// Pointer to the function, or null if not found.
    pub fn sctrlHENFindFunction(
        module_name: *const u8,
        library_name: *const u8,
        nid: u32,
    ) -> *mut u8;

    #[psp(0x826668E9)]
    /// Patch a syscall entry to redirect to a new function.
    ///
    /// Replaces the syscall table entry for `original` with `replacement`.
    /// After patching, any usermode call to `original` will instead invoke
    /// `replacement`.
    ///
    /// # Parameters
    ///
    /// - `original`: Pointer to the original function (from `sctrlHENFindFunction`)
    /// - `replacement`: Pointer to the replacement function
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sctrlHENPatchSyscall(
        original: *mut u8,
        replacement: *mut u8,
    ) -> i32;
}
