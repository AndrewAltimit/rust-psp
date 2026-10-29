# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

## Project Overview

rust-psp is a modernized Rust SDK for Sony PSP homebrew development. It targets bare-metal PSP hardware via the `mipsel-sony-psp` MIPS toolchain. Fork of `overdrivenpotato/rust-psp` with Edition 2024, kernel mode support, and safety hardening.

All development is AI-agent-driven under human direction. No external contributions are accepted. See `CONTRIBUTING.md`.

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

**Two separate workspaces exist -- this is the most common source of build errors:**
- **Root workspace** (`Cargo.toml`): Contains `psp` crate, `examples/*`, `ci/tests`, `ci/std_verification`. All build for `mipsel-sony-psp` target.
- **cargo-psp workspace** (`cargo-psp/Cargo.toml`): Build tools that run on the host. Excluded from root workspace because it targets the host architecture, not PSP.

Never add cargo-psp as a workspace member to the root `Cargo.toml`. Never run `cargo install cargo-psp` without `--git` (that installs the upstream version from crates.io).

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

`rust-std-src/` is a PSP PAL overlay; cargo-psp merges it with the toolchain's `rust-src` into `target/psp-std-sysroot/`, patches out the `restricted_std` gate, and builds with a generated `target/mipsel-sony-psp-std.json` (output in `target/mipsel-sony-psp-std/`). The overlay tracks one nightly's internal std API -- nightly bumps usually require overlay changes. cargo-psp locates `rust-std-src/` via `RUST_PSP_STD_SRC`, then next to the resolved `psp` package (works for git/path deps), then by walking up from the cwd (`find_std_overlay` in `cargo-psp/src/main.rs`).

### Distribution

