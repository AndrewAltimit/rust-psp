# Kernel Mode Guide

Kernel mode gives a PSP module privileged access: kernel memory partitions,
NAND flash, the Media Engine (ME) coprocessor, memory-mapped hardware
registers, exception handlers, and CFW syscall hooking. It requires custom
firmware.

> This guide replaces the original kernel-mode implementation plan, which was
> completed in commit `6f01ff8`. The plan is still available in git history
> (`git show ad92131:docs/KERNEL_MODE_PLAN.md`).

## Enabling kernel mode

Two things are needed: the `kernel` cargo feature, and declaring the module
with `module_kernel!()` instead of `module!()`.

```toml
[dependencies]
psp = { git = "https://github.com/AndrewAltimit/rust-psp", features = ["kernel"] }
```

```rust
#![no_std]
#![no_main]

psp::module_kernel!("MyKernelApp", 1, 0);

fn psp_main() {
    psp::callback::setup_exit_callback().unwrap();
    unsafe {
        let me_mhz = psp::sys::scePowerGetMeClockFrequency();
        psp::dprintln!("ME clock: {}MHz", me_mhz);
    }
}
```

- `module_kernel!()` only exists when the `kernel` feature is enabled. It
  emits the same `module_start`/`module_stop` glue as `module!()`, but sets
  the `SceModuleInfo` attribute to `0x1000` (kernel) instead of `0`.
- Enabling the feature without `module_kernel!()` still produces a user-mode
  module. The gated APIs will compile but fail at runtime.
- With `kernel` enabled, the global allocator reserves a smaller heap arena
  (`KERNEL_HEAP_SIZE` in `psp/src/alloc_impl.rs`), because kernel partition
  memory is shared with the firmware.

The complete example is in [`examples/kernel-mode/`](../examples/kernel-mode/).
It reads the ME clock, locks volatile memory, queries NAND geometry, and reads a
GPIO register through `psp::hw`. CI builds it on every push and PR, but CI
does not run it. PPSSPP will load it, but kernel-only calls may return errors.

## Requirements

- **Custom firmware** such as ARK-4, PRO, or ME. Stock firmware will not load
  kernel-mode homebrew.
- Development and testing have mostly targeted **6.60/6.61** CFW. `psp::hook`
  also includes a workaround for PRO-C2 on 6.20 (see below).
- A kernel-mode EBOOT is installed the same way as a user-mode one, under
  `ms0:/PSP/GAME/<name>/EBOOT.PBP`.
- Test on real hardware. PPSSPP does not emulate most of these paths (ME
  boot, NAND, hardware registers, exception handlers).

## What the `kernel` feature unlocks

### Compile-time gated (only exist with `features = ["kernel"]`)

| Item | Kind | Description |
|------|------|-------------|
| `psp::module_kernel!()` | macro | Declares a kernel-mode module (attribute `0x1000`) |
| `psp::sys::sceNand*` | 12 syscalls | NAND flash: lock/unlock, write-protect, read pages/ID/status, geometry, bad-block check |
| `psp::sys::sceSircsSend` | 1 syscall | Infrared remote (SIRCS protocol) |
| `psp::sys::sctrlHENFindFunction`, `sctrlHENPatchSyscall` | 2 syscalls | CFW `SystemCtrlForKernel` API |
| `psp::sys::sceUsbbd*` | 8 syscalls | USB bus driver (`sceUsbBus_driver`): register a USB device driver, send/receive requests, stall, clear FIFO |
| `psp::sys::sceKernelRegister{Default,}ExceptionHandler`, `SceKernelException` | 2 syscalls + types | CPU exception handlers (`ExceptionManagerForKernel`) |
| `psp::me` | module | Media Engine boot (`me_boot`, `MeBootParams`), ME memory (`me_alloc`, `to_uncached`/`to_cached`), and a task executor (`MeExecutor`, `MeHandle`) |
| `psp::hw` | module | Volatile MMIO helpers (`hw_read{8,16,32}`, `hw_write{8,16,32}`), typed `Register<T>`, and register address constants (GPIO, sysreg, DMAC, display, audio, USB) |
| `psp::hook` | module | `SyscallHook::install` (tries `sctrlHENPatchSyscall`, falls back to an inline `j` patch with a trampoline) and `find_function` (resolve a kernel export by module/library/NID) |
| `psp::gpio`, `psp::syscon`, `psp::sysreg` | modules | High-level GPIO, System Controller (battery, power, raw Syscon commands), and System Register (GPIO/USB enable) access. Call `init()` first. |
| `psp::sys::gpio`, `psp::sys::syscon`, `psp::sys::sysreg` | modules | Low-level driver tables backing the modules above. These are resolved at runtime with `psp::hook::find_function`, not with `psp_extern!` import stubs. |

