//! MP3 decoder for the PSP.
//!
//! Wraps the hardware-accelerated `sceMp3*` syscalls for decoding MP3
//! audio data into PCM samples suitable for playback via [`crate::audio`].
//!
//! # Example
//!
//! ```ignore
//! use psp::mp3::Mp3Decoder;
//!
//! let data = psp::io::read_to_vec("ms0:/music/song.mp3").unwrap();
//! let mut decoder = Mp3Decoder::new(&data).unwrap();
//!
//! while let Ok(samples) = decoder.decode_frame() {
//!     if samples.is_empty() { break; }
//!     // Feed samples to psp::audio::AudioChannel
//! }
//! ```

use crate::sys;
use alloc::vec::Vec;
use core::ffi::c_void;

/// Error from an MP3 operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Mp3Error(pub i32);

impl core::fmt::Debug for Mp3Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Mp3Error({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for Mp3Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "mp3 error {:#010x}", self.0 as u32)
    }
}

/// MP3 decoder with RAII resource management.
///
/// Decodes MP3 data using the PSP's hardware decoder. The MP3 data is
/// provided as a byte slice and must remain valid for the decoder's lifetime.
pub struct Mp3Decoder {
    handle: sys::Mp3Handle,
    /// MP3 source data (kept alive for the duration of decoding).
    _data: Vec<u8>,
    /// Internal stream buffer used by the MP3 decoder.
    mp3_buf: Vec<u8>,
    /// Internal PCM output buffer.
    pcm_buf: Vec<i16>,
    /// Whether we've finished feeding data.
    eof: bool,
}

/// Size of the internal MP3 stream buffer.
const MP3_BUF_SIZE: usize = 8 * 1024;
/// Size of the internal PCM output buffer (max output per decode call).
const PCM_BUF_SIZE: usize = 4608; // 1152 samples * 2 channels * 2 bytes (as i16 count)

impl Mp3Decoder {
    /// Create a decoder from in-memory MP3 data.
    ///
    /// Initializes the MP3 resource subsystem, reserves a handle, and
    /// feeds the initial data to the decoder.
    pub fn new(data: &[u8]) -> Result<Self, Mp3Error> {
        let ret = unsafe { sys::sceMp3InitResource() };
        if ret < 0 {
            return Err(Mp3Error(ret));
        }

        // Skip ID3v2 tag so the decoder sees raw MP3 frames.
        let start_offset = skip_id3v2(data);

        let owned_data = Vec::from(data);
        let mut mp3_buf = alloc::vec![0u8; MP3_BUF_SIZE];
        let mut pcm_buf = alloc::vec![0i16; PCM_BUF_SIZE];

        let mut init_arg = sys::SceMp3InitArg {
            mp3_stream_start: start_offset as u32,
            unk1: 0,
            mp3_stream_end: owned_data.len() as u32,
            unk2: 0,
            mp3_buf: mp3_buf.as_mut_ptr() as *mut c_void,
            mp3_buf_size: MP3_BUF_SIZE as i32,
            pcm_buf: pcm_buf.as_mut_ptr() as *mut c_void,
            pcm_buf_size: (PCM_BUF_SIZE * 2) as i32, // in bytes
        };

        let handle_id = unsafe { sys::sceMp3ReserveMp3Handle(&mut init_arg) };
        if handle_id < 0 {
            unsafe { sys::sceMp3TermResource() };
            return Err(Mp3Error(handle_id));
        }
        let handle = sys::Mp3Handle(handle_id);

        let mut decoder = Self {
            handle,
            _data: owned_data,
            mp3_buf,
            pcm_buf,
            eof: false,
        };

        // Feed initial data.
        if let Err(e) = decoder.feed_data() {
            unsafe {
                sys::sceMp3ReleaseMp3Handle(handle);
                sys::sceMp3TermResource();
            }
            return Err(e);
        }

        // Initialize the decoder.
        let ret = unsafe { sys::sceMp3Init(handle) };
        if ret < 0 {
            unsafe {
                sys::sceMp3ReleaseMp3Handle(handle);
                sys::sceMp3TermResource();
            }
            return Err(Mp3Error(ret));
        }

        Ok(decoder)
    }

    /// Decode the next frame of MP3 data.
    ///
    /// Returns a slice of interleaved stereo i16 PCM samples.
    /// Returns an empty slice when decoding is complete.
    pub fn decode_frame(&mut self) -> Result<&[i16], Mp3Error> {
        // Feed more data if the decoder needs it.
        if !self.eof && unsafe { sys::sceMp3CheckStreamDataNeeded(self.handle) } > 0 {
            self.feed_data()?;
        }

        let mut out_ptr: *mut i16 = core::ptr::null_mut();
        let ret = unsafe { sys::sceMp3Decode(self.handle, &mut out_ptr) };
        if ret < 0 {
            // Negative values other than "no more data" are errors.
            // sceMp3Decode returns 0 when no more data.
            return Err(Mp3Error(ret));
        }
        if ret == 0 || out_ptr.is_null() {
            return Ok(&[]);
        }

        // ret is the number of bytes decoded.
        let sample_count = ret as usize / 2; // i16 samples
        Ok(unsafe { core::slice::from_raw_parts(out_ptr, sample_count) })
    }

    /// Get the sampling rate of the MP3 stream.
    pub fn sample_rate(&self) -> u32 {
        let ret = unsafe { sys::sceMp3GetSamplingRate(self.handle) };
        if ret < 0 { 0 } else { ret as u32 }
    }

