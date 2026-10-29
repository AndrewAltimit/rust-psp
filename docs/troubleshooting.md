# Running, Customizing, Debugging & Troubleshooting

## Running on hardware or PPSSPP

Copy the `EBOOT.PBP` to a memory stick on a PSP running custom firmware:

```
ms0:/PSP/GAME/<your-app>/EBOOT.PBP
```

Or run it in [PPSSPP](https://ppsspp.org): `ppsspp target/mipsel-sony-psp/release/EBOOT.PBP`.
PPSSPP is more forgiving than hardware, especially for graphics code, so
test on real hardware when you can. The networking examples need real
Wi-Fi.

## Customizing the EBOOT (`Psp.toml`)

A `Psp.toml` next to `Cargo.toml` sets XMB metadata. All keys are optional:

```toml
title = "XMB title"
xmb_icon_png = "path/to/24bit_144x80_image.png"
xmb_background_png = "path/to/24bit_480x272_background.png"
xmb_music_at3 = "path/to/ATRAC3_audio.at3"
```

The other keys are `xmb_icon_pmf`, `xmb_background_overlay_png`, `psar`,
`disc_id`, `disc_version`, `language`, `parental_level`, `psp_system_ver`,
`region`, `title_{jp,fr,es,de,it,nl,pt,ru}`, and `updater_version`. See
[`PspConfig` in cargo-psp/src/main.rs](../cargo-psp/src/main.rs#L18-L101).

## Debugging

With [psplink and psp-gdb](https://github.com/pspdev) (`psplinkusb v3.1.0`
and GNU gdb 11.0.50.20210718-git or later), Rust types work in the debugger.
Keep debug symbols in release builds:

```toml
[profile.release]
debug = true
```

Then follow part 6 of the
[PSPLink manual](https://usermanual.wiki/Document/psplinkmanual.1365336729/).

## Common problems

| Symptom | Fix |
|---------|-----|
| Errors in `psp` such as `no sqrtf32 in intrinsics`, `cannot find truncf32`, or `no PanicPayload in panic` | Your nightly is newer than the SDK supports. Pin a known-good nightly in `rust-toolchain.toml` (see [Quick start](../README.md#1-toolchain)). |
| `std` build errors inside `library/std` (e.g. `BorrowedCursor`, E0107) | The nightly doesn't match the `rust-std-src` overlay. Pin the matching nightly. |
| `could not find repository root (looking for rust-std-src/ directory)` | Your `cargo-psp` predates the overlay lookup; reinstall it from the same rust-psp revision as `psp`. See [Using `std` from your own project](std.md#using-std-from-your-own-project). |
| `failed to run prxgen` / `mksfo` / `pack-pbp` | The helper binaries aren't on `PATH`. Install all of them, not just `cargo-psp`. |
| `cargo-psp requires a nightly rustc version` | Use nightly (`rust-toolchain.toml`, or `cargo +nightly psp`). |
| `error[E0460]: found possibly newer version of crate ...` | Run `cargo clean`. |
| Panic or crash inside `unicode-width` (e.g. with `ratatui`) | Add the alignment workaround to your **root** `Cargo.toml`. A dependency's `[patch]` doesn't apply to you:<br>`[patch.crates-io]`<br>`unicode-width = { git = "https://git.sr.ht/~sajattack/unicode-width" }` |