### Ungated, but only useful from a kernel module

These bindings compile without the feature, but the firmware rejects them
(or they do nothing useful) unless the caller runs in kernel mode:

| Item | Notes |
|------|-------|
| `scePowerSetMeClockFrequency` / `scePowerGetMeClockFrequency` | ME clock control |
| `sceKernelAllocPartitionMemory` with kernel partitions | Partitions 1, 3-5, and 8-12 are kernel-only (see `SceSysMemPartitionId`). Partitions 3 and 10 are ME-accessible. |
| `sceKernelRegisterSubIntrHandler` and related calls | Interrupt handler registration |
| `sceKernelLoadModule` / `sceKernelStartModule` | Loading kernel PRXs. From user mode, CFW's `psp::sys::kubridge::kuKernelLoadModule` is the usual alternative. |

`psp::sys::kubridge` (`kuKernelLoadModule`, `kuKernelGetModel`) is **not**
gated. It is a CFW bridge that lets user-mode code ask the kernel to do
things for it.

## Media Engine

The ME is a second MIPS R4000 core. `psp::me` boots it and runs tasks on it:

```rust
use psp::me::MeExecutor;

unsafe extern "C" fn double(arg: i32) -> i32 { arg * 2 }

let mut exec = MeExecutor::new(4096).unwrap();      // stack size in bytes
let handle = unsafe { exec.submit(double, 21) };
let result = exec.wait(&handle);                     // 42
```

Memory shared between the main CPU and the ME must be accessed through
uncached addresses (`addr | 0x4000_0000`, see `psp::me::to_uncached`), because
the ME has no coherent view of the main CPU's cache. `me_alloc` allocates from
the ME kernel partition and returns an uncached pointer. The boot stub lives
in `psp/src/asm/me.S` and is linked with `global_asm!`.

Separately, the high-level `psp::mpeg::AvcDecoder` and `psp::audiocodec` use
the ME through the firmware's codec modules. Those modules work in user mode
and do not need the `kernel` feature.

## Syscall hooking (CFW plugins)

`psp::hook::SyscallHook::install(module, library, nid, replacement)` tries
`sctrlHENPatchSyscall` first. If that fails, it overwrites the target's first
two instructions with `j hook; nop` and builds a trampoline, which
`original_ptr()` returns. On some CFW versions (notably PRO-C2 on 6.20), the
kernel import stubs for `SystemCtrlForKernel` are only partially patched.
`psp::hook` therefore resolves these stubs by reading their raw bytes instead
of calling through `psp_extern!`. See the module docs in `psp/src/hook.rs`.

## Exception handlers

```rust
use core::ffi::c_void;

unsafe extern "C" fn on_exception(code: u32, ctx: *mut c_void) -> i32 {
    // Inspect `ctx` (firmware-dependent register dump). Return 0 to mark
    // the exception handled, or -1 to pass it to the next handler.
    -1
}

unsafe {
    psp::sys::sceKernelRegisterDefaultExceptionHandler(on_exception);
}
```

The NIDs come from community documentation, and only 6.60/6.61 have been
exercised.

## Further reading

- `psp/src/me.rs`, `psp/src/hw.rs`, `psp/src/hook.rs`: module-level docs
  with more detail and caveats.
- `psp/src/sys/gpio.rs`: register map notes for TA-090v2 (PSP-3001).
- [`docs/prx.md`](prx.md): PRX section layout.
