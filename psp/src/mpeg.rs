//! High-level H.264 AVC decoder using the PSP Media Engine.
//!
//! Wraps the `sceMpeg*` syscalls into a safe RAII interface for decoding H.264
//! NAL units from MP4 containers. The decoder uses the Media Engine (ME)
//! coprocessor for hardware-accelerated H.264 decode and color space conversion.
//!
//! # Architecture
//!
//! The PSP has two decode paths through `sceMpeg`:
//!
//! 1. **PSMF ringbuffer path** (`sceMpegGetAvcAu`): For UMD video playback.
//!    H.264 access units are wrapped in MPEG-PS packets and fed through a
//!    ringbuffer callback. Standard PSPSDK documentation covers this path.
//!
//! 2. **NAL direct path** (`sceMpegGetAvcNalAu`): For MP4 container playback.
//!    Raw H.264 NAL units (AVCC format, length-prefixed) are fed directly to
//!    the ME with separate SPS/PPS. This path is used by homebrew video players
//!    like PMPlayer (cooleyes). **This module implements this path.**
//!
//! # Undocumented Parameters (discovered via PMPlayer source analysis)
//!
//! The NAL path requires three parameters not documented in PSPSDK or PPSSPP:
//!
//! - **`sceMpegQueryMemSize(mode)`** / **`sceMpegCreate(..., mode, ddr_top)`**:
//!   `mode` must be **4** (video ≤480×272) or **5** (video >480×272). Standard
//!   value 0 only works for the PSMF ringbuffer path.
//!
//! - **`ddr_top`**: A 2MB buffer aligned to a 4MB boundary, passed as the last
//!   argument to `sceMpegCreate`. The ME uses this as workspace for decoded
//!   YCbCr frames. Without it, `sceMpegAvcDecode` returns `0x80628002`
//!   (AVC_DECODE_FATAL) on every frame.
//!
//! - **AU buffer**: Must be `ddr_top + 0x10000` (not `sceMpegMallocAvcEsBuf`).
//!   The AU struct must be initialized with `0xFF` bytes before `sceMpegInitAu`.
//!
//! # Requirements
//!
//! - Load AV modules before creating a decoder:
//!   `sceUtilityLoadModule(PSP_MODULE_AV_AVCODEC)` and
//!   `sceUtilityLoadModule(PSP_MODULE_AV_MP3)` (for ME codec support).
//! - Load `mpeg_vsh370.prx` via `sceKernelLoadModule` + `sceKernelStartModule`
//!   from a non-main thread (loading on main thread can freeze GU). This PRX
//!   registers the "sceMpeg" library, which resolves the EBOOT's weak import
//!   stubs. See [`AvcDecoder::new`] for details.
//!
//! **Important:** `sceUtilityLoadModule(AvMpegBase)` does NOT work for the NAL
//! decode path. AvMpegBase provides the standard PSMF ringbuffer path only
//! (`sceMpegGetAvcAu`). The mode 4/5 + DDR top parameters required by
//! `sceMpegGetAvcNalAu` are specific to `mpeg_vsh370.prx`'s implementation.
//! AvMpegBase returns `0x80628002` (AVC_DECODE_FATAL) even with correct
//! parameters. Tested on real PSP hardware (2026-03-25).
//!
//! # Example
//!
//! ```ignore
//! use psp::mpeg::{AvcDecoder, AvcNal};
//!
//! // SPS/PPS extracted from MP4 avcC box; NAL data in AVCC format.
//! let mut decoder = AvcDecoder::new(480, 272).unwrap();
//! let nal = AvcNal {
//!     sps: &sps_bytes,
//!     pps: &pps_bytes,
//!     data: &avcc_nal_data,
//!     prefix_size: 4,        // from avcC lengthSizeMinusOne + 1
//!     is_first_frame: true,  // mode=3 for IDR, mode=0 for subsequent
//! };
//! if let Some(frame) = decoder.decode(&nal) {
//!     // frame.pixels is ABGR 8888, frame.width × frame.height
//! }
//! ```
//!
//! # References
//!
//! - PMPlayer source: `DavisDev/pmplayer-advance` on GitHub
//!   (key file: `/ppa/mod/mp4avcdecoder.c`)
//! - OASIS OS investigation: `docs/psp-me-decode-next-steps.md`

use alloc::{boxed::Box, vec, vec::Vec};
use core::{ffi::c_void, marker::PhantomData};

/// Error from an sceMpeg operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MpegError(pub i32);

