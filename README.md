# rust-psp

> Rust SDK for Sony PSP homebrew. A modernized fork with edition 2024, safety
> hardening, kernel mode, experimental `std`, and a large set of high-level
> APIs.

**Forked from** [overdrivenpotato/rust-psp](https://github.com/overdrivenpotato/rust-psp) (MIT).
Upstream is maintained at a low cadence. This fork diverges substantially:
see [docs/fork-changes.md](docs/fork-changes.md).

> [!IMPORTANT]
> This fork is **not published on crates.io**. The `psp` and `cargo-psp`
> crates on crates.io belong to upstream. `cargo install cargo-psp` or
> `psp = "0.x"` will give you the upstream project, not this one. Use the
> git URLs below.

```rust
#![no_std]
#![no_main]

psp::module!("sample_module", 1, 0);

fn psp_main() {
    psp::callback::setup_exit_callback().unwrap();
    psp::dprintln!("Hello PSP from rust!");
}
```

## Quick start

### 1. Toolchain

The PSP target needs **nightly** Rust with `rust-src` (`std`/`core` are
rebuilt with `-Z build-std`). Nightly regularly breaks crates that use
compiler internals, so pin a known-good nightly in your project:

```toml
# rust-toolchain.toml (in your project root)
[toolchain]
channel = "nightly-2026-04-13"
components = ["rust-src"]
```

`nightly-2026-04-13` is the nightly CI pins and validates for `no_std` and `std` builds (`NIGHTLY_DATE` in [`docker/rust-ci.Dockerfile`](docker/rust-ci.Dockerfile)).
Recent nightlies do **not** build the `psp` crate as of this writing. Run
`cargo psp` without `+nightly`, since `+nightly` overrides the pin.

### 2. Install the build tools

`cargo-psp` wraps `cargo build`, then runs `prxgen`, `mksfo`, and `pack-pbp`
to produce an `EBOOT.PBP`. **All five binaries must be on your `PATH`.**

```bash
# a) Build from this repo with cargo (any host OS; pin with --tag v1.0.0 or --rev <sha>)
cargo install --git https://github.com/AndrewAltimit/rust-psp cargo-psp --locked
# b) Prebuilt binaries (currently linux-aarch64 only):
#    https://github.com/AndrewAltimit/rust-psp/releases
# c) From a clone, then add rust-psp/cargo-psp/target/release to PATH
git clone https://github.com/AndrewAltimit/rust-psp
cd rust-psp/cargo-psp && cargo build --release
```

Option (a) installs `cargo-psp`, `prxgen`, `pack-pbp`, `mksfo`, and `prxmin`.
The host tools build on stable or nightly.

### 3. Create a project

```toml
# Cargo.toml
[package]
name = "my-homebrew"
version = "0.1.0"
edition = "2024"

[dependencies]
psp = { git = "https://github.com/AndrewAltimit/rust-psp" }
# Pin with  tag = "v1.0.0"  or  rev = "<sha>".
# Features: "kernel", "std", "embedded-graphics", "stub-only" (see docs/api-overview.md)
```

Put the hello world above in `src/main.rs`. No `.cargo/config.toml` or target
JSON is needed: `cargo-psp` passes `--target mipsel-sony-psp` and the
`-Z build-std` flags itself.

### 4. Build and run

```bash
cargo psp            # debug   -> target/mipsel-sony-psp/debug/EBOOT.PBP
cargo psp --release  # release -> target/mipsel-sony-psp/release/EBOOT.PBP
```

Extra arguments are forwarded to `cargo build`. In a workspace with several
binaries, outputs are named `<bin>.PBP`. Copy the EBOOT to
`ms0:/PSP/GAME/<your-app>/EBOOT.PBP` on a custom-firmware PSP, or run it in
[PPSSPP](https://ppsspp.org). Debugging and common build errors:
[docs/troubleshooting.md](docs/troubleshooting.md).

An optional `Psp.toml` sets XMB title, icon, background, and music: see
[Customizing the EBOOT](docs/troubleshooting.md#customizing-the-eboot-psptoml).

## Highlights

- **[Syscall coverage](docs/api-overview.md#syscall-bindings-pspsys)**: 837
  firmware imports (812 user-mode, 25 kernel-only) plus native Rust GU/GUM graphics.
- **[High-level modules](docs/api-overview.md#high-level-modules)**: 41 safe,
  mostly RAII wrappers: threads, input, I/O, savedata, audio, H.264 on the ME,
  fonts, sprite batching, TCP/UDP/HTTP, and more.
- **[Kernel mode](docs/kernel-mode.md)** (CFW only): `module_kernel!()`, Media
  Engine tasks, syscall hooking, NAND, IR remote, MMIO, exception handlers.
- **[Experimental `std`](docs/std.md)**: `String`, `Vec`, `HashMap`, `std::fs`,
  `std::thread`, `std::sync`, `std::time` via a PSP std overlay.
- **VFPU**: the `vfpu_asm!` inline assembler, `Vec4`/`Mat4` math, context save/restore.
- **[Self-contained toolchain](docs/prx.md)**: no C/C++ PSPSDK dependency;
  `cargo-psp` builds the PRX and `EBOOT.PBP` directly.

## Examples

There are 30 examples in [`examples/`](examples/), from `hello-world` and a
textured 3D `cube` to audio, OSK, savedata, networking, kernel mode, and `std`.
Build one with `cd examples/<name> && cargo psp --release`. The full annotated
list is in [docs/examples.md](docs/examples.md).

## Documentation

Full index: [docs/README.md](docs/README.md).
- [API overview](docs/api-overview.md): feature flags, every syscall domain, every high-level module
- [Kernel mode](docs/kernel-mode.md): what the `kernel` feature unlocks, ME tasks, hooking, exceptions
- [Standard library](docs/std.md): how the `std` overlay works, out-of-tree setup, support matrix
- [Examples](docs/examples.md): all 30 examples and what they demonstrate
- [Running & troubleshooting](docs/troubleshooting.md): PPSSPP, `Psp.toml`, psplink/gdb, common errors
- [PRX section structure](docs/prx.md): ELF/PRX section layout
- [Changes from upstream](docs/fork-changes.md): what this fork changed and why
- [Maintainer & CI infrastructure](docs/maintainers.md): workspaces, local CI, releases, Docker images

## Projects using rust-psp

**[OASIS_OS](https://github.com/AndrewAltimit/oasis-os)**: an embeddable OS
framework (skinnable shell, scene-graph UI, VFS, plugins) for desktop, Unreal
Engine 5, and PSP. Its PSP backend uses `std` + `kernel` and exercises most of
the SDK, from input, fonts, and audio to the Media Engine and H.264 video.

## Repository layout

```
psp/           SDK crate: psp::sys bindings + high-level modules
cargo-psp/     Host tools (separate workspace): cargo-psp, prxgen, pack-pbp, mksfo, prxmin
rust-std-src/  PSP std overlay, merged with rust-src at build time
examples/      30 sample programs
ci/            On-device and std test harnesses, PPSSPP Dockerfile, run-ci.sh
libunwind/     Makefile + patch used to produce psp/libunwind*.a
docker/        rust-ci image
docs/          Guides (see docs/README.md)
site/          GitHub Pages site
tools/cli/     Local AI-agent launchers
.github/       Workflows and composite actions
```

Contributors need two toolchains: the root workspace targets the PSP
(nightly), and `cargo-psp/` builds for the host (stable). Development
commands (`./ci/run-ci.sh full` runs everything, containerized), CI, releases,
and the AI review pipeline are documented in
[docs/maintainers.md](docs/maintainers.md). External contributions are not
accepted: see [CONTRIBUTING.md](CONTRIBUTING.md).

## Roadmap

This SDK aims to be a complete replacement for PSPSDK, with more efficient
graphics implementations and the libraries PSPSDK is missing. Done: `core`,
`alloc`, `panic = "unwind"`, the VFPU assembler, full 3D graphics, user-mode
parity with PSPSDK, `libc` definitions, kernel mode, and experimental `std`.
Open:
- [ ] Find `rust-std-src` automatically for out-of-tree `std` projects
- [ ] Automatically sign EBOOT.PBP files to run on unmodified PSPs

## License

MIT. See [LICENSE](LICENSE). Based on
[overdrivenpotato/rust-psp](https://github.com/overdrivenpotato/rust-psp) and
its contributors.
