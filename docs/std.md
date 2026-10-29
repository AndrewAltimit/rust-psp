# Standard Library (`std`) Support

The `std` feature lets PSP code use `String`, `Vec`, collections,
`std::fs`, `std::thread`, `std::sync`, `std::time`, and `println!`-style I/O.
It is experimental.

```rust
#![no_main]

psp::module!("rust_std_hello_world", 1, 1);

fn psp_main() {
    psp::callback::setup_exit_callback().unwrap();
    let greeting = String::from("Hello from std!");
    psp::dprintln!("{}", greeting);
}
```

```toml
psp = { git = "https://github.com/AndrewAltimit/rust-psp", features = ["std"] }
```

A complete program is in
[`examples/rust-std-hello-world`](../examples/rust-std-hello-world/).

## How it works

PSP isn't a std-supported Rust target, so this fork ships a PSP Platform
Abstraction Layer as a source overlay:

1. [`rust-std-src/`](../rust-std-src/) contains PSP implementations of std's
   internal `sys` modules (alloc, fs, io, stdio, thread, sync, time, random,
   os).
2. When `std` is in use, `cargo-psp` copies the toolchain's `rust-src` to
   `target/psp-std-sysroot/` and overlays these files on top. It also patches
   std's `restricted_std` gate so that third-party crates using std (serde
   and similar) compile without `#![feature(restricted_std)]`, and generates
   `target/mipsel-sony-psp-std.json`.
3. `-Z build-std` rebuilds std from the merged sources.
4. `psp/src/std_support/` exports the `extern "C"` bridge functions
   (`__psp_*`) that the overlay calls.

`cargo-psp` turns this on automatically when a workspace member depends on
`psp` with the `std` feature. Set `RUST_PSP_BUILD_STD=1` to force it, for
example when `std` is enabled indirectly through another crate's feature.
Output goes to **`target/mipsel-sony-psp-std/<profile>/EBOOT.PBP`**.

## Using `std` from your own project

Depend on `psp` with the `std` feature and run `cargo psp`; nothing else is
needed:

```toml
[dependencies]
psp = { git = "https://github.com/AndrewAltimit/rust-psp", features = ["std"] }
```

`cargo-psp` looks for the overlay in this order:

1. `RUST_PSP_STD_SRC`, if set (a path to a `rust-std-src/` directory).
2. Next to the resolved `psp` package. A git or path dependency checks out
   the whole rust-psp repository, so the overlay always matches the `psp`
   revision you build against.
3. Parent directories of the current one (building inside rust-psp, or a
   project that vendors its own copy).

The build log names the overlay it used (`[NOTE]: Using PSP std overlay at
...`). The overlay is written against one nightly's internal std APIs (see
the comment in `rust-std-src/library/std/src/sys/fs/psp.rs`), so pin a
matching nightly with `rust-toolchain.toml`. `nightly-2026-04-13` (the
nightly CI pins) is verified.

Use a `cargo-psp` built from the same rust-psp revision as your `psp`
dependency. Versions before this lookup existed only searched parent
directories and fail with `could not find repository root`.

## What's supported

| Area | Status | Backed by |
|------|--------|-----------|
| `String`, `Vec`, collections, `HashMap` | Working | Shared global allocator. `HashMap` seeding uses MT19937 (`sceKernelUtilsMt19937*`) |
| `std::thread` | Working | `sceKernelCreateThread` / `StartThread` / `WaitThreadEnd` |
| `std::sync::Mutex` / `Condvar` | Working | Lightweight mutexes / event flags |
| `std::time::Instant` / `SystemTime` | Working | `sceKernelGetSystemTimeWide` / `sceRtcGetCurrentTick` |
| `std::fs` | Working | `sceIo*` (`ms0:/`, `host0:/`, ...) |
| `std::io::stdin/stdout/stderr` | Working | `sceKernelStd*` descriptors |
| `std::env::set_current_dir` | Working | `sceIoChdir` (`current_dir` is unsupported) |
| `std::process::exit` | Working | `sceKernelExitGame` |
| `std::net`, process spawning | Unsupported | Returns `ErrorKind::Unsupported` |

## Notes

- Use `#![no_main]` and `fn psp_main()` declared via `psp::module!()`, not
  `fn main()`.
- `#![feature(restricted_std)]` is normally unnecessary (the bundled examples
  still carry it, which is harmless). If `cargo-psp` prints
  `[WARN]: Could not find restricted_std attributes to patch`, add it.
- `available_parallelism()` returns 1. The PSP has one application core with
  preemptive threads.
- Paths use PSP device notation: `ms0:/PSP/GAME/...`, `host0:/...`.

std-specific build errors are covered in
[troubleshooting.md](troubleshooting.md).