impl core::fmt::Debug for MpegError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "MpegError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for MpegError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "sceMpeg error {:#010x}", self.0 as u32)
    }
}

/// Input NAL unit for [`AvcDecoder::decode`].
///
/// Contains SPS/PPS parameter sets and one H.264 access unit in AVCC format
/// (length-prefixed NAL units, as stored in MP4 `mdat`).
pub struct AvcNal<'a> {
    /// Raw SPS NAL unit bytes (from MP4 avcC box, without start codes).
    pub sps: &'a [u8],
    /// Raw PPS NAL unit bytes (from MP4 avcC box, without start codes).
    pub pps: &'a [u8],
    /// H.264 access unit data in AVCC format (length-prefixed NAL units).
    /// This is the raw sample data from the MP4 container.
    pub data: &'a [u8],
    /// NAL length prefix size in bytes (from avcC `lengthSizeMinusOne + 1`,
    /// typically 4). Each NAL unit in `data` is preceded by a big-endian
    /// length field of this many bytes.
    pub prefix_size: i32,
    /// Set to `true` for the first frame (IDR/keyframe). This sets `mode=3`
    /// in the NAL struct, which tells the ME to initialize decode state.
    /// Subsequent frames should use `false` (`mode=0`).
    pub is_first_frame: bool,
}

/// A decoded video frame with ABGR pixel data.
pub struct DecodedFrame {
    /// ABGR 8888 pixel data, `width × height × 4` bytes.
    /// Alpha channel is set to 0xFF (fully opaque).
    pub pixels: Vec<u8>,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
}

/// Mp4AvcNalStruct — the C struct expected by `sceMpegGetAvcNalAu`.
///
/// Layout discovered from PMPlayer source (`mp4avcdecoder.h`).
#[repr(C)]
struct Mp4AvcNalStruct {
    sps_buffer: *const u8,
    sps_size: i32,
    pps_buffer: *const u8,
    pps_size: i32,
    nal_prefix_size: i32,
    nal_buffer: *const u8,
    nal_size: i32,
    mode: i32, // 3 = first frame (IDR), 0 = subsequent
}

/// Mp4AvcCscStruct — passed to `sceMpegBaseCscAvc` for YCbCr→ABGR conversion.
///
/// Layout: height in macroblocks, width in macroblocks, two mode fields,
/// then 8 buffer pointers from the ME's YCbCr output.
#[repr(C)]
struct Mp4AvcCscStruct {
    height: i32,
    width: i32,
    mode0: i32,
    mode1: i32,
    buffers: [*const c_void; 8],
}

/// RAII H.264 AVC decoder using the PSP Media Engine.
///
/// Manages all ME resources (DDR workspace, ringbuffer, sceMpeg instance).
/// Resources are released on drop.
///
/// # Thread Safety
///
/// Not `Send` or `Sync`. Must be used from the thread that created it
/// (typically the video decode thread, NOT the main/GU thread).
pub struct AvcDecoder {
    mpeg_storage: *mut *mut c_void,
    _mpeg_data: Vec<u8>,
    ddr_block: crate::sys::SceUid,
    ddr_aligned: u32,
    _ringbuffer: Box<crate::sys::SceMpegRingbuffer>,
    _rb_data: Vec<u8>,
    au: crate::sys::SceMpegAu,
    output_buf: Vec<u8>,
    pic_num: i32,
    frame_width: u32,
    width: u32,
    height: u32,
    _marker: PhantomData<*const ()>, // !Send + !Sync
}

