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

# PSP integration tests (requires Docker)
docker compose --profile ci run --rm rust-ci cargo +nightly psp --release  # in ci/tests
docker compose --profile psp run --rm ppsspp /roms/debug/test_cases.EBOOT.PBP --timeout=10
# Success: "FINAL_SUCCESS" appears in psp_output_file.log
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

The core SDK. A `#![no_std]` crate providing:

- **`src/lib.rs`**: Entry point macros `module!()` and `module_kernel!()`. These generate `module_start`/`module_stop` functions and `.rodata.sceModuleInfo` sections.
- **`src/sys/`**: All PSP OS syscall bindings (~15K lines). Wrapped via the `psp_extern!` macro which generates MIPS syscall stubs. Major modules: `gu.rs` (graphics, 107KB), `net.rs` (networking), `kernel/` (privileged APIs).
- **`src/vfpu.rs`**: VFPU (Vector FPU) inline assembly macros (169KB). MIPS-specific.
- **`src/vram_alloc.rs`**: VRAM allocator returning `Result<VramMemChunk, VramAllocError>` (no panics).
- **`src/debug.rs`**: Thread-safe debug printing via `SpinMutex`.
- **`src/panic.rs`**: Panic handler with unwinding support. Uses `AtomicUsize` for panic counting.
- **`src/alloc_impl.rs`**: Global allocator using PSP kernel memory partitions.
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
- `.lib.ent.top/.bottom` - Export table boundaries
- `.lib.stub.top/.bottom` - Import stub boundaries

The `psp_extern!` macro generates syscall stubs with NIDs (numeric IDs) that the PSP OS resolves at load time.

### Kernel Mode (`--features kernel`)

Enables privileged PSP APIs: kernel memory partitions, interrupt handlers, PRX module loading, volatile memory access. Uses `module_kernel!()` macro. Commented-out modules (`codec.rs`, `nand.rs`, `sircs.rs`) are planned for activation in kernel mode.

## Key Conventions

- **Edition 2024 syntax**: Use `#[unsafe(no_mangle)]`, `#[unsafe(link_section = "...")]` (not the older `#[no_mangle]`).
- **No panics in allocators/VRAM**: Return `Result` types with structured errors.
- **Manual memcpy/memset**: The codebase uses manual byte-loop implementations to avoid LLVM recursion when `core::ptr` operations compile to C intrinsics on MIPS.
- **Unsafe scoping**: `unsafe_op_in_unsafe_fn` is a workspace-level warning. Scope `#[allow]` narrowly, don't use blanket allows.
- **MSRV**: 1.91.0 (uses `str::floor_char_boundary` stabilized in 1.91).
- **Rustfmt**: Max width 100, edition 2024, Unix newlines.
- **Clippy**: Cognitive complexity limit 25, function line limit 100.

## CI/CD

Three GitHub Actions workflows:
- **`ci.yml`**: Push to main — fmt, clippy, test, build, deny, emulator test
- **`main-ci.yml`**: Main + version tags — full CI plus release binary creation
- **`pr-validation.yml`**: PRs — full CI plus AI code reviews (Gemini/Codex), agent auto-fix (max 5 iterations)

CI runs in Docker containers (`docker/rust-ci.Dockerfile` based on `rust:1.93-slim`). PSP tests run in PPSSPPHeadless emulator container.

## Contribution Policy

No external contributions accepted. Development is AI-agent-driven under human direction. See `CONTRIBUTING.md`.
