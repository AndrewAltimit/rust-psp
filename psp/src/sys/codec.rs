//! Hardware video and audio codec bindings (`sceVideocodec`, `sceAudiocodec`).
//!
//! These functions work in both user mode and kernel mode:
//!
//! - **User mode**: Requires `sceUtilityLoadModule(AVCODEC)` and
//!   `sceUtilityLoadModule(MPEGBASE)` to be called first so the codec
//!   firmware modules are loaded.
//! - **Kernel mode**: Codec modules are typically loaded by the game or
//!   application. Kernel callers may need user-memory buffers for source/
//!   destination parameters (the codec validates pointer ranges).
//!
//! # Media Engine Integration
//!
//! `sceAudiocodecDecode` uses the Media Engine (ME) coprocessor internally.
//! In kernel mode, callers can directly control ME partition allocation for
//! codec buffers using `sceKernelAllocPartitionMemory` with partition 3
//! (ME kernel) or 10 (extended ME kernel).

psp_extern! {
    #![name = "sceVideocodec"]
    #![flags = 0x4001]
    #![version = (0x00, 0x11)]

    #[psp(0xC01EC829)]
    pub fn sceVideocodecOpen(
        buffer: *mut u32,
        type_: i32,
    ) -> i32;

    #[psp(0x2D31F5B1)]
    pub fn sceVideocodecGetEDRAM(
        buffer: *mut u32,
        type_: i32,
    ) -> i32;

    #[psp(0x17099F0A)]
    pub fn sceVideocodecInit(
        buffer: *mut u32,
        type_: i32,
    ) -> i32;

    #[psp(0xDBA273FA)]
    pub fn sceVideocodecDecode(
        buffer: *mut u32,
        type_: i32,
    ) -> i32;

    #[psp(0x4F160BF4)]
    pub fn sceVideocodecReleaseEDRAM(buffer: *mut u32) -> i32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum AudioCodec {
    At3Plus = 0x00001000,
    At3 = 0x00001001,
    Mp3 = 0x00001002,
    Aac = 0x00001003,
}

psp_extern! {
    #![name = "sceAudiocodec"]
    #![flags = 0x4009]
    #![version = (0x00, 0x00)]

    #[psp(0x9D3F790C)]
    pub fn sceAudiocodecCheckNeedMem(
        buffer: *mut u32,
        type_: i32,
    ) -> i32;

    #[psp(0x5B37EB1D)]
    pub fn sceAudiocodecInit(
        buffer: *mut u32,
        type_: i32,
    ) -> i32;

    #[psp(0x70A703F8)]
    pub fn sceAudiocodecDecode(
        buffer: *mut u32,
        type_: i32,
    ) -> i32;

    #[psp(0x3A20A200)]
    pub fn sceAudiocodecGetEDRAM(
        buffer: *mut u32,
        type_: i32,
    ) -> i32;

    #[psp(0x29681260)]
    pub fn sceAudiocodecReleaseEDRAM(buffer: *mut u32) -> i32;
}