impl AvcDecoder {
    /// Create a new H.264 AVC decoder for the given video dimensions.
    ///
    /// Performs the full sceMpeg initialization sequence:
    /// 1. `sceMpegInit`
    /// 2. `sceMpegQueryMemSize(mode)` — mode 4 (≤480p) or 5 (>480p)
    /// 3. DDR top allocation — 2MB, 4MB-aligned (ME decode workspace)
    /// 4. `sceMpegRingbufferConstruct` — minimal 8-packet ringbuffer
    /// 5. `sceMpegCreate(mode, ddr_top)` — passes undocumented params
    /// 6. `sceMpegRegistStream` — register video stream
    /// 7. `sceMpegInitAu(ddr_top + 0x10000)` — AU in DDR region
    /// 8. `sceMpegAvcDecodeMode(Psm8888)` — ABGR pixel output
    ///
    /// # Prerequisites
    ///
    /// - AV modules loaded (`sceUtilityLoadModule(AvCodec)`, `(AvMp3)`)
    /// - `mpeg_vsh370.prx` loaded and started (provides sceMpeg implementation)
    /// - Called from a non-main thread (main thread module loading freezes GU)
    ///
    /// # Errors
    ///
    /// Returns `MpegError` if any initialization step fails.
    pub fn new(width: u32, height: u32) -> Result<Self, MpegError> {
        // Mode 4 for ≤480p, 5 for larger (PMPlayer convention).
        let mpeg_mode = if width > 480 { 5 } else { 4 };
        let frame_width = if width > 480 { 768u32 } else { 512 };

        // Step 1: Init MPEG subsystem (ignore "already init" errors).
        let ret = unsafe { crate::sys::sceMpegInit() };
        if ret < 0 && ret != 0x80618003u32 as i32 && ret != 0x80618005u32 as i32 {
            return Err(MpegError(ret));
        }

        // Step 2: Query buffer size for this mode.
        let mem_size = unsafe { crate::sys::sceMpegQueryMemSize(mpeg_mode) };
        if mem_size <= 0 {
            return Err(MpegError(mem_size));
        }

        // Allocate 64-byte-aligned mpeg data buffer.
        let mut mpeg_data = vec![0u8; mem_size as usize + 64];
        let mpeg_data_aligned = {
            let p = mpeg_data.as_mut_ptr();
            unsafe { p.add(p.align_offset(64)) }
        };

        // Step 3: Allocate DDR top — 2MB workspace, 4MB-aligned.
        // The ME writes decoded YCbCr frames here. Without this pointer,
        // sceMpegAvcDecode returns 0x80628002 (AVC_DECODE_FATAL).
        let ddr_block = unsafe {
            crate::sys::sceKernelAllocPartitionMemory(
                crate::sys::SceSysMemPartitionId::SceKernelPrimaryUserPartition,
                b"MeDdrTop\0".as_ptr(),
                crate::sys::SceSysMemBlockTypes::Low,
                0x20_0000 + 0x40_0000, // 2MB + 4MB for alignment
                core::ptr::null_mut(),
            )
        };
        if ddr_block < crate::sys::SceUid(0) {
            return Err(MpegError(ddr_block.0));
        }
        let ddr_raw = unsafe { crate::sys::sceKernelGetBlockHeadAddr(ddr_block) };
        let ddr_aligned = ((ddr_raw as u32) + 0x3F_FFFF) & !0x3F_FFFF;

        // Step 4: Construct minimal ringbuffer (required by sceMpegCreate,
        // but NAL feeding bypasses it — we use sceMpegGetAvcNalAu instead).
        let rb_packets = 8;
        let rb_size = unsafe { crate::sys::sceMpegRingbufferQueryMemSize(rb_packets) };
        let mut rb_data = vec![0u8; if rb_size > 0 { rb_size as usize } else { 16384 }];
        let mut ringbuffer = Box::new(unsafe {
            core::mem::zeroed::<crate::sys::SceMpegRingbuffer>()
        });
        if rb_size > 0 {
            let ret = unsafe {
                crate::sys::sceMpegRingbufferConstruct(
                    &mut *ringbuffer,
                    rb_packets,
                    rb_data.as_mut_ptr() as *mut c_void,
                    rb_size,
                    None,
                    core::ptr::null_mut(),
                )
            };
            if ret < 0 {
                unsafe { crate::sys::sceKernelFreePartitionMemory(ddr_block) };
                return Err(MpegError(ret));
            }
        }

        // Step 5: Create sceMpeg instance with mode + DDR top.
        let mpeg_storage = Box::into_raw(Box::new(core::ptr::null_mut::<c_void>()));
        let mpeg: crate::sys::SceMpeg = unsafe {
            core::mem::transmute(mpeg_storage as *mut *mut c_void)
        };
        let ret = unsafe {
            crate::sys::sceMpegCreate(
                mpeg,
                mpeg_data_aligned as *mut c_void,
                mem_size,
                &mut *ringbuffer,
                frame_width as i32,
                mpeg_mode,
                ddr_aligned as i32,
            )
        };
        if ret < 0 {
            unsafe {
                let _ = Box::from_raw(mpeg_storage);
                crate::sys::sceKernelFreePartitionMemory(ddr_block);
            }
            return Err(MpegError(ret));
        }

        // Step 6: Register video stream.
        let _stream = unsafe { crate::sys::sceMpegRegistStream(mpeg, 0, 0) };

        // Step 7: Initialize AU from DDR top + 0x10000 (PMPlayer convention).
        // AU must be filled with 0xFF before sceMpegInitAu.
        let au_buffer = (ddr_aligned + 0x10000) as *mut c_void;
        let mut au = unsafe {
            let mut a = core::mem::MaybeUninit::<crate::sys::SceMpegAu>::uninit();
            core::ptr::write_bytes(
                a.as_mut_ptr() as *mut u8,
                0xFF,
                core::mem::size_of::<crate::sys::SceMpegAu>(),
            );
            a.assume_init()
        };
        let ret = unsafe { crate::sys::sceMpegInitAu(mpeg, au_buffer, &mut au) };
        if ret < 0 {
            unsafe {
                crate::sys::sceMpegDelete(mpeg);
                let _ = Box::from_raw(mpeg_storage);
                crate::sys::sceKernelFreePartitionMemory(ddr_block);
            }
            return Err(MpegError(ret));
        }

        // Step 8: Set decode mode to ABGR 8888.
        let mut mode = crate::sys::SceMpegAvcMode {
            unk0: -1,
            pixel_format: crate::sys::DisplayPixelFormat::Psm8888,
        };
        let ret = unsafe { crate::sys::sceMpegAvcDecodeMode(mpeg, &mut mode) };
        if ret < 0 {
            unsafe {
                crate::sys::sceMpegDelete(mpeg);
                let _ = Box::from_raw(mpeg_storage);
                crate::sys::sceKernelFreePartitionMemory(ddr_block);
            }
            return Err(MpegError(ret));
        }

        // Output pixel buffer (stride × aligned_height × 4 bytes ABGR).
        let out_h = ((height + 15) / 16) * 16;
        let output_buf = vec![0u8; frame_width as usize * out_h as usize * 4];

        Ok(Self {
            mpeg_storage,
            _mpeg_data: mpeg_data,
            ddr_block,
            ddr_aligned,
            _ringbuffer: ringbuffer,
            _rb_data: rb_data,
            au,
            output_buf,
            pic_num: 0,
            frame_width,
            width,
            height,
            _marker: PhantomData,
        })
    }

