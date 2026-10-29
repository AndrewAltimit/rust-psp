# Examples

There are 30 examples in [`examples/`](../examples/). Build one with
`cd examples/<name> && cargo psp --release`.

| Example | Demonstrates |
|---------|--------------|
| [`hello-world`](../examples/hello-world/) | Minimal program: `module!`, exit callback, `dprint!` |
| [`cube`](../examples/cube/) | Rotating textured 3D cube (`sceGu*`/`sceGum*`, VRAM allocator) |
| [`rainbow`](../examples/rainbow/) | Color-cycling fill, written straight to the framebuffer (`sceDisplay*`, no GU) |
| [`gu-background`](../examples/gu-background/) | Minimal GU setup that clears the screen |
| [`gu-debug-print`](../examples/gu-debug-print/) | On-screen text with `sceGuDebugPrint` |
| [`screenshot`](../examples/screenshot/) | GU frame captured with `screenshot_bmp()` and written via `sceIo*` |
| [`system-font`](../examples/system-font/) | System PGF fonts via `psp::font::FontRenderer` + GU |
| [`embedded-graphics`](../examples/embedded-graphics/) | Shapes, text, and a BMP via `embedded-graphics` + `tinybmp` |
| [`paint-mode`](../examples/paint-mode/) | Analog-stick paint program (embedded-graphics, raw `sceCtrl*`) |
| [`fontdue-scrolltext`](../examples/fontdue-scrolltext/) | TrueType scrolling text with `fontdue` |
| [`ratatui`](../examples/ratatui/) | `ratatui` TUI rendered through `mousefood` (needs the `unicode-width` patch, see [troubleshooting.md](troubleshooting.md#common-problems)) |
| [`vfpu-addition`](../examples/vfpu-addition/) | `vfpu_asm!` integer addition on the VFPU |
| [`vfpu-context-switching`](../examples/vfpu-context-switching/) | Save/restore VFPU matrices with `sys::vfpu_context::Context` |
| [`input-analog`](../examples/input-analog/) | `psp::input::Controller` with analog deadzone |
| [`osk-input`](../examples/osk-input/) | On-screen keyboard (`psp::osk`, builder and convenience API) |
| [`msg-dialog`](../examples/msg-dialog/) | System message dialog via `psp::dialog` over GU |
| [`savedata`](../examples/savedata/) | System save/load dialog via `psp::savedata` |
| [`config-save`](../examples/config-save/) | Key-value settings via `psp::config` |
| [`file-io`](../examples/file-io/) | Write and read back a file with `psp::io` |
| [`audio-tone`](../examples/audio-tone/) | 440 Hz sine wave through `psp::audio::AudioChannel` |
| [`clock-speed`](../examples/clock-speed/) | Read and set CPU/bus clocks with `psp::power` |
| [`time`](../examples/time/) | Current date and time via `psp::time::DateTime` |
| [`rtc-sysinfo`](../examples/rtc-sysinfo/) | `psp::rtc` tick math and RFC 3339, plus `psp::system_param` |
| [`timer-alarm`](../examples/timer-alarm/) | One-shot `Alarm` and `VTimer` |
| [`thread-sync`](../examples/thread-sync/) | `psp::thread::spawn` with a shared `SpinMutex` counter |
| [`wlan`](../examples/wlan/) | WLAN power/switch state and MAC address via `psp::wlan::status()` |
| [`net-http`](../examples/net-http/) | Wi-Fi connect + raw HTTP over `psp::net::TcpStream` (real hardware only) |
| [`http-client`](../examples/http-client/) | High-level GET with `psp::http::HttpClient` (real hardware only) |
| [`kernel-mode`](../examples/kernel-mode/) | `module_kernel!`, ME clock, volatile memory, NAND info, `psp::hw` registers (CFW only) |
| [`rust-std-hello-world`](../examples/rust-std-hello-world/) | `String`, `Vec`, and `format!` with the `std` feature |

The examples depend on `psp` by path (`../../psp`). In your own project,
use the git dependency shown in the [README](../README.md#quick-start).
