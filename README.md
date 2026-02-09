# rust-psp

> Rust SDK for Sony PSP homebrew development -- modernized fork with edition 2024, safety fixes, and kernel mode support.

**Forked from:** [github.com/overdrivenpotato/rust-psp](https://github.com/overdrivenpotato/rust-psp) (MIT license)

The upstream project is maintained at a low cadence (3-5 commits/year, mostly nightly breakage fixes) and has 33 open issues including known soundness bugs. This fork diverges for edition 2024 compatibility, comprehensive safety hardening, and feature additions while tracking upstream for bug fixes.

```rust
#![no_std]
#![no_main]

psp::module!("sample_module", 1, 1);

fn psp_main() {
    psp::enable_home_button();
    psp::dprintln!("Hello PSP from rust!");
}
```

## API Overview

The `psp` crate provides ~825 syscall bindings covering every major PSP subsystem, plus high-level Rust utilities.

### Syscall Bindings (`psp::sys`)

| Domain | Module(s) | Functions | Description |
|--------|-----------|-----------|-------------|
| Graphics | `gu`, `ge`, `display` | ~150 | GU/GUM 3D rendering, GPU command queue, display mode/vsync |
| Input | `ctrl` | 12 | Buttons, D-pad, analog stick |
| Audio | `audio` | 26 | PCM output/input, channel management, volume |
| ATRAC3 | `atrac` | 23 | Sony ATRAC3/ATRAC3plus codec |
| MP3 | `mp3` | 17 | MP3 decoder |
| MPEG | `mpeg`, `psmf` | 79 | MPEG video, PSMF stream decoding |
| JPEG | `jpeg` | 5 | JPEG image decoding |
| File I/O | `io` | 36 | Open/read/write/close, directories, async I/O |
| Networking | `net` | 150 | Sockets, TCP/UDP, DNS, HTTP, DHCP |
| WLAN | `wlan` | 5 | Wireless LAN module control |
| USB | `usb` | 45 | USB device, storage, camera |
| Power | `power` | 31 | Battery, clock frequency, power callbacks |
| RTC | `rtc` | 36 | Real-time clock, time conversion |
| Font | `font` | 28 | Font loading, glyph rendering, metrics |
| Registry | `registry` | 16 | PSP system registry access |
| UMD | `umd` | 14 | UMD disc drive control |
| Utility | `utility` | 43 | Save data, message dialog, OSK, browser |
| Kernel/Threading | `kernel` | 188 | Threads, semaphores, mutexes, events, callbacks, memory |
| GE | `ge` | 18 | Graphics Engine low-level command queue |
| HPRM | `hprm` | 6 | Headphone remote accessory |
| OpenPSID | `openpsid` | 1 | Console unique ID |
| **Kernel-only** | `nand` | 12 | NAND flash read/write/status |
| **Kernel-only** | `sircs` | 1 | Infrared remote control (SIRCS protocol) |
| **Kernel-only** | `codec` | 10 | Hardware video/audio codec control |

### High-Level Utilities

| Module | Description |
|--------|-------------|
| `psp::vram_alloc` | VRAM bump allocator with `Result` error handling |
| `psp::embedded_graphics` | `DrawTarget` impl for the `embedded-graphics` crate |
| `psp::screenshot_bmp()` | Capture framebuffer to BMP |
| `psp::benchmark()` | Cycle-accurate benchmarking via RTC |
| `psp::math` | VFPU-accelerated `sinf`/`cosf`, full libm math library |
| `psp::vfpu!()` | Inline VFPU (Vector FPU) assembly macros |
| `psp::hw` | Memory-mapped hardware register I/O (kernel mode) |
| `psp::me` | Media Engine coprocessor boot/task management (kernel mode) |
| `psp::dprintln!()` | Thread-safe debug printing via `SpinMutex` |

## Features

| Feature | Description |
|---------|-------------|
| `kernel` | Kernel mode module support -- enables `module_kernel!()` macro, NAND/SIRCS/codec syscalls, ME coprocessor control, and hardware register access. Requires custom firmware. |
| `std` | Experimental standard library support -- `String`, `Vec`, `std::fs`, `std::thread`, `std::sync`, `std::time`, `println!()` on real hardware. Build with `RUST_PSP_BUILD_STD=1`. |
| `embedded-graphics` | Enables the `Framebuffer` display driver for the `embedded-graphics` ecosystem. |
| `stub-only` | Compile as a stub provider (static library for external projects). |

## Examples

| Example | APIs Demonstrated | Description |
|---------|-------------------|-------------|
| `hello-world` | `dprintln!`, `enable_home_button` | Minimal PSP program |
| `cube` | `sceGu*`, `sceGum*`, VRAM alloc | Rotating 3D cube with lighting |
| `rainbow` | `sceGu*`, vertex colors | Animated color gradient |
| `gu-background` | `sceGu*`, VRAM alloc | Clear screen with solid color |
| `gu-debug-print` | `sceGu*`, debug font | On-screen debug text via GU |
| `clock-speed` | `scePower*` | Read/set CPU and bus clock speeds |
| `time` | `sceRtc*` | Read and display real-time clock |
| `wlan` | `sceWlan*` | Query WLAN module status |
| `msg-dialog` | `sceUtility*` | System message dialog |
| `embedded-graphics` | `Framebuffer`, `tinybmp` | Draw BMP images via embedded-graphics |
| `paint-mode` | `Framebuffer`, `sceCtrl*` | Touch-style paint app with D-pad |
| `fontdue-scrolltext` | `fontdue`, framebuffer | TrueType font rendering and scrolling |
| `ratatui` | `ratatui`, `mousefood` | TUI framework on PSP display |
| `vfpu-addition` | `vfpu!()` macro | VFPU vector addition |
| `vfpu-context-switching` | `vfpu!()`, threads | VFPU context save/restore across threads |
| `rust-std-hello-world` | `String`, `Vec`, `std` | Standard library on PSP |
| `kernel-mode` | `module_kernel!()`, NAND, volatile mem | Kernel-mode APIs (requires CFW) |
| `file-io` | `sceIoOpen/Write/Read/Close` | File write and read-back |
| `screenshot` | `screenshot_bmp()`, `sceIoWrite` | Capture framebuffer to BMP file |
| `audio-tone` | `sceAudioChReserve`, `sceAudioOutputBlocking` | Generate and play a sine wave |

## Kernel Mode

Kernel mode unlocks privileged PSP APIs for custom firmware homebrew. Enable it by adding the `kernel` feature and using `module_kernel!()`:

```toml
[dependencies]
psp = { git = "https://github.com/AndrewAltimit/rust-psp", features = ["kernel"] }
```

```rust
#![no_std]
#![no_main]

psp::module_kernel!("MyKernelApp", 1, 0);

fn psp_main() {
    psp::enable_home_button();
    unsafe {
        let me_freq = psp::sys::scePowerGetMeClockFrequency();
        psp::dprintln!("ME clock: {}MHz", me_freq);
    }
}
```

### What Kernel Mode Enables

| Module | APIs | Description |
|--------|------|-------------|
| `psp::sys::nand` | `sceNand*` | NAND flash read/write/status |
| `psp::sys::sircs` | `sceSircsSend` | Infrared remote control (SIRCS) |
| `psp::sys::codec` | `sceVideocodec*`, `sceAudiocodec*` | Hardware video/audio codecs |
| `psp::sys::power` | `scePowerGet/SetMeClockFrequency` | Media Engine clock control |
| `psp::me` | `me_boot`, `me_alloc`, `to_uncached` | Media Engine coprocessor boot/task management |
| `psp::hw` | `hw_read32`, `hw_write32`, `Register<T>` | Memory-mapped I/O register access |
| `psp::sys::kernel` | `sceKernelRegister*ExceptionHandler` | CPU exception handler registration |
| `psp::sys::kernel` | `sceKernelVolatileMem*` | Extra 4MB RAM (PSP-2000+) |
| `psp::sys::kernel` | `sceKernelAllocPartitionMemory` | ME/kernel memory partitions |

### Requirements

- **Custom firmware**: ARK-4, PRO CFW, ME CFW, or similar
- **Firmware version**: 6.60/6.61 (standard CFW base)
- Kernel-mode EBOOTs run from `ms0:/PSP/GAME/` like user-mode EBOOTs

See `examples/kernel-mode/` for a complete example.

## Standard Library Support

This fork adds experimental `std` support for PSP, allowing use of `String`, `Vec`, `std::fs`, `std::thread`, `std::sync`, `std::time`, and `println!()` on real hardware.

### Quick Start

```rust
#![feature(restricted_std)]
#![no_main]

psp::module!("rust_std_hello_world", 1, 1);

fn psp_main() {
    psp::enable_home_button();

    let greeting = String::from("Hello from std!");
    psp::dprintln!("{}", greeting);

    let people = vec!["sajattack", "overdrivenpotato", "iridescence"];
    for person in people {
        psp::dprint!("Hello, {}!\n", person);
    }
}
```

Enable `std` in your `Cargo.toml`:

```toml
[dependencies]
psp = { git = "https://github.com/AndrewAltimit/rust-psp", features = ["std"] }
```

Build with `RUST_PSP_BUILD_STD=1`:

```bash
RUST_PSP_BUILD_STD=1 cargo +nightly psp --release
```

### How It Works

PSP is not a supported Rust target, so `std` falls through to the `unsupported` PAL by default. This fork provides a PSP-specific Platform Abstraction Layer (PAL) via a sysroot overlay:

1. **`rust-std-src/`** contains PSP PAL source files that implement std's internal interfaces using PSP syscalls
2. **At build time**, `cargo-psp` creates a merged sysroot by copying the installed `rust-src` component and overlaying PSP files on top
3. **`-Z build-std`** rebuilds std from the merged source, producing a PSP-aware standard library
4. **`psp/src/std_support/`** exports `#[no_mangle] extern "C"` FFI bridge functions that the PAL calls at link time

### What's Supported

| Module | Status | Implementation |
|--------|--------|---------------|
| `String`, `Vec`, collections | Working | PSP kernel memory allocator |
| `std::thread` | Working | `sceKernelCreateThread` / `sceKernelStartThread` |
| `std::sync::Mutex` | Working | PSP lightweight mutexes (`sceKernelLwMutex*`) |
| `std::sync::Condvar` | Working | PSP event flags (`sceKernelEventFlag*`) |
| `std::time::Instant` | Working | `sceKernelGetSystemTimeWide` (microsecond precision) |
| `std::time::SystemTime` | Working | `sceRtcGetCurrentTick` |
| `std::fs` | Working | `sceIo*` (read/write files on `ms0:/`, `host0:/`, etc.) |
| `std::io::stdout/stderr` | Working | `sceKernelStdout()` / `sceKernelStderr()` file descriptors |
| `std::os::psp::raw::RawFd` | Working | PSP `SceUid` as raw file descriptor |
| `std::net`, `std::process` | Unsupported | Returns `io::ErrorKind::Unsupported` |

### Important Notes

- User crates must use `#![feature(restricted_std)]` and `#![no_main]`
- Entry point is `fn psp_main()` (not `fn main()`), declared via `psp::module!()`
- PSP is single-core MIPS with preemptive threading -- `available_parallelism()` returns 1
- File paths use PSP device notation: `ms0:/PSP/GAME/...`, `host0:/...`

See `examples/rust-std-hello-world/` for a complete example.

## Usage

To use the `psp` crate in your own project, add it as a git dependency:

```toml
[dependencies]
psp = { git = "https://github.com/AndrewAltimit/rust-psp", branch = "main" }
```

In your `main.rs` file, set up a basic skeleton:

```rust
#![no_std]
#![no_main]

psp::module!("sample_module", 1, 0);

fn psp_main() {
    psp::enable_home_button();
    psp::dprintln!("Hello PSP from rust!");
}
```

Run `cargo +nightly psp` to build your `EBOOT.PBP` file, or
`cargo +nightly psp --release` for a release build.

Customize your EBOOT with a `Psp.toml` in your project root (all keys optional):

```toml
title = "XMB title"
xmb_icon_png = "path/to/24bit_144x80_image.png"
xmb_background_png = "path/to/24bit_480x272_background.png"
xmb_music_at3 = "path/to/ATRAC3_audio.at3"
```

More options can be found in the schema definition [here](cargo-psp/src/main.rs#L18-L100).

## What's Different from Upstream

### Edition 2024 and Toolchain

- Workspace and all crates updated to Rust edition 2024
- All `#[no_mangle]` and `#[link_section]` attributes updated to `#[unsafe(no_mangle)]` / `#[unsafe(link_section)]` syntax
- Re-exported `paste::paste` for `$crate::paste!` macro resolution in edition 2024
- Workspace lints configured (`unsafe_op_in_unsafe_fn = "warn"`, clippy lints)
- Removed 4 stabilized nightly features (`global_asm`, `const_loop`, `const_if_match`, `panic_info_message`)

### Safety Fixes

- **C runtime intrinsics (CRITICAL):** Reverted `memset`/`memcpy`/`memmove` implementations to manual byte loops. LLVM lowers `core::ptr::write_bytes`/`copy`/`copy_nonoverlapping` back to C memset/memcpy/memmove calls, causing infinite recursion when those functions ARE the implementation. On MIPS this manifests as "jump to invalid address", not a stack overflow.
- **Use-after-free in test_runner:** Fixed `psp_filename()` returning a pointer to a dropped `String` -- the format buffer now outlives the syscall.
- **Thread-unsafe panic counter:** Replaced `static mut PANIC_COUNT` with `AtomicUsize` for safe concurrent access.
- **Allocator overflow checks:** Added `checked_add` for size + alignment calculations in `SystemAlloc::alloc` to prevent integer overflow.
- **OOM diagnostic:** Added explicit "out of memory" message before spin loop in the allocation error handler.
- **Global allow scoping:** Removed blanket `#![allow(unsafe_op_in_unsafe_fn)]` from crate root; scoped allows only where needed in `debug.rs`, `sys/mod.rs`, `panic.rs`.
- **Screenshot BmpHeader:** Replaced `core::mem::transmute` with safe field-by-field LE byte serialization.
- **libunwind malloc/free shims:** Overflow-safe `malloc` with `checked_add` and validated `Layout`; null-safe `free`; uses `size_of::<usize>()` instead of hardcoded `4` for pointer-width portability.

### Safety Hardening (Refactor Rounds 1-3)

- **GU matrix stack:** Fixed `sceGumMatrixMode` writing out of bounds with unchecked `current_mode` index -- added bounds check returning early on invalid mode.
- **GU projection matrix:** Fixed `sceGumOrtho` using wrong variable (`delta_x` instead of `delta_y`) for Y-axis scaling.
- **Allocator double-free guard:** `SystemAlloc::dealloc` now checks freed pointers against allocation base range before dereferencing the stored size.
- **VRAM allocator alignment:** Fixed `alloc()` using `checked_add` after alignment calculation to prevent silent wraparound.
- **Eliminated UB in GU:** Replaced raw pointer arithmetic with `addr_of_mut!` and safe indexing throughout `gu.rs`.
- **Panic handler hardening:** `AtomicUsize` ordering upgraded from `Relaxed` to `SeqCst` for cross-thread panic visibility.

### VRAM Allocator

- Changed `alloc()` from panicking to returning `Result<VramMemChunk, VramAllocError>`
- Added structured error types: `OutOfMemory { requested, available }`, `UnsupportedPixelFormat`, and `Overflow`
- VRAM base address now uses `sceGeEdramGetAddr()` instead of hardcoded constants
- Replaced `static mut VRAM_ALLOCATOR` singleton with atomic take pattern (`AtomicBool` guard)
- Added `checked_add` in `alloc()` and `checked_mul` in `alloc_sized()` to prevent integer overflow

### Hardware Constants

- Extracted magic numbers into `psp/src/constants.rs`: `SCREEN_WIDTH`, `SCREEN_HEIGHT`, `BUF_WIDTH`, `VRAM_BASE_UNCACHED`, thread priorities, NID values
- Module macros and `enable_home_button()` use named constants instead of raw numbers

### Thread-Safe Debug Printing

- `dprintln!`/`dprint!` macros now use a `SpinMutex` (atomic spinlock) to protect the character buffer
- Eliminates `static mut` data race for multi-threaded PSP homebrew
- Zero overhead on single-core PSP (compiler barrier only, no bus contention)

### Error Handling (cargo-psp)

- All tool binaries (`prxgen`, `pack-pbp`, `mksfo`, `prxmin`, `cargo-psp`) refactored from `unwrap()`/`panic!()` to `Result` with `anyhow` context
- `fix_imports.rs`: stub position lookup validated with descriptive error for malformed PRX files
- `build.rs`: replaced unwraps with fallible error handling
- `cargo-psp` main: cargo message parse errors handled gracefully; title fallback has descriptive error
- Descriptive error messages with recovery hints

### Features

- `kernel` feature flag added for kernel mode module support (`PSP_MODULE_INFO` flag `0x1000`)
- `std` support via sysroot overlay and FFI bridge (see [Standard Library Support](#standard-library-support))
- `libm` dependency added for floating-point math in `no_std`

### Upstream Issues Fixed

| Upstream Issue | Description | Fix |
|---------------|-------------|-----|
| [#120](https://github.com/overdrivenpotato/rust-psp/issues/120) | VRAM allocator panics | Result API + atomic singleton + overflow checks |
| [#126](https://github.com/overdrivenpotato/rust-psp/issues/126) | clippy warnings in cargo-psp | Full anyhow refactor |
| [#156](https://github.com/overdrivenpotato/rust-psp/issues/156) | Excessive nightly features | 4 stabilized features removed |
| [#75](https://github.com/overdrivenpotato/rust-psp/issues/75) | memcpy/memset improvements | Idiomatic ptr methods + documented footgun |
| [#165](https://github.com/overdrivenpotato/rust-psp/issues/165) | Panic/exception support | Hardened malloc/free shims |

## Running Examples

Enter one of the example directories, `examples/hello-world` for instance, and
run `cargo psp`.

This will create an `EBOOT.PBP` file under `target/mipsel-sony-psp/debug/`

Assuming you have a PSP with custom firmware installed, you can simply copy this
file into a new directory under `PSP/GAME` on your memory stick, and it will
show up in your XMB menu.

```
.
+-- PSP
    +-- GAME
        +-- hello-world
            +-- EBOOT.PBP
```

If you do not have a PSP, you can test with PPSSPP:

```bash
# Build your EBOOT
cd examples/hello-world
cargo +nightly psp --release

# Run in PPSSPP (install from https://ppsspp.org)
ppsspp target/mipsel-sony-psp/release/EBOOT.PBP
```

Note that graphics code is very sensitive -- if you're writing graphics code we
recommend developing on real hardware. PPSSPP is more relaxed in some aspects.

## Debugging

Using psplink and psp-gdb from the [pspdev github organization](https://github.com/pspdev) (`psplinkusb v3.1.0 and GNU gdb (GDB) 11.0.50.20210718-git` or later), Rust types are fully supported. Enable debug symbols in your release binaries:

```toml
[profile.release]
debug = true
```

Follow the instructions in part 6 of [the PSPlink manual](https://usermanual.wiki/Document/psplinkmanual.1365336729/).

## Troubleshooting

### `error[E0460]: found possibly newer version of crate ...`

```
error[E0460]: found possibly newer version of crate `panic_unwind` which `psp` depends on
```

Clean your target directory:

```sh
cargo clean
```

## CI/CD

All CI runs on a self-hosted GitHub Actions runner shared with [template-repo](https://github.com/AndrewAltimit/template-repo). Rust compilation and testing execute inside Docker containers for reproducibility; AI agent tooling runs directly on the host where it is pre-installed.

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| `ci.yml` | push to main | Basic CI: fmt, clippy, test, build, cargo-deny, PSP emulator test |
| `pr-validation.yml` | pull request | Full PR pipeline: CI + Gemini/Codex AI reviews + agent auto-fix |
| `main-ci.yml` | push to main, `v*` tags | CI on main, build release binaries and create GitHub Release on tags |

### CI Stages

All stages run inside the `rust-ci` Docker container (`docker compose --profile ci`):

1. **Format check** -- `cargo fmt --check` (stable for cargo-psp, nightly for psp workspace)
2. **Clippy** -- `cargo clippy -D warnings` (cargo-psp, host target)
3. **Unit tests** -- `cargo test` (cargo-psp)
4. **Build** -- release build of cargo-psp + CI test EBOOT
5. **cargo-deny** -- license and advisory checks for both workspaces
6. **PSP emulator test** -- run test EBOOT in PPSSPPHeadless (Docker)

### PR Review Pipeline

PRs receive automated AI code reviews from Gemini and Codex, followed by an agent that can automatically apply fixes from review feedback (with a 5-iteration safety limit per agent type). If CI stages fail, a separate failure-handler agent attempts automated fixes.

### Runner Dependencies from template-repo

The self-hosted runner provides the following binaries built from [template-repo](https://github.com/AndrewAltimit/template-repo). These are expected to be on `PATH`; workflows degrade gracefully if they are missing.

| Binary | Source | Used By | Purpose |
|--------|--------|---------|---------|
| `github-agents` | `tools/rust/github-agents-cli` | `pr-validation.yml` | PR reviews (Gemini/Codex), iteration tracking |
| `automation-cli` | `tools/rust/automation-cli` | `pr-validation.yml` | Agent review response, failure handler |

These binaries are also available from [template-repo releases](https://github.com/AndrewAltimit/template-repo/releases).

### Secrets

| Secret | Required By | Purpose |
|--------|-------------|---------|
| `GITHUB_TOKEN` | all workflows | Standard GitHub token (automatic) |
| `AGENT_TOKEN` | `pr-validation.yml` | Personal access token for agent commits (write access) |
| `GOOGLE_API_KEY` | `pr-validation.yml` | Gemini API key for AI code reviews |
| `GEMINI_API_KEY` | `pr-validation.yml` | Gemini API key (alternative) |

### Release Pipeline

Tagging a commit with `v*` (e.g., `v0.1.0`) triggers a release build:

1. Full CI validation
2. Containerized release build of all cargo-psp binaries
3. GitHub Release creation with binaries attached and auto-generated changelog

## Structure

```
rust-psp/
+-- psp/                # Core PSP crate (sceGu, sceCtrl, sys bindings, vram_alloc)
+-- cargo-psp/          # Build tool: cross-compile + prxgen + pack-pbp -> EBOOT.PBP
+-- rust-std-src/       # PSP PAL overlay for std support (merged with rust-src at build time)
+-- examples/           # Sample programs (hello-world, cube, gu-background, etc.)
+-- ci/                 # CI test harness, std verification, PPSSPPHeadless Dockerfile
+-- docker/             # Docker images (rust-ci)
+-- .github/            # GitHub Actions workflows and composite actions
+-- deny.toml           # cargo-deny license and advisory checks
```

### Docker Images

The repo includes two locally-built Docker images for CI and nine pre-built MCP images for AI agent tooling:

| Image | Dockerfile | Built From |
|-------|-----------|------------|
| `rust-ci` | `docker/rust-ci.Dockerfile` | This repo |
| `ppsspp` | `ci/Dockerfile-ppsspp` | This repo |
| `template-repo-mcp-code-quality` | `docker/mcp-code-quality.Dockerfile` | [template-repo](https://github.com/AndrewAltimit/template-repo) |
| `template-repo-mcp-content-creation` | `docker/mcp-content.Dockerfile` | template-repo |
| `template-repo-mcp-gemini` | `docker/mcp-gemini.Dockerfile` | template-repo |
| `template-repo-mcp-opencode` | `docker/mcp-opencode.Dockerfile` | template-repo |
| `template-repo-mcp-crush` | `docker/mcp-crush.Dockerfile` | template-repo |
| `template-repo-mcp-codex` | `docker/codex.Dockerfile` | template-repo |
| `template-repo-mcp-github-board` | `docker/mcp-github-board.Dockerfile` | template-repo |
| `template-repo-mcp-agentcore-memory` | `docker/mcp-agentcore-memory.Dockerfile` | template-repo |
| `template-repo-mcp-reaction-search` | `mcp_reaction_search/Dockerfile` | template-repo |

The MCP images are referenced as `image: template-repo-mcp-<name>:latest` in `docker-compose.yml`. They are **not buildable from this repo** -- source code lives in template-repo under `tools/mcp/`. Build them once from a template-repo checkout:

```bash
cd /path/to/template-repo
docker compose --profile services build
```

The images will then be available locally for this repo's `docker compose --profile services` commands. CI workflows and PSP development work without the MCP images -- they are only needed for interactive AI agent sessions (Claude Code, Codex, etc.).

## Pre-built Binaries

Pre-built binaries for all toolchain components are available from [GitHub Releases](../../releases). Each release includes:

| Binary | Description |
|--------|-------------|
| `cargo-psp` | Cargo subcommand for building PSP homebrew (EBOOT.PBP) |
| `prxgen` | PRX generator for PSP modules |
| `pack-pbp` | PBP archive packer |
| `mksfo` | SFO metadata file generator |
| `prxmin` | PRX minimizer/stripper |

```bash
# Download from the latest release and install
chmod +x cargo-psp-linux-* prxgen-linux-* pack-pbp-linux-* mksfo-linux-* prxmin-linux-*
cp cargo-psp-linux-* ~/.cargo/bin/cargo-psp
cp prxgen-linux-* ~/.cargo/bin/prxgen
cp pack-pbp-linux-* ~/.cargo/bin/pack-pbp
cp mksfo-linux-* ~/.cargo/bin/mksfo
cp prxmin-linux-* ~/.cargo/bin/prxmin
```

Binaries are built in Docker containers via the `main-ci.yml` GitHub Actions workflow and attached to releases on `v*` tags.

## Dependencies

Rust **nightly** toolchain with the `rust-src` component:

```sh
rustup default nightly && rustup component add rust-src
```

### Building from Source

If you prefer to build the toolchain from source instead of using pre-built binaries:

```sh
cd cargo-psp && cargo build --release
# Binaries at: target/release/{cargo-psp,prxgen,pack-pbp,mksfo,prxmin}
```

Or use it directly via `cargo run`:

```sh
cd /path/to/your/psp/project
cargo +nightly psp --release
```

**Do NOT run `cargo install cargo-psp`** -- this would install the upstream version from crates.io, not this fork. Use the local `cargo-psp/` directory or download pre-built binaries from [Releases](../../releases).

## Upstream

This project is a completely new SDK with no dependency on the original C/C++
PSPSDK. It aims to be a complete replacement with more efficient implementations
of graphics functions and the addition of missing libraries.

Upstream repository: [github.com/overdrivenpotato/rust-psp](https://github.com/overdrivenpotato/rust-psp)

### Roadmap

- [x] `core` support
- [x] PSP system library support
- [x] `alloc` support
- [x] `panic = "unwind"` support
- [x] Macro-based VFPU assembler
- [x] Full 3D graphics support
- [x] No dependency on PSPSDK / PSPToolchain
- [x] Full parity with user mode support in PSPSDK
- [x] Port definitions to `libc` crate
- [x] Kernel mode module support (`kernel` feature flag)
- [x] Add `std` support
- [ ] Automatically sign EBOOT.PBP files to run on unmodified PSPs