    /// Get the number of channels (1 = mono, 2 = stereo).
    pub fn channels(&self) -> u8 {
        let ret = unsafe { sys::sceMp3GetMp3ChannelNum(self.handle) };
        if ret < 0 { 0 } else { ret as u8 }
    }

    /// Get the bitrate in kbps.
    pub fn bitrate(&self) -> u32 {
        let ret = unsafe { sys::sceMp3GetBitRate(self.handle) };
        if ret < 0 { 0 } else { ret as u32 }
    }

    /// Set the number of times to loop. -1 = infinite, 0 = no loop.
    pub fn set_loop(&mut self, count: i32) {
        unsafe { sys::sceMp3SetLoopNum(self.handle, count) };
    }

    /// Reset playback position to the beginning.
    pub fn reset(&mut self) -> Result<(), Mp3Error> {
        let ret = unsafe { sys::sceMp3ResetPlayPosition(self.handle) };
        if ret < 0 { Err(Mp3Error(ret)) } else { Ok(()) }
    }

    /// Release the MP3 handle without terminating the global resource.
    ///
    /// Use this instead of dropping when another decoder will be created
    /// afterward (e.g. switching songs). The global MP3 resource subsystem
    /// stays initialized so the next `Mp3Decoder::new()` succeeds without
    /// a full Init→Term→Init cycle (which crashes on real PSP hardware).
    ///
    /// Heap buffers are freed normally — only `sceMp3TermResource` is skipped.
    pub fn release(self) {
        let mut this = core::mem::ManuallyDrop::new(self);
        unsafe { sys::sceMp3ReleaseMp3Handle(this.handle) };
        // Free heap buffers without running Drop (which calls TermResource).
        // SAFETY: Each field is valid and only dropped once.
        unsafe {
            core::ptr::drop_in_place(&mut this._data);
            core::ptr::drop_in_place(&mut this.mp3_buf);
            core::ptr::drop_in_place(&mut this.pcm_buf);
        }
    }

    /// Feed data from the source buffer into the decoder's stream buffer.
    fn feed_data(&mut self) -> Result<(), Mp3Error> {
        let mut dst_ptr: *mut u8 = core::ptr::null_mut();
        let mut to_write: i32 = 0;
        let mut src_pos: i32 = 0;

        let ret = unsafe {
            sys::sceMp3GetInfoToAddStreamData(
                self.handle,
                &mut dst_ptr,
                &mut to_write,
                &mut src_pos,
            )
        };
        if ret < 0 {
            return Err(Mp3Error(ret));
        }

        if to_write <= 0 || dst_ptr.is_null() {
            self.eof = true;
            return Ok(());
        }

        let src_offset = src_pos as usize;
        let available = self._data.len().saturating_sub(src_offset);
        let copy_len = (to_write as usize).min(available);

        if copy_len == 0 {
            self.eof = true;
            let _ = unsafe { sys::sceMp3NotifyAddStreamData(self.handle, 0) };
            return Ok(());
        }

        // SAFETY: src_offset and copy_len are bounds-checked above.
        unsafe {
            core::ptr::copy_nonoverlapping(
                self._data.as_ptr().add(src_offset),
                dst_ptr,
                copy_len,
            );
        }

        let ret = unsafe { sys::sceMp3NotifyAddStreamData(self.handle, copy_len as i32) };
        if ret < 0 {
            return Err(Mp3Error(ret));
        }

        if src_offset + copy_len >= self._data.len() {
            self.eof = true;
        }

        Ok(())
    }
}

impl Drop for Mp3Decoder {
    fn drop(&mut self) {
        unsafe {
            sys::sceMp3ReleaseMp3Handle(self.handle);
            sys::sceMp3TermResource();
        }
    }
}

// ---------------------------------------------------------------------------
// MP3 frame utilities
// ---------------------------------------------------------------------------

/// Find the next MP3 frame sync position in `data` starting from `offset`.
///
/// An MP3 frame sync is 0xFF followed by a byte with the upper 3 bits set
/// (0xE0 mask). This function additionally validates that the MPEG version
/// and layer fields are not "reserved" values, filtering out false positives.
///
/// Returns `None` if no valid sync is found.
pub fn find_sync(data: &[u8], offset: usize) -> Option<usize> {
    let mut i = offset;
    while i + 1 < data.len() {
        if data[i] == 0xFF && (data[i + 1] & 0xE0) == 0xE0 {
            // Validate MPEG version (bits 4-3) and layer (bits 2-1) are not reserved.
            let version = (data[i + 1] >> 3) & 0x03;
            let layer = (data[i + 1] >> 1) & 0x03;
            if version != 0x01 && layer != 0x00 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Skip an ID3v2 tag at the beginning of `data`.
///
/// If an ID3v2 header is present, returns the byte offset immediately after
/// the tag (header + body). Otherwise returns 0.
///
/// The ID3v2 tag size is encoded as a 28-bit synchsafe integer (4 bytes,
/// 7 bits each) in bytes 6-9 of the header. The total tag size includes the
/// 10-byte header.
pub fn skip_id3v2(data: &[u8]) -> usize {
    // ID3v2 header: "ID3" + version(2) + flags(1) + size(4) = 10 bytes minimum.
    if data.len() < 10 || data[0] != b'I' || data[1] != b'D' || data[2] != b'3' {
        return 0;
    }
    // Synchsafe integer: each byte uses only 7 bits.
    let size = ((data[6] as usize & 0x7F) << 21)
        | ((data[7] as usize & 0x7F) << 14)
        | ((data[8] as usize & 0x7F) << 7)
        | (data[9] as usize & 0x7F);
    // Total = 10-byte header + tag body.
    10 + size
}
