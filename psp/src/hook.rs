//! Kernel-mode function hooking for CFW plugins.
//!
//! Provides [`SyscallHook`] for intercepting PSP system functions from
//! kernel-mode PRX plugins. Also provides [`find_function`] for resolving
//! kernel functions by NID without hooking them.
//!
//! # Hooking methods
//!
//! Two methods are supported, chosen automatically:
//!
//! 1. **Syscall patching** via `sctrlHENPatchSyscall` (preferred) -- redirects
//!    the syscall table entry so usermode calls to the original function invoke
//!    the replacement instead.
//!
//! 2. **Inline patching** (fallback) -- overwrites the target function's first
//!    two instructions with `j hook; nop` and builds a trampoline with the
//!    saved instructions for calling the original.
//!
//! # Kernel stub workaround
//!
//! On some CFW versions (notably PRO-C2 on 6.20), kernel import stubs are
//! patched with a `j target` instruction but the delay slot (second word) is
//! left as the unpatched `Stub` struct data (a `nid_addr` pointer). Calling
//! through the `psp_extern!` wrapper crashes because the garbage delay slot
//! decodes to a MIPS instruction that corrupts registers.
//!
//! This module resolves the CFW functions by reading the raw stub bytes and
//! extracting the jump target, calling via transmuted function pointers.
//!
//! # Example
//!
//! ```ignore
//! use psp::hook::SyscallHook;
//!
//! static mut DISPLAY_HOOK: Option<SyscallHook> = None;
//!
//! unsafe extern "C" fn my_hook(
//!     top_addr: *const u8, buf_width: usize, pixel_format: u32, sync: u32,
//! ) -> u32 {
//!     // Draw overlay on framebuffer...
//!
//!     // Call original function
//!     let original: unsafe extern "C" fn(*const u8, usize, u32, u32) -> u32 =
//!         core::mem::transmute(DISPLAY_HOOK.as_ref().unwrap().original_ptr());
//!     original(top_addr, buf_width, pixel_format, sync)
//! }
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

// ---------------------------------------------------------------------------
// Raw stub symbol references
// ---------------------------------------------------------------------------

// The `psp_extern!` macro in `sys/sctrl.rs` generates these as
// `#[no_mangle] static Stub` values in `.sceStub.text`. Firmware patches
// them at load time. We read the raw bytes instead of calling through the
// broken wrappers.
unsafe extern "C" {
    #[link_name = "__sctrlHENFindFunction_stub"]
    static FIND_FUNC_STUB: [u32; 2];

    #[link_name = "__sctrlHENPatchSyscall_stub"]
    static PATCH_SYSCALL_STUB: [u32; 2];
}

// ---------------------------------------------------------------------------
// SyscallHook
// ---------------------------------------------------------------------------

/// A handle to a hooked kernel function.
///
/// Supports both syscall patching and inline patching. The method is chosen
/// automatically by [`install`](Self::install). Use [`original_ptr`](Self::original_ptr)
/// to get a callable pointer to the original function regardless of method.
///
/// # Storage
///
/// Typically stored in a `static mut`:
///
/// ```ignore
/// static mut HOOK: Option<SyscallHook> = None;
/// ```
#[repr(C, align(16))]
pub struct SyscallHook {
    /// Trampoline for inline hooks: [saved_instr1, saved_instr2, j orig+8, nop].
    /// Zeroed and unused for syscall-patched hooks.
    trampoline: [u32; 4],
    /// Original function address (from sctrlHENFindFunction).
    original: *mut u8,
    /// Whether this hook uses inline patching.
    is_inline: bool,
}

