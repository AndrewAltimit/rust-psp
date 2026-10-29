# PRX section structure

These are notes on the ELF sections a PSP module (PRX) uses, and on how
rust-psp produces each one. The module macros are in `psp/src/lib.rs`
(`module!` / `module_kernel!`, implemented by `__module_impl!`). The import
macro is `psp_extern!` in `psp/src/sys/macros.rs`.

## .rodata.sceModuleInfo

* The most important section. It holds the module's `SceModuleInfo` and
  points to the other sections.
* Emitted by `__module_impl!`. It contains the module attribute (`0` for user,
  `0x1000` for kernel via `module_kernel!`), the version, the name, `_gp`, and
  the bounds of the export and import tables (`__lib_ent_top/bottom` and
  `__lib_stub_top/bottom`).

## .lib.ent (with .lib.ent.top / .lib.ent.btm)

* Describes the module's exports.
* rust-psp emits a single syslib entry exporting one function
  (`module_start`) and one variable (`module_info`). It does **not**
  generate a `module_stop`.
* `.lib.ent.top` and `.lib.ent.btm` are marker sections. A `global_asm!`
  block in `lib.rs` emits them to delimit the table.

## .rodata.sceResident

* Has two uses:
  * The export table referenced from `.lib.ent`. The number of entries is
    given by the variable and function count fields in `.lib.ent`. For
    rust-psp this is the `SceLibraryEntryTable` (NIDs of `module_start` and
    `module_info`, followed by their addresses).
  * The names of imported libraries. `psp_extern!` writes each name to
    `.rodata.sceResident.<lib>`.

## .lib.stub (with .lib.stub.top / .lib.stub.btm)

* Describes the libraries the module imports. There is one
  `SceStubLibraryEntry` per library, emitted by `psp_extern!` into
  `.lib.stub.entry.<lib>`.
* Imports can resolve against firmware libraries or against libraries exported
  by other loaded modules (for example CFW's `KUBridge` and
  `SystemCtrlForKernel`).
* LTO can strip unused imports, which makes the per-library stub counts
  wrong. After linking, `cargo-psp/src/fix_imports.rs` rewrites the counts
  based on the order of `.rodata.sceNid`.

## .sceStub.text

* Holds the call stubs for imported functions, 8 bytes each, in
  `.sceStub.text.<lib>.<function>`.
* The PSPSDK C toolchain fills each stub with `jr $ra; nop`. rust-psp stores a
  `Stub` struct there instead: pointers to the function's NID and to its
  library stub entry. This makes LTO keep the NID and library entry for
  exactly the functions that are called. At load time the firmware
  overwrites the 8 bytes with `jr $ra; syscall <n>` (or a jump), so the
  pointer data is never executed. See the comment on `Stub` in
  `psp/src/sys/macros.rs`.
* Some CFW versions only patch the first word of kernel import stubs, which
  leaves the pointer data in the delay slot. `psp::hook` works around this
  (see [kernel-mode.md](kernel-mode.md#syscall-hooking-cfw-plugins)).

## .rodata.sceNid

* Holds the lists of NIDs that `.lib.stub` references, in
  `.rodata.sceNid.<lib>.<function>`.
