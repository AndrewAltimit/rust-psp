# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

rust-psp is a modernized Rust SDK for Sony PSP homebrew development. It targets bare-metal PSP hardware via the `mipsel-sony-psp` MIPS toolchain. Fork of `overdrivenpotato/rust-psp` with Edition 2024, kernel mode support, and safety hardening.

## Build & Development Commands

### Prerequisites
```bash
rustup default nightly
rustup component add rust-src
```
CI pins a specific nightly (`NIGHTLY_DATE` in `docker/rust-ci.Dockerfile`) because the `psp` crate and the `rust-std-src/` overlay depend on nightly internals; use the same date locally (`rustup toolchain install nightly-<date> --component rust-src`) if floating nightly fails to build.

### Building cargo-psp (host tools)
```bash
# cargo-psp is a SEPARATE workspace (not part of the main workspace)
cd cargo-psp && cargo build --release
# Produces: cargo-psp, prxgen, pack-pbp, mksfo, prxmin
```

### Building PSP examples/projects
```bash
cd examples/hello-world
cargo +nightly psp --release
# Output: target/mipsel-sony-psp/release/EBOOT.PBP
```

### Formatting
```bash
# cargo-psp (stable toolchain)
cargo fmt --manifest-path cargo-psp/Cargo.toml --all -- --check

# PSP workspace (nightly required)
cargo +nightly fmt --all -- --check
```

### Linting
```bash
# cargo-psp
cargo clippy --manifest-path cargo-psp/Cargo.toml --all-targets -- -D warnings

# PSP workspace (cross-compilation target, clippy runs via cargo-psp build)
```

### Testing
```bash
# cargo-psp unit tests (host)
cargo test --manifest-path cargo-psp/Cargo.toml

# PSP integration tests (requires Docker) -- or just: ./ci/run-ci.sh full
docker compose --profile ci run --rm rust-ci cargo build --manifest-path cargo-psp/Cargo.toml --release
docker compose --profile ci run --rm -w /app/ci/tests rust-ci \
  bash -c 'export PATH="/app/cargo-psp/target/release:$PATH" && cargo +nightly psp'   # debug build
bash ci/psp-test.sh   # runs test_cases.EBOOT.PBP in PPSSPPHeadless; fails unless the log ends in FINAL_SUCCESS
```

### License/Advisory checks
```bash
cargo deny check
cargo deny --manifest-path cargo-psp/Cargo.toml check
```

## Architecture

### Workspace Layout

**Two separate workspaces exist:**
- **Root workspace** (`Cargo.toml`): Contains `psp` crate, `examples/*`, `ci/tests`, `ci/std_verification`. All build for `mipsel-sony-psp` target.
- **cargo-psp workspace** (`cargo-psp/Cargo.toml`): Build tools that run on the host. Excluded from root workspace because it targets the host architecture, not PSP.

### PSP Crate (`psp/`)

The core SDK. `#![no_std]` unless the `std` feature is enabled. Provides:

- **`src/lib.rs`**: Entry point macros `module!()` and `module_kernel!()` (the latter only with `feature = "kernel"`). They generate `module_start` (no `module_stop`), the `.rodata.sceModuleInfo` struct, and the `.lib.ent`/`.rodata.sceResident` export table.
- **`src/sys/`**: Low-level PSP OS bindings (~19K lines). 837 firmware imports declared via `psp_extern!` (812 always available + 25 behind `kernel`), which generates import stubs with NIDs. `gu.rs`/`gum.rs` (~105KB) are a native Rust GE command-list implementation, not syscalls. Other large files: `net.rs`, `kernel/thread.rs`. `sys/gpio.rs`, `sys/syscon.rs`, `sys/sysreg.rs` are kernel driver tables resolved at runtime via `psp::hook::find_function`, not `psp_extern!`.
- **High-level modules** (`src/*.rs`): 41 safe wrappers over `sys` (34 always on, `embedded_graphics` behind its feature, `me`/`hw`/`hook`/`gpio`/`syscon`/`sysreg` behind `kernel`). Most are disabled by the `stub-only` feature.
- **`src/vfpu.rs`**: `vfpu_asm!` VFPU (Vector FPU) inline assembler macros (~169KB). MIPS-specific.
- **`src/vram_alloc.rs`**: VRAM allocator returning `Result<VramMemChunk, VramAllocError>` (no panics).
- **`src/debug.rs`**: Thread-safe debug printing via `SpinMutex`.
- **`src/panic.rs`**: Panic handler with unwinding support. Uses `AtomicUsize` for panic counting.
- **`src/alloc_impl.rs`**: Global allocator: one large partition block (8 MB user, smaller with `kernel`) managed by `linked_list_allocator`; large allocations bypass to `sceKernelAllocPartitionMemory`. Also the manual `memcpy`/`memset`/`memmove`.
- **`src/std_support/`**: `extern "C"` `__psp_*` bridge functions called by the std PAL overlay in `rust-std-src/` (only with `feature = "std"`).
- **`src/test_runner.rs`**: Test harness that outputs to file and prints "FINAL_SUCCESS".