impl SyscallHook {
    /// Install a hook on a kernel function identified by module, library,
    /// and NID.
    ///
    /// Tries `sctrlHENPatchSyscall` first, falling back to inline patching
    /// if syscall patching fails.
    ///
    /// Returns `Some(SyscallHook)` on success, `None` if:
    /// - SystemCtrlForKernel stubs were not resolved by CFW
    /// - The target function was not found
    /// - Both hooking methods failed
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
    pub unsafe fn install(
        module_name: *const u8,
        library_name: *const u8,
        nid: u32,
        replacement: *mut u8,
    ) -> Option<Self> {
        // Resolve sctrlHENFindFunction from firmware-patched stub.
        let find_addr = unsafe { resolve_kernel_stub(&FIND_FUNC_STUB)? };
        let find_fn: unsafe extern "C" fn(*const u8, *const u8, u32) -> *mut u8 =
            unsafe { core::mem::transmute(find_addr as usize) };

        // Find the target function.
        // SAFETY: Caller guarantees kernel mode and valid module/library names.
        let original = unsafe { find_fn(module_name, library_name, nid) };
        if original.is_null() {
            return None;
        }

        // Try syscall patching first (preferred -- doesn't modify function bytes).
        if let Some(patch_addr) = unsafe { resolve_kernel_stub(&PATCH_SYSCALL_STUB) } {
            let patch_fn: unsafe extern "C" fn(*mut u8, *mut u8) -> i32 =
                unsafe { core::mem::transmute(patch_addr as usize) };

            // SAFETY: original and replacement are valid function pointers.
            let ret = unsafe { patch_fn(original, replacement) };

            // PRO-C2 returns the old syscall table entry (a kernel address
            // like 0x8802xxxx) on success, not 0. Kernel addresses have bit
            // 31 set, making them negative as i32. Only treat actual SCE
            // error codes (0x8000xxxx - 0x8007xxxx) as failures.
            let ret_u = ret as u32;
            let is_error = ret_u >= 0x8000_0000 && ret_u < 0x8800_0000;
            if !is_error {
                // SAFETY: Flush caches so the patched syscall table is visible.
                unsafe {
                    crate::sys::sceKernelIcacheInvalidateAll();
                    crate::sys::sceKernelDcacheWritebackAll();
                }
                return Some(Self {
                    trampoline: [0; 4],
                    original,
                    is_inline: false,
                });
            }
        }

        // Fallback: inline hook (patches the function's entry point directly).
        // SAFETY: original is a valid kernel function address.
        unsafe { Self::install_inline(original, replacement) }
    }

    /// Get a callable pointer to the original function.
    ///
    /// For syscall-patched hooks, returns the original function address
    /// (the function bytes are unmodified, only the syscall table was changed).
    ///
    /// For inline-patched hooks, returns the trampoline address (which
    /// executes the two saved instructions then jumps to original+8).
    ///
    /// # Safety
    ///
    /// Cast to the correct function signature before calling.
    #[inline]
    pub unsafe fn original_ptr(&self) -> *mut u8 {
        if self.is_inline {
            self.trampoline.as_ptr() as *mut u8
        } else {
            self.original
        }
    }

    /// Install an inline hook by patching the function's entry point.
    ///
    /// Saves the first two instructions into a trampoline, writes
    /// `j replacement; nop` at the entry point, and appends
    /// `j original+8; nop` to the trampoline.
    ///
    /// Returns `None` if the first two instructions contain branches or
    /// jumps (which can't be safely relocated).
    ///
    /// # Safety
    ///
    /// `target` must be a valid kernel function pointer. `replacement`
    /// must have the same calling convention as the target.
    unsafe fn install_inline(
        target: *mut u8,
        replacement: *mut u8,
    ) -> Option<Self> {
        let func = target as *mut u32;

        // SAFETY: Reading the first two instructions of the kernel function.
        let instr1 = unsafe { ptr::read_volatile(func) };
        let instr2 = unsafe { ptr::read_volatile(func.add(1)) };

        // Branch/jump instructions are PC-relative and can't be relocated
        // into the trampoline without fixup. Bail out instead.
        if is_branch_or_jump(instr1) || is_branch_or_jump(instr2) {
            return None;
        }

        let mut hook = Self {
            trampoline: [0; 4],
            original: target,
            is_inline: true,
        };

        // Build trampoline: saved instructions + jump back to original+8.
        let orig_plus_8 = target as u32 + 8;
        hook.trampoline[0] = instr1;
        hook.trampoline[1] = instr2;
        hook.trampoline[2] = encode_j(orig_plus_8);
        hook.trampoline[3] = 0; // nop (delay slot)

        // Overwrite function entry with jump to replacement.
        // SAFETY: target is a valid kernel function, replacement is valid.
        unsafe {
            ptr::write_volatile(func, encode_j(replacement as u32));
            ptr::write_volatile(func.add(1), 0); // nop (delay slot)

            crate::sys::sceKernelDcacheWritebackAll();
            crate::sys::sceKernelIcacheInvalidateAll();
        }

        Some(hook)
    }
}

// SAFETY: SyscallHook is a thin wrapper around raw pointers to system
// functions and an inline trampoline. The pointers are stable kernel
// addresses and the hook is typically stored in a static.
unsafe impl Send for SyscallHook {}
unsafe impl Sync for SyscallHook {}