Not published on crates.io (the `psp`/`cargo-psp` names there are upstream's). Users depend on `psp = { git = "https://github.com/AndrewAltimit/rust-psp" }` and install tools via `cargo install --git https://github.com/AndrewAltimit/rust-psp cargo-psp --locked` or GitHub Releases. Never write instructions that point at crates.io.

## Key Conventions

- **Edition 2024 syntax**: Use `#[unsafe(no_mangle)]`, `#[unsafe(link_section = "...")]` (not the older `#[no_mangle]`).
- **No panics in allocators/VRAM**: Return `Result` types with structured errors.
- **Manual memcpy/memset**: The codebase uses manual byte-loop implementations to avoid LLVM recursion when `core::ptr` operations compile to C intrinsics on MIPS. Do not replace these with `core::ptr::write_bytes`/`copy`/`copy_nonoverlapping`.
- **Unsafe scoping**: `unsafe_op_in_unsafe_fn` is a workspace-level warning. Scope `#[allow]` narrowly, never use blanket allows at the crate root.
- **Error handling in cargo-psp**: All tool binaries use `anyhow` with `.context()`. Do not use `unwrap()` or `panic!()`.
- **MSRV**: 1.91.0 (uses `str::floor_char_boundary` stabilized in 1.91).
- **Rustfmt**: Max width 100, edition 2024, Unix newlines. Configuration in `rustfmt.toml`.
- **Clippy**: Cognitive complexity limit 25, function line limit 100. Configuration in `clippy.toml`.
- **Workspace lints**: `clone_on_ref_ptr`, `dbg_macro`, `todo`, `unimplemented` are warnings.

## CI/CD Pipeline

### Workflows

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| `main-ci.yml` | Push to main + `v*` tags + manual | Full CI, release binaries + GitHub Release (tags/manual), GitHub Pages deploy of `site/` (main) |
| `pr-validation.yml` | Pull requests | Full CI + Gemini AI review + agent auto-fix (Codex disabled) |

`ci/run-ci.sh <stage|full>` runs the same stages locally. Full details: `docs/maintainers.md`.

CI runs in Docker containers (`docker/rust-ci.Dockerfile` based on `rust:1.99-slim`, with nightly pinned via `NIGHTLY_DATE`). PSP tests run in a PPSSPPHeadless container (`ci/Dockerfile-ppsspp`, pinned to PPSSPP `v1.20.4` via `PPSSPP_REF`) through `ci/psp-test.sh`.

### CI Stage Order

All stages run inside the `rust-ci` Docker container (`docker compose --profile ci`):

1. **Format check** -- `cargo fmt --check` (stable for cargo-psp, nightly for psp workspace)
2. **Clippy** -- `cargo clippy -D warnings` (cargo-psp, host target)
3. **Unit tests** -- `cargo test` (cargo-psp)
4. **Build** -- release build of cargo-psp, then `cargo psp` for `examples/kernel-mode` (kernel feature check) and `ci/tests` (test EBOOT)
5. **cargo-deny** -- license and advisory checks for both workspaces
6. **PSP emulator test** -- run test EBOOT in PPSSPPHeadless (Docker)

### PR Pipeline for Agent Commits

When an agent opens or pushes to a PR targeting `main`:

1. **Fork guard** blocks fork PRs from using self-hosted runners
2. **CI** runs all 6 stages above
3. **Gemini AI review** posts code review comments (via `github-agents pr-review`)
4. ~~**Codex AI review**~~ -- **DISABLED** (OpenAI removed due to mass surveillance / autonomous weapons concerns)
5. **Agent review response** reads Gemini feedback and auto-applies fixes (via `automation-cli review respond`, max 5 iterations)
6. **Agent failure handler** auto-fixes CI failures if CI failed (via `automation-cli review failure`, max 5 iterations)
7. **PR status summary** aggregates all results

### Iteration Limits

Agent auto-fix loops are capped at **5 iterations per agent type** (`review-fix` and `failure-fix`). The iteration counter tracks via PR comments with `agent-metadata:type=<type>` markers. An admin can comment `[CONTINUE]` to extend the limit by another 5 iterations. Add the `no-auto-fix` label to disable automated fixes entirely.

### Agent Commit Authors

Automated agent commits use these author names (recognized by the pipeline):
- `AI Review Agent`
- `AI Pipeline Agent`
- `AI Agent Bot`

### Secrets Required

| Secret | Purpose |
|--------|---------|
| `GITHUB_TOKEN` | Standard GitHub token (automatic) |
| `AGENT_TOKEN` | Personal access token for agent commits (write access) |
| `GOOGLE_API_KEY` / `GEMINI_API_KEY` | Gemini API for AI code reviews |

### Runner Dependencies

The self-hosted runner provides these binaries from [template-repo](https://github.com/AndrewAltimit/template-repo). Workflows degrade gracefully if missing.

| Binary | Used By | Purpose |
|--------|---------|---------|
| `github-agents` | `pr-validation.yml` | PR reviews (Gemini), iteration tracking |
| `automation-cli` | `pr-validation.yml` | Agent review response, failure handler |

## Local Agent Tooling

### Running Agents Locally

Scripts in `tools/cli/agents/` launch each agent:

| Script | Agent | Notes |
|--------|-------|-------|
| `run_claude.sh` / `run_claude.bat` | Claude Code | Requires NVM + Node.js 22.16.0 (`stop_claude.sh` to stop) |
| `run_gemini.sh` | Gemini CLI | Requires `@google/gemini-cli` |
| `run_opencode.sh` | OpenCode | Requires OpenRouter API key |
| `run_crush.sh` | Crush | Requires OpenRouter API key |

### MCP Services

Container-based MCP services available via `docker compose --profile services`:

| Service | Purpose |
|---------|---------|
| `mcp-code-quality` | Linting, formatting, testing, security scanning |
| `mcp-content-creation` | LaTeX, TikZ, Manim document generation |
| `mcp-gemini` | Gemini AI consultation |
| `mcp-opencode` | OpenCode AI (Qwen model via OpenRouter) |
| `mcp-crush` | Crush AI (via OpenRouter) |
| `mcp-codex` | ~~Codex AI consultation~~ **DISABLED** (security policy; moved to the `disabled-codex` profile) |
| `mcp-github-board` | GitHub Projects board management |
| `mcp-agentcore-memory` | Agent memory (ChromaDB backend) |
| `mcp-reaction-search` | Reaction image search |

MCP images are pre-built from template-repo. Build them there first: `docker compose --profile services build`.

## Common Pitfalls

- **Do not use `core::ptr::write_bytes`/`copy`/`copy_nonoverlapping` in C runtime intrinsic implementations.** LLVM lowers these back to memset/memcpy/memmove calls, causing infinite recursion on MIPS.
- **The two workspaces have different toolchain requirements.** cargo-psp builds with stable Rust; the psp workspace requires nightly with `rust-src`.
- **Graphics code behaves differently on PPSSPP vs real hardware.** PPSSPP is more permissive. Test on real PSP hardware when possible for graphics-related changes.
- **`cargo deny check` requires separate runs for each workspace** since they have independent Cargo.lock files.
- **The `ci/tests` and `ci/std_verification` crates use edition 2018** (not 2024 like the rest of the workspace). This is intentional.
- **Nightly drift breaks the build.** Newer nightlies remove compiler internals the `psp` crate uses (e.g. `core::intrinsics::sqrtf32`/`truncf32`, `core::panic::PanicPayload`) and change std internals the `rust-std-src/` overlay depends on. When bumping nightly, build both a no_std example and `examples/rust-std-hello-world`.
