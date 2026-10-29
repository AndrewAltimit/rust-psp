# rust-psp Documentation

Start with the [project README](../README.md) for installation and a quick
start.

## Using the SDK

| Document | Description |
|----------|-------------|
| [api-overview.md](api-overview.md) | Feature flags, all syscall domains in `psp::sys` with counts, and every high-level module |
| [examples.md](examples.md) | The 30 bundled examples and what each one demonstrates |
| [std.md](std.md) | Experimental `std` support: how the overlay works, out-of-tree setup, support matrix |
| [kernel-mode.md](kernel-mode.md) | Kernel-mode modules: what the `kernel` feature unlocks, Media Engine, syscall hooking, exception handlers |
| [troubleshooting.md](troubleshooting.md) | Running on hardware or PPSSPP, debugging with psplink/gdb, common build errors |

## Internals and project

| Document | Description |
|----------|-------------|
| [prx.md](prx.md) | PRX/ELF section layout (`sceModuleInfo`, `.lib.ent`, `.lib.stub`, NIDs) |
| [fork-changes.md](fork-changes.md) | Changes from upstream `overdrivenpotato/rust-psp` |
| [maintainers.md](maintainers.md) | Workspaces, running CI locally, GitHub Actions, releases, Docker images, agent tooling |
