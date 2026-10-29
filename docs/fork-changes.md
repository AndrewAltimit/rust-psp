# Changes from Upstream

This fork diverged from
[overdrivenpotato/rust-psp](https://github.com/overdrivenpotato/rust-psp)
(MIT) to get edition 2024 compatibility, safety hardening, kernel mode, `std`
support, and a large high-level API layer. This page summarizes the
differences. For specifics, see git history.

## Toolchain and edition

- All crates use edition 2024. Attributes are written as `#[unsafe(no_mangle)]`
  and `#[unsafe(link_section = "...")]`.
- `pastey::paste` (maintained fork of `paste`) is re-exported as `psp::paste` so that `$crate::paste!` resolves inside
  `psp_extern!`.
- Workspace lints: `unsafe_op_in_unsafe_fn = "warn"`, plus clippy's
  `clone_on_ref_ptr`, `dbg_macro`, `todo`, and `unimplemented`.
- Removed stabilized nightly features (`global_asm`, `const_loop`,
  `const_if_match`, `panic_info_message`).

## Safety fixes

- **C runtime intrinsics:** `memset`, `memcpy`, and `memmove` are manual byte
  loops. LLVM lowers `core::ptr::write_bytes`/`copy`/`copy_nonoverlapping`
  back into calls to those same functions, which recurses forever. On MIPS
  this shows up as "jump to invalid address".
- **test_runner:** `psp_filename()` no longer returns a pointer into a dropped
  `String`.
- **Panic counter:** `static mut PANIC_COUNT` was replaced with an
  `AtomicUsize`.
- **Allocator:** size and alignment arithmetic is overflow-checked. Every block
  carries a tagged header (arena or kernel block), and `dealloc` logs and
  ignores headers it doesn't recognize instead of freeing garbage. The OOM
  handler prints a message before halting.
- **Allocator design:** Rust allocations now come from a single
  `linked_list_allocator` arena (8 MB user / smaller in kernel mode) instead
  of one kernel block per allocation. The old design hung the firmware after
  roughly 1500-2000 live blocks. Large allocations bypass the arena. Details
  are in `psp/src/alloc_impl.rs`.
- **GU:** fixed an out-of-bounds write in `sceGumMatrixMode` and the wrong
  axis variable in `sceGumOrtho`. Raw pointer arithmetic was replaced with
  `addr_of_mut!` and checked indexing.
- **Screenshot:** the BMP header is serialized field by field instead of
  with `transmute`.
- **libunwind shims:** `malloc` is overflow-safe, `free` is null-safe, and
  pointer width uses `size_of::<usize>()`.
- `#![allow(unsafe_op_in_unsafe_fn)]` is no longer applied crate-wide. It is
  scoped to individual modules.
- `static mut` singletons were replaced in the kernel and dialog modules.

## VRAM allocator

- `alloc()` returns `Result<VramMemChunk, VramAllocError>` with the variants
  `OutOfMemory { requested, available }`, `UnsupportedPixelFormat`, and
  `Overflow`.
- The base address comes from `sceGeEdramGetAddr()`.
- The singleton is guarded by an `AtomicBool` take pattern instead of
  `static mut`.

## Other

- Hardware constants live in `psp/src/constants.rs`: `SCREEN_WIDTH`,
  `SCREEN_HEIGHT`, `BUF_WIDTH`, `VRAM_BASE_UNCACHED`, thread defaults, and
  module NIDs.
- `dprintln!`/`dprint!` use a `SpinMutex`-protected buffer.
- cargo-psp and its helper binaries report errors through `anyhow` instead
  of `unwrap()`/`panic!()`.
- New features: `kernel` (see [kernel-mode.md](kernel-mode.md)) and `std`
  (a sysroot overlay plus an FFI bridge, see [std.md](std.md)).
- About 40 high-level modules were added on top of `psp::sys` (audio, net,
  http, io, threads, fonts, ME, and so on).

## Upstream issues addressed

| Upstream issue | Description | Resolution here |
|----------------|-------------|-----------------|
| [#120](https://github.com/overdrivenpotato/rust-psp/issues/120) | VRAM allocator panics | `Result` API, atomic singleton, overflow checks |
| [#126](https://github.com/overdrivenpotato/rust-psp/issues/126) | clippy warnings in cargo-psp | `anyhow` refactor |
| [#156](https://github.com/overdrivenpotato/rust-psp/issues/156) | Excessive nightly features | Stabilized features removed |
| [#75](https://github.com/overdrivenpotato/rust-psp/issues/75) | memcpy/memset improvements | Kept manual loops and documented the recursion footgun |
| [#165](https://github.com/overdrivenpotato/rust-psp/issues/165) | Panic/exception support | Hardened malloc/free shims |