// ---------------------------------------------------------------------------
// Standalone function resolution
// ---------------------------------------------------------------------------

/// Find a kernel function by module, library, and NID.
///
/// Wraps `sctrlHENFindFunction` with the kernel stub delay slot workaround.
/// Useful for resolving driver-level functions without hooking them (e.g.
/// `sceCtrl_driver`, `sceCtrlSetSamplingMode`).
///
/// # Parameters
///
/// - `module_name`: Null-terminated module name (e.g. `b"sceController_Service\0"`)
/// - `library_name`: Null-terminated library name (e.g. `b"sceCtrl_driver\0"`)
/// - `nid`: Numeric ID of the function to find
///
/// # Safety
///
/// Must be called from kernel mode. Module and library names must be
/// null-terminated.
pub unsafe fn find_function(
    module_name: *const u8,
    library_name: *const u8,
    nid: u32,
) -> Option<*mut u8> {
    let find_addr = unsafe { resolve_kernel_stub(&FIND_FUNC_STUB)? };
    let find_fn: unsafe extern "C" fn(*const u8, *const u8, u32) -> *mut u8 =
        unsafe { core::mem::transmute(find_addr as usize) };

    // SAFETY: Caller guarantees kernel mode and valid module/library names.
    let ptr = unsafe { find_fn(module_name, library_name, nid) };
    if ptr.is_null() { None } else { Some(ptr) }
}

// ---------------------------------------------------------------------------
// MIPS instruction helpers
// ---------------------------------------------------------------------------

/// Encode a MIPS `j target` instruction.
///
/// The `j` instruction uses `PC[31:28] | (target[27:2] << 2)` for the
/// effective address. Only the lower 28 bits of `target` are encoded;
/// the upper 4 bits come from the program counter at execution time.
fn encode_j(target: u32) -> u32 {
    0x0800_0000 | ((target >> 2) & 0x03FF_FFFF)
}

/// Extract the absolute target address from a MIPS `j` instruction.
///
/// Returns `None` if `instruction` is not a `j` (opcode 2).
/// The upper 4 bits are taken from `pc` (the address of the instruction).
fn extract_j_target(instruction: u32, pc: u32) -> Option<u32> {
    if (instruction >> 26) != 2 {
        return None;
    }
    let offset = (instruction & 0x03FF_FFFF) << 2;
    Some((pc & 0xF000_0000) | offset)
}

/// Check if a MIPS instruction is a branch or jump that can't be relocated.
///
/// Opcodes: regimm(1), j(2), jal(3), beq(4), bne(5), blez(6), bgtz(7).
/// These are PC-relative or use absolute offsets that would be wrong in
/// a trampoline at a different address.
fn is_branch_or_jump(instr: u32) -> bool {
    matches!(instr >> 26, 1 | 2 | 3 | 4 | 5 | 6 | 7)
}

/// Resolve a CFW function address from a firmware-patched import stub.
///
/// Firmware patches kernel stubs in two patterns:
///
/// - **User-mode**: `jr $ra` (0x03E00008) + `syscall N` -- both words are
///   valid and the stub can be called directly as a function.
///
/// - **Kernel-mode** (PRO-C2): `j target` -- only word 0 is patched. Word 1
///   remains the original `Stub.nid_addr` pointer, which decodes to a
///   garbage MIPS instruction in the delay slot. We extract the jump target
///   and return it for direct calling.
///
/// Returns `None` if the stub was not resolved by firmware.
///
/// # Safety
///
/// `stub` must point to a valid import stub generated by `psp_extern!`.
unsafe fn resolve_kernel_stub(stub: &[u32; 2]) -> Option<u32> {
    // SAFETY: Reading firmware-patched stub data.
    let first_word = unsafe {
        ptr::read_volatile(stub as *const [u32; 2] as *const u32)
    };

    // User-mode resolved: jr $ra (0x03E00008) + syscall.
    // The stub works as a callable function -- return its address.
    if first_word == 0x03E00008 {
        return Some(stub as *const _ as u32);
    }

    // Kernel-mode resolved: j target.
    // Extract the absolute jump target address.
    if (first_word >> 26) == 2 {
        let stub_addr = stub as *const _ as u32;
        return extract_j_target(first_word, stub_addr);
    }

    // Not resolved by firmware.
    None
}
