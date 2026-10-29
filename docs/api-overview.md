# API Overview

What the `psp` crate provides: feature flags, the raw syscall bindings in
`psp::sys`, and the safe high-level modules built on top of them.

- [Feature flags](#feature-flags)
- [Syscall bindings (`psp::sys`)](#syscall-bindings-pspsys)
- [High-level modules](#high-level-modules)
- [Other utilities](#other-utilities)

## Feature flags

| Feature | Description |
|---------|-------------|
| `kernel` | Kernel-mode support. Enables `module_kernel!()`, NAND/SIRCS/USB-bus-driver/sctrl/exception-handler syscalls, and the `me`, `hw`, `hook`, `gpio`, `syscon`, and `sysreg` modules. Requires custom firmware. See [kernel-mode.md](kernel-mode.md). |
| `std` | Experimental `std` support (`String`, `Vec`, `std::fs`, `std::thread`, `std::sync`, `std::time`, ...). See [std.md](std.md). |
| `embedded-graphics` | `psp::embedded_graphics::Framebuffer`, a `DrawTarget` for the [embedded-graphics](https://crates.io/crates/embedded-graphics) ecosystem. |
| `stub-only` | Builds only the syscall stubs (no runtime, allocator, panic handler, or most high-level modules), as a static library for linking into non-Rust projects. |

## Syscall bindings (`psp::sys`)

There are **837 firmware imports** declared with `psp_extern!`. 812 are always
available, and 25 more require the `kernel` feature. The GU/GUM graphics
API is a native Rust implementation (about 95 `sceGu*` and 24 `sceGum*`
functions) that writes GE command lists directly, so it isn't counted as
syscalls.

| Domain | Module(s) | Functions | Notes |
|--------|-----------|-----------|-------|
| Kernel / threading | `kernel` | 190 | Threads, semaphores, mutexes, event flags, message pipes, callbacks, memory partitions, module loading, interrupts, cache, DMA (2 of these are kernel-only exception handlers) |
| Networking | `net` | 150 | `sceNet`, Inet sockets, Apctl, Resolver, Adhoc/Adhocctl/Matching, `sceHttp` (46), `sceSsl` |
| MPEG / PSMF | `mpeg`, `psmf` | 84 | `sceMpeg`, `sceMpegbase` (CSC), PSMF and PSMF player |
| USB | `usb` | 45 | USB bus, USB camera, USB storage |
| Utility | `utility` | 43 | Save data, message dialog, OSK, net config, module loading |
| File I/O | `io` | 36 | Files, directories, async I/O, devctl |
| RTC | `rtc` | 36 | Real-time clock, tick arithmetic, formatting |
| Power | `power` | 32 | Battery, CPU/bus/ME clocks, power callbacks |
| Font | `font` | 28 | `sceLibFont` PGF system fonts |
| Audio | `audio` | 26 | PCM channels, SRC output, input |
| ATRAC3 | `atrac` | 23 | ATRAC3/ATRAC3plus decoder |
| GE | `ge` | 18 | Graphics Engine command lists, EDRAM |
| MP3 | `mp3` | 17 | Hardware MP3 decoder |
| Registry | `registry` | 16 | System registry |
| Display | `display` | 14 | Display mode, framebuffer, vblank |
| UMD | `umd` | 14 | UMD drive |
| Controller | `ctrl` | 12 | Buttons, analog stick, latch |
| Codecs | `codec` | 11 | `sceVideocodec` (6) and `sceAudiocodec` (5). Works in user mode once the AVCODEC module is loaded. |
| HPRM | `hprm` | 6 | Headphone remote |
| JPEG | `jpeg` | 5 | Hardware JPEG decode |
| WLAN | `wlan` | 5 | WLAN switch/power, MAC address |
| KUBridge | `kubridge` | 2 | CFW user-to-kernel bridge (`kuKernelLoadModule`, `kuKernelGetModel`) |
| OpenPSID | `openpsid` | 1 | Console unique ID |
| **Kernel-only** | `nand` | 12 | NAND flash |
| **Kernel-only** | `usbbd` | 8 | USB bus driver (`sceUsbBus_driver`) |
| **Kernel-only** | `sctrl` | 2 | CFW `sctrlHENFindFunction` / `sctrlHENPatchSyscall` |
| **Kernel-only** | `sircs` | 1 | Infrared remote (SIRCS) |

Kernel builds also get `sys::gpio`, `sys::syscon`, and `sys::sysreg` driver
tables. These are resolved at runtime with `psp::hook::find_function`, not
imported. `sys::vfpu_context` provides VFPU register save/restore.

## High-level modules

There are 41 safe wrapper modules over `psp::sys` (34 always available, 1 behind
`embedded-graphics`, 6 behind `kernel`), most with RAII resource management.

| Area | Modules |
|------|---------|
| System & lifecycle | `callback` (exit callback), `power` (clocks, battery), `display` (vblank, framebuffer), `time` (`Instant`, `Duration`, `DateTime`, `FrameTimer`), `timer` (`Alarm`, `VTimer`), `dialog` (message/confirm/error dialogs), `system_param` (language, nickname, timezone, formats), `rtc` (`Tick`, RFC 3339, day of week) |
| Threads & sync | `thread` (`spawn`, `JoinHandle`, `sleep_ms`), `sync` (`SpinMutex`, `SpinRwLock`, `Semaphore`, `EventFlag`, `SpscQueue`) |
| Input | `input` (`Controller`, press/release edges, analog deadzone), `osk` (on-screen keyboard, `OskBuilder`) |
| Files & storage | `io` (`File`, `ReadDir`, `read_to_vec`, `write_bytes`), `config` (binary RCFG key-value store), `savedata` (system save/load dialog) |
| Audio | `audio` (`AudioChannel`, `SrcChannel`), `audio_mixer` (software mixer), `audiocodec` (`AudiocodecDecoder` for MP3/AAC/ATRAC3/ATRAC3plus), `mp3` (`Mp3Decoder`, frame sync, ID3v2) |
| Video | `mpeg` (`AvcDecoder`: H.264 decode on the ME via `sceMpeg`, including strided/zero-copy CSC output) |
| Graphics | `framebuffer` (`DoubleBuffer`, `LayerCompositor`), `gu_ext` (`setup_2d`, `SpriteBatch`, `GuStateSnapshot`), `simd` (VFPU `Vec4`/`Mat4`, easing, color), `image` (`decode`, `decode_jpeg`, `decode_bmp`, `load`), `font` (`FontLib`, `FontRenderer` with VRAM glyph atlas), `vram_alloc` (`Result`-based VRAM allocator), `embedded_graphics`* |
| Networking | `net` (`connect_ap`, `TcpStream`, `UdpSocket`, DNS), `http` (`HttpClient`, `RequestBuilder`), `wlan` (status) |
| Memory & hardware | `mem` (`Partition2Alloc`/`Partition3Alloc` typed partition allocators), `cache` (`CachedPtr`, `UncachedPtr`, flush/invalidate), `dma` (`memcpy_dma`, `vram_blit_dma`), `usb` (`UsbStorageMode`), `math` (VFPU `sinf`/`cosf` etc. plus libm) |
| Kernel-only | `me` (Media Engine boot, `MeExecutor`), `hw` (MMIO `hw_read32`/`hw_write32`, `Register<T>`), `hook` (`SyscallHook`, `find_function`), `gpio`, `syscon` (battery/power via System Controller), `sysreg` (GPIO/USB enable) |

\* requires the `embedded-graphics` feature.

## Other utilities

Also available: `psp::screenshot_bmp()`, `psp::benchmark()`, the
`psp::vfpu_asm!` inline VFPU assembler, `psp::dprint!`/`psp::dprintln!`
(thread-safe debug text), `psp::test_runner` (on-device test harness), and
the screen and VRAM constants (`SCREEN_WIDTH`, `BUF_WIDTH`, ...).

For runnable usage of most of these modules, see [examples.md](examples.md).
