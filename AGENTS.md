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

**Two separate workspaces exist -- this is the most common source of build errors:**
- **Root workspace** (`Cargo.toml`): Contains `psp` crate, `examples/*`, `ci/tests`, `ci/std_verification`. All build for `mipsel-sony-psp` target.
- **cargo-psp workspace** (`cargo-psp/Cargo.toml`): Build tools that run on the host. Excluded from root workspace because it targets the host architecture, not PSP.

Never add cargo-psp as a workspace member to the root `Cargo.toml`. Never run `cargo install cargo-psp` (that installs the upstream version from crates.io).

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

Enables privileged PSP APIs: kernel memory partitions, interrupt handlers, PRX module loading, volatile memory access. Uses `module_kernel!()` macro. Commented-out modules (`codec.rs`, `nand.rs`, `sircs.rs`) are planned for activation in kernel mode. See `KERNEL_MODE_PLAN.md` for the 6-phase implementation roadmap.

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
| `ci.yml` | Push to main | fmt, clippy, test, build, cargo-deny, PSP emulator test |
| `main-ci.yml` | Push to main + `v*` tags | Full CI plus release binary creation |
| `pr-validation.yml` | Pull requests | Full CI + Gemini/Codex AI reviews + agent auto-fix |

CI runs in Docker containers (`docker/rust-ci.Dockerfile` based on `rust:1.93-slim`). PSP tests run in PPSSPPHeadless emulator container.

### CI Stage Order

All stages run inside the `rust-ci` Docker container (`docker compose --profile ci`):

1. **Format check** -- `cargo fmt --check` (stable for cargo-psp, nightly for psp workspace)
2. **Clippy** -- `cargo clippy -D warnings` (cargo-psp, host target)
3. **Unit tests** -- `cargo test` (cargo-psp)
4. **Build** -- release build of cargo-psp + CI test EBOOT
5. **cargo-deny** -- license and advisory checks for both workspaces
6. **PSP emulator test** -- run test EBOOT in PPSSPPHeadless (Docker)

### PR Pipeline for Agent Commits

When an agent opens or pushes to a PR targeting `main`:

1. **Fork guard** blocks fork PRs from using self-hosted runners
2. **CI** runs all 6 stages above
3. **Gemini AI review** posts code review comments (via `github-agents pr-review`)
4. **Codex AI review** posts secondary code review (via `github-agents pr-review --agent codex`)
5. **Agent review response** reads Gemini/Codex feedback and auto-applies fixes (via `automation-cli review respond`, max 5 iterations)
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
| `github-agents` | `pr-validation.yml` | PR reviews (Gemini/Codex), iteration tracking |
| `automation-cli` | `pr-validation.yml` | Agent review response, failure handler |

## Local Agent Tooling

### Running Agents Locally

Scripts in `tools/cli/agents/` launch each agent:

| Script | Agent | Notes |
|--------|-------|-------|
| `run_claude.sh` | Claude Code | Requires NVM + Node.js 22.16.0 |
| `run_gemini.sh` | Gemini CLI | Requires `@google/gemini-cli` |
| `run_codex.sh` | Codex CLI | Requires `@openai/codex` + `codex login` |
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
| `mcp-codex` | Codex AI consultation |
| `mcp-github-board` | GitHub Projects board management |
| `mcp-agentcore-memory` | Agent memory (ChromaDB backend) |
| `mcp-reaction-search` | Reaction image search |

MCP images are pre-built from template-repo. Build them there first: `docker compose --profile services build`.

## Common Pitfalls

- **Do not use `core::ptr::write_bytes`/`copy`/`copy_nonoverlapping` in C runtime intrinsic implementations.** LLVM lowers these back to memset/memcpy/memmove calls, causing infinite recursion on MIPS.
- **The two workspaces have different toolchain requirements.** cargo-psp builds with stable Rust; the psp workspace requires nightly with `rust-src`.
- **Graphics code behaves differently on PPSSPP vs real hardware.** PPSSPP is more permissive. Test on real PSP hardware when possible for graphics-related changes.
- **`cargo deny check` requires separate runs for each workspace** since they have independent Cargo.lock files.
- **The `ci/tests` crate uses edition 2018** (not 2024 like the rest of the workspace). This is intentional.
