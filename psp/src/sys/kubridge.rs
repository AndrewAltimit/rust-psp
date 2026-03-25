//! KUBridge — CFW user-mode kernel bridge functions.
//!
//! These functions are provided by custom firmware (M33/PRO/ARK-4) and
//! allow user-mode code to perform kernel-mode operations like loading
//! modules from flash0.
//!
//! The KUBridge library must be available in the CFW's kernel modules
//! for these functions to resolve.

use super::SceUid;
use super::kernel::SceKernelLMOption;

psp_extern! {
    #![name = "KUBridge"]
    #![flags = 0x4009]
    #![version = (0, 0)]

    #[psp(0x4C25EA72)]
    /// Load a module using the kernel's ModuleMgrForKernel.
    ///
    /// Unlike `sceKernelLoadModule`, this can load encrypted modules from
    /// flash0:/kd/ from user-mode code. The module is loaded into the
    /// calling process's context.
    ///
    /// # Parameters
    /// - `path`: Null-terminated path to the module (e.g. flash0:/kd/mpeg_vsh.prx)
    /// - `flags`: Unused, always 0
    /// - `option`: Module load options, can be NULL
    ///
    /// # Returns
    /// Module UID on success, negative error code on failure.
    pub fn kuKernelLoadModule(
        path: *const u8,
        flags: i32,
        option: *mut SceKernelLMOption,
    ) -> SceUid;

    #[psp(0x24331850)]
    /// Get the PSP hardware model number.
    ///
    /// # Returns
    /// 0 = PSP-1000, 1 = PSP-2000, 2 = PSP-3000, etc.
    pub fn kuKernelGetModel() -> i32;
}
