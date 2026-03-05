# Kernel Mode Implementation Plan

> **Status: COMPLETED** -- All 6 phases were implemented in commit `6f01ff8`. This document is retained as historical record of the design process.

---

This plan covers all work needed to bring the rust-psp SDK to full kernel mode support. The primary consumer is [OASIS_OS](https://github.com/AndrewAltimit/template-repo/tree/main/packages/oasis_os), but all changes are general-purpose SDK extensions useful to any PSP homebrew project.

**Branch:** `initial_release`
**CI:** All changes must pass `docker compose --profile ci run --rm rust-ci cargo fmt`, clippy, test, build, deny, and the PPSSPPHeadless emulator test.

---

## Current State

### What Already Works

| Feature | File | Status |
|---------|------|--------|
| `module_kernel!()` macro (sets 0x1000 flag) | `psp/src/lib.rs:199-218` | Complete |
| `ModuleInfoAttr::Kernel = 0x1000` enum | `psp/src/sys/mod.rs:137-144` | Complete |
| PRX module loading/unloading | `psp/src/sys/kernel/mod.rs:755-979` | Complete |
| Memory partition allocation (incl. ME partitions) | `psp/src/sys/kernel/mod.rs:83-221` | Complete |
| Cache control (dcache/icache) | `psp/src/sys/kernel/mod.rs:268-320` | Complete |
| Interrupt registration/control | `psp/src/sys/kernel/mod.rs:581-660` | Complete |
| Thread management, semaphores, mutexes, events | `psp/src/sys/kernel/thread.rs` (2378 lines) | Complete |
| Volatile memory lock (extra 4MB RAM) | `psp/src/sys/kernel/mod.rs:981-1034` | Complete |
| MIPS-EABI ABI bridges (i5/i6/i7) | `psp/src/eabi.rs:63-131` | Complete |
| `global_asm!` for ELF entry points | `psp/src/lib.rs:99-124` | Complete |

### What's Missing

| Feature | Impact | Phase |
|---------|--------|-------|
| `kernel` feature flag has no conditional compilation | No compile-time safety | 1 |
| NAND, SIRCS, Codec modules commented out | Can't access flash, IR, or HW codecs | 2 |
| Media Engine syscall bindings | Can't boot or control ME coprocessor | 3 |
| ME initialization assembly (`me.S`) | Can't start ME from Rust | 3 |
| Hardware register I/O helpers | Can't do memory-mapped register access | 4 |
| Exception handler registration | Can't catch bus errors, address errors | 5 |
| Kernel-only documentation | Unclear which APIs need kernel mode | 6 |

---

## Phase 1: Activate the `kernel` Feature Flag

**Goal:** Make the `kernel` feature flag functional so that kernel-only APIs are gated behind `#[cfg(feature = "kernel")]` and users get compile-time errors if they try to use kernel APIs from a user-mode module.

### 1.1 Add `#[cfg(feature = "kernel")]` gates

**File: `psp/src/sys/mod.rs`**

The following modules contain syscalls that only work in kernel mode. Gate their `pub use` exports behind the feature flag:

```rust
// Currently (line 112-116):
// pub mod sircs;
// pub mod codec;
// pub mod nand;

// Change to:
#[cfg(feature = "kernel")]
mod nand;
#[cfg(feature = "kernel")]
pub use nand::*;

#[cfg(feature = "kernel")]
mod sircs;
#[cfg(feature = "kernel")]
pub use sircs::*;

#[cfg(feature = "kernel")]
mod codec;
#[cfg(feature = "kernel")]
pub use codec::*;
```

### 1.2 Gate kernel-only partition IDs

**File: `psp/src/sys/kernel/mod.rs`**

The `SceSysMemPartitionId` enum (line 83-99) contains kernel-only partition IDs. Add doc comments indicating which partitions require kernel mode, but do NOT gate the enum itself (user-mode code may need the type for interop).

Instead, add a doc comment block:

```rust
/// Memory partition identifiers.
///
/// Partitions 1, 3-5, 8-12 are kernel-only. Attempting to allocate from
/// these partitions in user mode will return an error from the firmware.
/// Use `SceKernelPrimaryUserPartition` (2) for user-mode allocations.
```

### 1.3 Gate the `module_kernel!` macro

**File: `psp/src/lib.rs`**

Wrap the `module_kernel!` macro with `#[cfg(feature = "kernel")]` so it's only available when the feature is enabled:

```rust
#[cfg(feature = "kernel")]
#[macro_export]
macro_rules! module_kernel {
    ($name:expr, $version_major:expr, $version_minor:expr) => {
        $crate::__module_impl!($name, $version_major, $version_minor, 0x1000);
    };
}
```

### 1.4 Update documentation

**File: `psp/Cargo.toml`**

Expand the feature comment:

```toml
# Kernel mode module support (PSP_MODULE_INFO flag 0x1000).
# Enables: module_kernel!() macro, NAND/SIRCS/Codec syscalls,
# Media Engine control, and hardware register access.
# Requires custom firmware (ARK-4, PRO, ME CFW, etc.).
kernel = []
```

### 1.5 Add CI stage for kernel feature

**File: `ci/run-ci.sh`** (if it exists) or the GitHub Actions workflows.

Add a build step that compiles with `--features kernel` to ensure kernel-gated code compiles:

```yaml
- name: Build with kernel feature
  run: docker compose --profile ci run --rm rust-ci cargo +nightly build --features kernel
```

Note: This is a build check only (not a runtime test) since kernel-mode EBOOTs can't run in PPSSPP user-mode emulation.

---

## Phase 2: Uncomment and Export Existing Modules

**Goal:** Enable the NAND, SIRCS, and Codec modules that already have bindings written but are commented out.

### 2.1 Enable NAND module

**File: `psp/src/sys/nand.rs`** -- already complete (52 lines, 9 syscalls).

No changes needed to the file itself. Just uncomment and gate in `sys/mod.rs` (done in Phase 1.1).

Syscalls exposed:
- `sceNandSetWriteProtect`, `sceNandLock`, `sceNandUnlock`
- `sceNandReadStatus`, `sceNandReset`, `sceNandReadId`
- `sceNandReadPages`, `sceNandGetPageSize`, `sceNandGetPagesPerBlock`
- `sceNandGetTotalBlocks`, `sceNandReadBlockWithRetry`, `sceNandIsBadBlock`

### 2.2 Enable SIRCS module

**File: `psp/src/sys/sircs.rs`** -- already complete (20 lines, 1 syscall).

No changes needed to the file itself.

Syscalls exposed:
- `sceSircsSend` (infrared remote control)

### 2.3 Enable Codec module

**File: `psp/src/sys/codec.rs`** -- already complete (72 lines).

No changes needed to the file itself.

Syscalls exposed:
- Video: `sceVideocodecOpen`, `sceVideocodecGetEDRAM`, `sceVideocodecInit`, `sceVideocodecDecode`, `sceVideocodecReleaseEDRAM`
- Audio: `sceAudiocodecCheckNeedMem`, `sceAudiocodecInit`, `sceAudiocodecDecode`, `sceAudiocodecGetEDRAM`, `sceAudiocodecReleaseEDRAM`
- `AudioCodec` enum: `At3Plus`, `At3`, `Mp3`, `Aac`

### 2.4 Verify NID correctness

Cross-reference all NIDs in `nand.rs`, `sircs.rs`, and `codec.rs` against the [PSP NID database](https://github.com/pspdev/pspsdk/blob/master/tools/psp-nid-tables.xml) or the [psp-archive NID reference](https://psp-archive.github.io/pspsdk-docs/).

---

## Phase 3: Media Engine Coprocessor Support

**Goal:** Add syscall bindings and assembly support for controlling the PSP's Media Engine (ME) coprocessor. The ME is a second MIPS core dedicated to media decoding (MP3, ATRAC3, H.264). Booting and controlling it requires kernel mode.

### 3.1 Create ME syscall module

**File: `psp/src/sys/me.rs`** (NEW)

Add bindings for the `sceMeCore` and related kernel functions. These are not in the upstream rust-psp or the standard PSPSDK headers -- they come from reverse-engineered kernel modules and the pspdev community.

```rust
//! Media Engine (ME) coprocessor control.
//!
//! The PSP contains a second MIPS core (the "Media Engine") running at up to
//! 333MHz. In kernel mode, applications can boot the ME, submit tasks to it,
//! and use it for hardware-accelerated media decoding.
//!
//! Requires `feature = "kernel"`.

use core::ffi::c_void;

/// ME task function signature.
///
/// The ME executes this function on its own core with its own stack.
pub type MeTask = unsafe extern "C" fn(arg: i32) -> i32;

// ME power/clock control (from scePower kernel API)
psp_extern! {
    #![name = "scePower"]
    #![flags = 0x4001]
    #![version = (0x00, 0x00)]

    /// Set the Media Engine clock frequency (MHz).
    ///
    /// Valid values: 1-333. The ME clock is independent of the CPU clock.
    #[psp(0x469989AD)]
    pub fn scePowerSetMeClockFrequency(frequency: i32) -> i32;

    /// Get the current Media Engine clock frequency (MHz).
    #[psp(0x3234844A)]
    pub fn scePowerGetMeClockFrequency() -> i32;
}
```

**Key syscalls to bind** (research NIDs from pspdev/psplinkusb/ME documentation):

| Function | Purpose | Source |
|----------|---------|--------|
| ME clock control | Set/get ME frequency | `scePower` module |
| ME partition allocation | Allocate ME-accessible memory | Already in `kernel/mod.rs` (partitions 3, 7, 10) |
| ME cache operations | Flush/invalidate for ME-shared memory | Already in `kernel/mod.rs` |

Note: The ME does not have a formal syscall API in all firmware versions. The traditional approach uses:
1. Allocate memory in ME partition (partition 3 or 10)
2. Write task code + data to that memory
3. Boot the ME with a jump to the task entry point
4. Synchronize via shared memory flags

### 3.2 ME initialization assembly

**File: `psp/src/asm/me.S`** (NEW -- to be extracted from psixpsp.7z)

This file contains ~60 lines of hand-tuned MIPS assembly that:
1. Manipulates CP0 registers (Status, Cause, EPC, ErrorEPC)
2. Sets up the ME's cache (16KB I-cache, 16KB D-cache)
3. Configures the ME's exception vectors
4. Jumps to the ME task entry point

The assembly must be integrated via `global_asm!` in a new file:

**File: `psp/src/me.rs`** (NEW)

```rust
//! Media Engine boot and task management.
//!
//! Provides the low-level assembly for booting the ME coprocessor and
//! a safe(r) Rust wrapper for submitting tasks.

#[cfg(all(target_os = "psp", feature = "kernel"))]
core::arch::global_asm!(include_str!("asm/me.S"));

/// Boot the Media Engine and execute a task on it.
///
/// # Safety
///
/// - `task` must be a valid function pointer in ME-accessible memory
///   (partition 3 or 10, uncached address with 0x4000_0000 mask).
/// - The caller must have allocated and initialized the ME stack.
/// - The caller must be running in kernel mode.
/// - Only one ME task can run at a time.
#[cfg(feature = "kernel")]
pub unsafe fn me_boot(task: MeTask, arg: i32, stack_top: *mut u8) -> i32 {
    // Implementation calls into the global_asm! ME boot stub.
    // The exact mechanism depends on the assembly extracted from psixpsp.7z.
    todo!("Implement after extracting me.S from psixpsp.7z")
}
```

### 3.3 ME memory helpers

**File: `psp/src/me.rs`** (append to the file from 3.2)

Add helpers for ME-compatible memory allocation:

```rust
/// Allocate memory in the ME kernel partition (partition 3).
///
/// Returns an uncached pointer suitable for ME access.
/// The ME cannot access cached main RAM -- all shared memory must use
/// uncached addresses (OR'd with 0x4000_0000).
#[cfg(feature = "kernel")]
pub unsafe fn me_alloc(size: usize, name: &str) -> Result<*mut u8, i32> {
    // Use sceKernelAllocPartitionMemory with partition 3
    // Convert returned pointer to uncached address
    todo!()
}
```

### 3.4 Audio codec integration with ME

**File: `psp/src/sys/codec.rs`** (extend)

Add documentation explaining that `sceAudiocodecDecode` uses the ME internally when called from user mode, but in kernel mode the caller can directly control ME partition allocation for codec buffers.

### 3.5 Source the assembly

The `me.S` file needs to be extracted from `psixpsp.7z` in the template-repo. If the archive is not available, write the assembly from scratch based on:
- [PSP ME documentation](https://github.com/pspdev/pspsdk/tree/master/src/me)
- [ME boot sequence](https://github.com/pspdev/psplinkusb) source
- PSPSDK's `pspmecore.h` and `libme` implementation

The assembly structure follows this pattern:
```asm
# me.S -- Media Engine boot stub
# Linked via global_asm! in psp/src/me.rs

.set noreorder
.set noat

.section .text.me_boot, "ax", @progbits
.global _me_boot_entry
_me_boot_entry:
    # 1. Disable interrupts (mtc0 $zero, CP0_STATUS)
    # 2. Set up ME exception vectors
    # 3. Initialize ME cache (cache instructions)
    # 4. Set up ME stack pointer ($sp)
    # 5. Jump to task entry point ($a0 = task, $a1 = arg)
```

---

## Phase 4: Hardware Register I/O

**Goal:** Add safe abstractions for memory-mapped I/O register access. Many PSP hardware peripherals are controlled by reading/writing specific physical addresses. In kernel mode, these addresses are directly accessible.

### 4.1 Register access primitives

**File: `psp/src/hw.rs`** (NEW)

```rust
//! Hardware register access for kernel-mode PSP applications.
//!
//! The PSP's peripherals are controlled via memory-mapped I/O registers
//! at fixed physical addresses. These functions provide volatile read/write
//! access with proper memory ordering.
//!
//! Requires `feature = "kernel"`.

/// Read a 32-bit hardware register.
///
/// # Safety
///
/// `addr` must be a valid memory-mapped I/O register address.
/// Caller must be in kernel mode.
#[cfg(feature = "kernel")]
#[inline(always)]
pub unsafe fn hw_read32(addr: u32) -> u32 {
    let ptr = addr as *const u32;
    core::ptr::read_volatile(ptr)
}

/// Write a 32-bit hardware register.
///
/// # Safety
///
/// `addr` must be a valid memory-mapped I/O register address.
/// Caller must be in kernel mode.
#[cfg(feature = "kernel")]
#[inline(always)]
pub unsafe fn hw_write32(addr: u32, value: u32) {
    let ptr = addr as *mut u32;
    core::ptr::write_volatile(ptr, value);
}

/// Read a 16-bit hardware register.
#[cfg(feature = "kernel")]
#[inline(always)]
pub unsafe fn hw_read16(addr: u32) -> u16 {
    let ptr = addr as *const u16;
    core::ptr::read_volatile(ptr)
}

/// Write a 16-bit hardware register.
#[cfg(feature = "kernel")]
#[inline(always)]
pub unsafe fn hw_write16(addr: u32, value: u16) {
    let ptr = addr as *mut u16;
    core::ptr::write_volatile(ptr, value);
}
```

### 4.2 Hardware register definitions

**File: `psp/src/hw.rs`** (append)

Define constants for commonly-used hardware register addresses. Source these from PSPSDK headers and the pspdev wiki.

```rust
// System control registers
pub const SYS_CTRL_BASE: u32 = 0xBC10_0000;

// GPIO registers
pub const GPIO_BASE: u32 = 0xBE24_0000;
pub const GPIO_PORT_READ: u32 = GPIO_BASE + 0x004;
pub const GPIO_PORT_SET: u32 = GPIO_BASE + 0x008;
pub const GPIO_PORT_CLEAR: u32 = GPIO_BASE + 0x00C;

// Display engine registers
pub const DISPLAY_BASE: u32 = 0xBE14_0000;

// Audio hardware registers
pub const AUDIO_BASE: u32 = 0xBE00_0000;

// DMA controller
pub const DMAC_BASE: u32 = 0xBC90_0000;

// Memory Stick interface
pub const MSPRO_BASE: u32 = 0xBD20_0000;
```

### 4.3 Type-safe register wrappers (optional enhancement)

Consider adding a `Register<T>` wrapper type for compile-time checked register access:

```rust
/// A memory-mapped I/O register at a fixed address.
#[cfg(feature = "kernel")]
pub struct Register<T: Copy> {
    addr: u32,
    _phantom: core::marker::PhantomData<T>,
}

#[cfg(feature = "kernel")]
impl Register<u32> {
    pub const fn new(addr: u32) -> Self {
        Self { addr, _phantom: core::marker::PhantomData }
    }

    #[inline(always)]
    pub unsafe fn read(&self) -> u32 {
        core::ptr::read_volatile(self.addr as *const u32)
    }

    #[inline(always)]
    pub unsafe fn write(&self, value: u32) {
        core::ptr::write_volatile(self.addr as *mut u32, value);
    }
}
```

---

## Phase 5: Exception Handling

**Goal:** Add bindings for registering custom exception handlers. In kernel mode, applications can install handlers for CPU exceptions (bus errors, address errors, etc.) which is useful for debugging and fault recovery.

### 5.1 Exception handler syscalls

**File: `psp/src/sys/kernel/mod.rs`** (extend)

Add bindings for exception-related kernel functions:

```rust
/// CPU exception codes.
#[repr(u32)]
pub enum SceKernelException {
    /// Interrupt (handled separately via sub-interrupt system).
    Interrupt = 0,
    /// TLB modification exception.
    TlbModification = 1,
    /// TLB load miss.
    TlbLoadMiss = 2,
    /// TLB store miss.
    TlbStoreMiss = 3,
    /// Address error on load.
    AddressErrorLoad = 4,
    /// Address error on store.
    AddressErrorStore = 5,
    /// Bus error on instruction fetch.
    BusErrorInstruction = 6,
    /// Bus error on data access.
    BusErrorData = 7,
    /// Syscall exception.
    Syscall = 8,
    /// Breakpoint exception.
    Breakpoint = 9,
    /// Reserved instruction.
    ReservedInstruction = 10,
    /// Coprocessor unusable.
    CoprocessorUnusable = 11,
    /// Arithmetic overflow.
    Overflow = 12,
}

/// Exception handler function signature.
///
/// `exception` is the exception code, `context` points to the saved
/// CPU register state at the time of the exception.
pub type SceKernelExceptionHandler = unsafe extern "C" fn(
    exception: u32,
    context: *mut c_void,
) -> i32;
```

Add the syscall bindings (research NIDs from pspdev):

```rust
psp_extern! {
    #![name = "ExceptionManagerForKernel"]
    #![flags = 0x0001]
    #![version = (0x00, 0x00)]

    /// Register a default exception handler.
    ///
    /// Called for any exception that doesn't have a specific handler.
    #[psp(0x565C0B0E)]
    pub fn sceKernelRegisterDefaultExceptionHandler(
        handler: SceKernelExceptionHandler,
    ) -> i32;

    /// Register a handler for a specific exception type.
    #[psp(0x1AA6CFFA)]
    pub fn sceKernelRegisterExceptionHandler(
        code: u32,
        handler: SceKernelExceptionHandler,
    ) -> i32;
}
```

Note: The exact NIDs for `ExceptionManagerForKernel` need verification. The NIDs above are from community documentation and may vary by firmware version. Test against firmware 6.60/6.61 (the standard CFW base).

---

## Phase 6: Documentation and Examples

**Goal:** Document which APIs require kernel mode, add usage examples, and update the README.

### 6.1 Module-level documentation

Every kernel-gated module and function should have a doc comment noting the kernel requirement:

```rust
/// # Kernel Mode Required
///
/// This function requires the module to be compiled with `feature = "kernel"`
/// and declared with `psp::module_kernel!()`. Calling from user mode will
/// return an error or crash.
```

### 6.2 Kernel mode example

**File: `examples/kernel-mode/Cargo.toml`** (NEW)

```toml
[package]
name = "kernel-mode-example"
version = "0.1.0"
edition = "2024"

[dependencies]
psp = { path = "../../psp", features = ["kernel"] }
```

**File: `examples/kernel-mode/src/main.rs`** (NEW)

```rust
#![no_std]
#![no_main]

psp::module_kernel!("KernelDemo", 1, 0);

fn psp_main() {
    psp::enable_home_button();

    // Demonstrate kernel-only features:
    // 1. Read ME clock frequency
    // 2. Enumerate loaded modules
    // 3. Access volatile memory (extra 4MB)
    // 4. Read NAND info

    unsafe {
        // ME clock
        let me_freq = psp::sys::scePowerGetMeClockFrequency();
        psp::dprintln!("ME clock: {}MHz", me_freq);

        // Volatile memory (4MB extra RAM on PSP-2000+)
        let mut addr: *mut u8 = core::ptr::null_mut();
        let mut size: u32 = 0;
        let ret = psp::sys::sceKernelVolatileMemLock(
            0,
            &mut addr as *mut _ as *mut *mut core::ffi::c_void,
            &mut size as *mut _ as *mut i32,
        );
        if ret == 0 {
            psp::dprintln!("Volatile mem: {:p}, {} bytes", addr, size);
            psp::sys::sceKernelVolatileMemUnlock(0);
        }

        // NAND info
        let page_size = psp::sys::sceNandGetPageSize();
        let pages_per_block = psp::sys::sceNandGetPagesPerBlock();
        let total_blocks = psp::sys::sceNandGetTotalBlocks();
        psp::dprintln!("NAND: page={}B, ppb={}, blocks={}", page_size, pages_per_block, total_blocks);
    }
}
```

### 6.3 Update README.md

Add a "Kernel Mode" section to the README documenting:
- How to enable kernel mode (`features = ["kernel"]` + `module_kernel!()`)
- What APIs become available
- CFW requirements
- The ME coprocessor subsystem
- Hardware register access patterns
- Link to the kernel-mode example

### 6.4 Add kernel feature to CI test matrix

Update `.github/workflows/ci.yml` to include a build step with `--features kernel`:

```yaml
- name: Build with kernel feature
  run: |
    docker compose --profile ci run --rm rust-ci \
      cargo +nightly build --features kernel
```

This ensures kernel-gated code continues to compile as the codebase evolves.

---

## File Summary

| File | Action | Phase |
|------|--------|-------|
| `psp/Cargo.toml` | Expand `kernel` feature docs | 1 |
| `psp/src/lib.rs` | Gate `module_kernel!` behind feature | 1 |
| `psp/src/sys/mod.rs` | Uncomment + gate nand/sircs/codec | 1, 2 |
| `psp/src/sys/nand.rs` | No changes (already complete) | 2 |
| `psp/src/sys/sircs.rs` | No changes (already complete) | 2 |
| `psp/src/sys/codec.rs` | Add ME documentation | 2, 3 |
| `psp/src/sys/me.rs` | NEW: ME syscall bindings | 3 |
| `psp/src/me.rs` | NEW: ME boot wrapper + memory helpers | 3 |
| `psp/src/asm/me.S` | NEW: ME boot assembly (from psixpsp.7z) | 3 |
| `psp/src/hw.rs` | NEW: Hardware register I/O | 4 |
| `psp/src/sys/kernel/mod.rs` | Add exception handler bindings | 5 |
| `examples/kernel-mode/` | NEW: Kernel mode example | 6 |
| `README.md` | Add kernel mode section | 6 |
| `.github/workflows/ci.yml` | Add kernel feature build step | 6 |
| `.github/workflows/main-ci.yml` | Add kernel feature build step | 6 |

---

## Implementation Order

Phases 1-2 are independent and can be done together. Phase 3 depends on having the `psixpsp.7z` assembly extracted (or rewritten from scratch). Phases 4-5 are independent of each other but both depend on Phase 1. Phase 6 should be done last.

```
Phase 1 (feature flag) ──┬── Phase 3 (ME support)
Phase 2 (uncomment)    ──┤
                          ├── Phase 4 (HW registers)
                          ├── Phase 5 (exceptions)
                          └── Phase 6 (docs + examples)
```

## NID Verification Resources

All syscall NIDs must be verified against these references before implementation:

- [PSPSDK NID tables](https://github.com/pspdev/pspsdk/blob/master/tools/psp-nid-tables.xml)
- [PSP archive documentation](https://psp-archive.github.io/pspsdk-docs/)
- [pspdev headers](https://github.com/pspdev/pspsdk/tree/master/src)
- [PSP kernel module exports](https://github.com/uofw/upern/wiki) (community RE)
- Firmware 6.60/6.61 is the target firmware version (standard CFW base)

## Testing Strategy

- **Phases 1-2:** `cargo +nightly build --features kernel` must compile. Existing CI tests must still pass without the kernel feature.
- **Phase 3:** Build-only verification. ME assembly can be syntax-checked but not runtime-tested in PPSSPP (ME emulation is incomplete).
- **Phase 4:** Build-only. Hardware register access cannot be tested in emulation.
- **Phase 5:** Build-only. Exception handlers require real hardware or very specific PPSSPP scenarios.
- **Phase 6:** The kernel-mode example must produce an EBOOT.PBP. Running it in PPSSPP headless (timeout = success) verifies it doesn't crash on load.