### cargo-psp (`cargo-psp/`)

Host-side build toolchain:
- **`cargo-psp`**: Main build command (wraps cargo, links PSP ELF, creates EBOOT.PBP)
- **`prxgen`**: Converts ELF to PRX format
- **`pack-pbp`**: Creates PBP packages (PSP executable format)
- **`mksfo`**: Generates SFO metadata files
- **`prxmin`**: Minimizes PRX files
- **`fix_imports.rs`**: Fixes PRX import tables

### PSP Module System

PSP executables require specific ELF sections:
- `.rodata.sceModuleInfo` - Module metadata (flags: `0x0000` user-mode, `0x1000` kernel-mode)
- `.lib.ent.top/.btm` - Export table boundaries
- `.lib.stub.top/.btm` - Import stub boundaries

The `psp_extern!` macro generates syscall stubs with NIDs (numeric IDs) that the PSP OS resolves at load time. See `docs/prx.md` for the full section layout.

### Kernel Mode (`--features kernel`)

Compile-time gates `module_kernel!()`, NAND flash (`sys/nand.rs`), IR remote (`sys/sircs.rs`), CFW `sctrlHEN*` (`sys/sctrl.rs`), USB bus driver (`sys/usbbd.rs`), exception handler registration (`sys/kernel/mod.rs`), and the high-level `me.rs` (Media Engine boot/`MeExecutor`), `hw.rs` (MMIO), `hook.rs` (syscall hooking, `find_function`), `gpio.rs`, `syscon.rs`, `sysreg.rs`. Kernel partitions, ME clock, interrupt registration, and module loading are ungated bindings that only succeed from a kernel module. `sys/codec.rs` is NOT kernel-gated. Guide: `docs/kernel-mode.md`.

### std Support (`--features std`)

`rust-std-src/` is a PSP PAL overlay; cargo-psp merges it with the toolchain's `rust-src` into `target/psp-std-sysroot/`, patches out the `restricted_std` gate, and builds with a generated `target/mipsel-sony-psp-std.json` (output in `target/mipsel-sony-psp-std/`). The overlay tracks one nightly's internal std API -- nightly bumps usually require overlay changes. cargo-psp locates `rust-std-src/` by walking up from the cwd, so out-of-tree projects must copy it (known gap).

### Distribution

Not published on crates.io (the `psp`/`cargo-psp` names there are upstream's). Users depend on `psp = { git = "https://github.com/AndrewAltimit/rust-psp" }` and install tools via `cargo install --git https://github.com/AndrewAltimit/rust-psp cargo-psp --locked` or GitHub Releases. Never write instructions that point at crates.io.

## Key Conventions

- **Edition 2024 syntax**: Use `#[unsafe(no_mangle)]`, `#[unsafe(link_section = "...")]` (not the older `#[no_mangle]`).
- **No panics in allocators/VRAM**: Return `Result` types with structured errors.
- **Manual memcpy/memset**: The codebase uses manual byte-loop implementations to avoid LLVM recursion when `core::ptr` operations compile to C intrinsics on MIPS.
- **Unsafe scoping**: `unsafe_op_in_unsafe_fn` is a workspace-level warning. Scope `#[allow]` narrowly, don't use blanket allows.
- **MSRV**: 1.91.0 (uses `str::floor_char_boundary` stabilized in 1.91).
- **Rustfmt**: Max width 100, edition 2024, Unix newlines.
- **Clippy**: Cognitive complexity limit 25, function line limit 100.

## CI/CD

Two GitHub Actions workflows:
- **`main-ci.yml`**: Push to main + `v*` tags — full CI (fmt, clippy, test, build incl. `examples/kernel-mode` + `ci/tests`, deny, emulator test), release binaries + GitHub Release on tags/manual trigger, and GitHub Pages deploy of `site/` on main
- **`pr-validation.yml`**: PRs — full CI plus AI code reviews (Gemini), agent auto-fix (max 5 iterations)

`ci/run-ci.sh <stage|full>` runs the same stages locally. Details: `docs/maintainers.md`.

CI runs in Docker containers (`docker/rust-ci.Dockerfile` based on `rust:1.99-slim`, with nightly pinned via `NIGHTLY_DATE`). PSP tests run in a PPSSPPHeadless container (`ci/Dockerfile-ppsspp`, pinned to PPSSPP `v1.20.4` via `PPSSPP_REF`) through `ci/psp-test.sh`.

## Contribution Policy

No external contributions accepted. Development is AI-agent-driven under human direction. See `CONTRIBUTING.md`.