    /// Get the SceMpeg handle for low-level access.
    fn mpeg(&self) -> crate::sys::SceMpeg {
        unsafe { core::mem::transmute(self.mpeg_storage as *mut *mut c_void) }
    }

    /// Decode one H.264 access unit.
    ///
    /// Feeds the NAL data to the ME via `sceMpegGetAvcNalAu`, decodes via
    /// `sceMpegAvcDecode`, and converts YCbCr→ABGR via `sceMpegBaseCscAvc`.
    ///
    /// Returns `Some(DecodedFrame)` when a frame is produced, `None` if the
    /// ME needs more data (e.g., B-frame reordering). Returns `Err` on
    /// fatal decode errors.
    ///
    /// # NAL Data Format
    ///
    /// `nal.data` must be in **AVCC format** (length-prefixed NAL units), as
    /// stored directly in MP4 `mdat`. Do NOT convert to Annex B start codes.
    /// The `prefix_size` field tells the ME how to parse the length fields.
    pub fn decode(&mut self, nal: &AvcNal<'_>) -> Result<Option<DecodedFrame>, MpegError> {
        if nal.data.is_empty() {
            return Ok(None);
        }

        let mpeg = self.mpeg();

        // Build NAL struct for sceMpegGetAvcNalAu.
        let mut nal_struct = Mp4AvcNalStruct {
            sps_buffer: nal.sps.as_ptr(),
            sps_size: nal.sps.len() as i32,
            pps_buffer: nal.pps.as_ptr(),
            pps_size: nal.pps.len() as i32,
            nal_prefix_size: nal.prefix_size,
            nal_buffer: nal.data.as_ptr(),
            nal_size: nal.data.len() as i32,
            mode: if nal.is_first_frame { 3 } else { 0 },
        };

        // Flush D-cache so ME sees current data.
        unsafe {
            crate::sys::sceKernelDcacheWritebackInvalidateRange(
                nal.data.as_ptr() as *const c_void,
                nal.data.len() as u32,
            );
            crate::sys::sceKernelDcacheWritebackInvalidateRange(
                &nal_struct as *const _ as *const c_void,
                core::mem::size_of::<Mp4AvcNalStruct>() as u32,
            );
        }

        // Feed NAL to ME.
        let ret = unsafe {
            crate::sys::sceMpegGetAvcNalAu(
                mpeg,
                &mut nal_struct as *mut _ as *mut c_void,
                &mut self.au,
            )
        };
        if ret < 0 {
            return Err(MpegError(ret));
        }

        // Decode.
        let mut output_ptr = self.output_buf.as_mut_ptr() as *mut c_void;
        let buf_arg = &mut output_ptr as *mut *mut c_void as *mut c_void;
        let ret = unsafe {
            crate::sys::sceMpegAvcDecode(mpeg, &mut self.au, 512, buf_arg, &mut self.pic_num)
        };
        if ret < 0 {
            return Err(MpegError(ret));
        }

        if self.pic_num <= 0 {
            return Ok(None); // No picture yet (B-frame reordering).
        }

        // Get YCbCr buffer pointers from decode detail.
        let mut detail2: *mut c_void = core::ptr::null_mut();
        let ret = unsafe { crate::sys::sceMpegAvcDecodeDetail2(mpeg, &mut detail2) };
        if ret < 0 || detail2.is_null() {
            return Err(MpegError(if ret < 0 { ret } else { -1 }));
        }

        // Extract info and YCbCr pointers from detail struct.
        let detail_ptr = detail2 as *const u32;
        let info_ptr = unsafe { *detail_ptr.add(4) } as *const u32;
        let yuv_ptr = unsafe { *detail_ptr.add(11) } as *const u32;

        if info_ptr.is_null() || yuv_ptr.is_null() {
            return Ok(None);
        }

        // Build CSC struct for hardware YCbCr→ABGR conversion.
        let info_w = unsafe { *info_ptr.add(2) } as u32;
        let info_h = unsafe { *info_ptr.add(3) } as u32;
        let csc_width = if info_w > 480 { 768i32 } else { 512 };

        let csc = Mp4AvcCscStruct {
            height: ((info_h + 15) / 16) as i32,
            width: ((info_w + 15) / 16) as i32,
            mode0: 0,
            mode1: 0,
            buffers: [
                unsafe { *yuv_ptr.add(0) } as *const c_void,
                unsafe { *yuv_ptr.add(1) } as *const c_void,
                unsafe { *yuv_ptr.add(2) } as *const c_void,
                unsafe { *yuv_ptr.add(3) } as *const c_void,
                unsafe { *yuv_ptr.add(4) } as *const c_void,
                unsafe { *yuv_ptr.add(5) } as *const c_void,
                unsafe { *yuv_ptr.add(6) } as *const c_void,
                unsafe { *yuv_ptr.add(7) } as *const c_void,
            ],
        };

        let ret = unsafe {
            crate::sys::sceMpegBaseCscAvc(
                self.output_buf.as_mut_ptr() as *mut c_void,
                0,
                csc_width,
                &csc as *const _ as *mut c_void,
            )
        };
        if ret < 0 {
            return Err(MpegError(ret));
        }

        // Copy from stride-aligned output to tight pixel buffer,
        // fixing alpha (CSC outputs A=0x00, we need A=0xFF).
        let w = self.width as usize;
        let h = self.height as usize;
        let stride = self.frame_width as usize;
        let mut pixels = vec![0u8; w * h * 4];
        for row in 0..h {
            let src_off = row * stride * 4;
            let dst_off = row * w * 4;
            unsafe {
                core::ptr::copy_nonoverlapping(
                    self.output_buf.as_ptr().add(src_off),
                    pixels.as_mut_ptr().add(dst_off),
                    w * 4,
                );
            }
            // Fix alpha channel to 0xFF (fully opaque).
            for x in 0..w {
                pixels[dst_off + x * 4 + 3] = 0xFF;
            }
        }

        Ok(Some(DecodedFrame {
            pixels,
            width: self.width,
            height: self.height,
        }))
    }

    /// Video width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Video height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// DDR top address (4MB-aligned ME workspace).
    pub fn ddr_top(&self) -> u32 {
        self.ddr_aligned
    }
}

impl Drop for AvcDecoder {
    fn drop(&mut self) {
        let mpeg = self.mpeg();
        unsafe {
            crate::sys::sceMpegDelete(mpeg);
            let _ = Box::from_raw(self.mpeg_storage);
            if self.ddr_block >= crate::sys::SceUid(0) {
                crate::sys::sceKernelFreePartitionMemory(self.ddr_block);
            }
            crate::sys::sceMpegFinish();
        }
    }
}
