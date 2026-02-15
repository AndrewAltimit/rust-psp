//! High-level `sceAudiocodec` decoder with RAII resource management.
//!
//! Wraps the PSP's hardware audio codec for decoding MP3, AAC, ATRAC3, and
//! ATRAC3plus audio frames. The codec uses the Media Engine (ME) coprocessor
//! and requires EDRAM allocation.
//!
//! # Codec Buffer Layout
//!
//! The `sceAudiocodec*` functions operate on a 65-word (`u32`) buffer with
//! 64-byte alignment. Key fields (reverse-engineered from Sony's `mp3play.prx`):
//!
//! | Index | Set by        | Description                                   |
//! |-------|---------------|-----------------------------------------------|
//! | [3]   | `GetEDRAM`    | EDRAM handle/size                             |
//! | [4]   | `CheckNeedMem`| Required working memory (bytes)               |
//! | [5]   | `GetEDRAM`    | EDRAM pointer (ME memory)                     |
//! | [6]   | caller        | Source buffer pointer                         |
//! | [7]   | caller/codec  | Source length; after decode: bytes consumed    |
//! | [8]   | caller        | Destination buffer pointer                    |
//! | [9]   | caller        | Destination capacity (bytes)                  |
//! | [10]  | caller        | Source length (duplicate of [7], **required**) |
//! | [14]  | `Init`        | Max frame size                                |
//!
//! # Requirements
//!
//! - **User mode**: Call `sceUtilityLoadModule(PSP_MODULE_AV_AVCODEC)` (and
//!   `PSP_MODULE_AV_MPEGBASE` for some codecs) before creating a decoder.
//! - **Kernel mode**: The codec modules are typically loaded by the game.
//!   Source/destination buffers should be in user-accessible memory since the
//!   codec validates pointer ranges.
//!
//! # Example
//!
//! ```ignore
//! use psp::audiocodec::{AudiocodecDecoder, CodecType};
//!
//! let mut decoder = AudiocodecDecoder::new(CodecType::Mp3).unwrap();
//! let mut pcm = [0i16; 1152 * 2]; // stereo
//! let consumed = decoder.decode(&mp3_frame, &mut pcm).unwrap();
//! ```

use alloc::boxed::Box;
use core::marker::PhantomData;

/// Audio codec type identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum CodecType {
    /// ATRAC3plus
    At3Plus = 0x1000,
    /// ATRAC3
    At3 = 0x1001,
    /// MPEG-1 Audio Layer III (MP3)
    Mp3 = 0x1002,
    /// Advanced Audio Coding (AAC)
    Aac = 0x1003,
}

/// Error from an audiocodec operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct AudiocodecError(pub i32);

impl core::fmt::Debug for AudiocodecError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "AudiocodecError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for AudiocodecError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "audiocodec error {:#010x}", self.0 as u32)
    }
}

/// Internal codec buffer with required 64-byte alignment.
#[repr(C, align(64))]
struct CodecBuffer {
    words: [u32; 65],
}

impl CodecBuffer {
    fn new() -> Self {
        Self { words: [0u32; 65] }
    }
}

/// RAII handle to a hardware audio codec decoder.
///
/// Manages EDRAM allocation and codec initialization. Decodes compressed
/// audio frames into PCM samples. The EDRAM is released on drop.
pub struct AudiocodecDecoder {
    buf: Box<CodecBuffer>,
    codec_type: CodecType,
    edram_allocated: bool,
    _marker: PhantomData<*const ()>, // !Send + !Sync
}

impl AudiocodecDecoder {
    /// Create and initialize a decoder for the given codec type.
    ///
    /// Performs `CheckNeedMem` -> `GetEDRAM` -> `Init`. Returns an error if
    /// any step fails (e.g., AVCODEC module not loaded, EDRAM unavailable).
    pub fn new(codec_type: CodecType) -> Result<Self, AudiocodecError> {
        let mut buf = Box::new(CodecBuffer::new());
        let ct = codec_type as i32;

        // Step 1: Query required working memory size.
        let ret = unsafe { crate::sys::sceAudiocodecCheckNeedMem(buf.words.as_mut_ptr(), ct) };
        if ret < 0 {
            return Err(AudiocodecError(ret));
        }

        // Step 2: Allocate EDRAM for the codec's working memory.
        let ret = unsafe { crate::sys::sceAudiocodecGetEDRAM(buf.words.as_mut_ptr(), ct) };
        if ret < 0 {
            return Err(AudiocodecError(ret));
        }

        // Step 3: Initialize the codec.
        let ret = unsafe { crate::sys::sceAudiocodecInit(buf.words.as_mut_ptr(), ct) };
        if ret < 0 {
            // Clean up EDRAM on init failure.
            unsafe { crate::sys::sceAudiocodecReleaseEDRAM(buf.words.as_mut_ptr()) };
            return Err(AudiocodecError(ret));
        }

        Ok(Self {
            buf,
            codec_type,
            edram_allocated: true,
            _marker: PhantomData,
        })
    }

    /// Decode one compressed audio frame.
    ///
    /// `src` is the compressed frame data. `dst` receives interleaved stereo
    /// i16 PCM samples (e.g., 1152*2 for MP3).
    ///
    /// Returns the number of bytes consumed from `src`.
    pub fn decode(&mut self, src: &[u8], dst: &mut [i16]) -> Result<usize, AudiocodecError> {
        let dst_bytes = dst.len() * 2; // i16 -> bytes
        let words = &mut self.buf.words;

        // Set source pointer and length.
        words[6] = src.as_ptr() as u32;
        words[7] = src.len() as u32;
        // Set destination pointer and capacity.
        words[8] = dst.as_mut_ptr() as u32;
        words[9] = dst_bytes as u32;
        // Duplicate source length (required by the codec).
        words[10] = src.len() as u32;

        let ret =
            unsafe { crate::sys::sceAudiocodecDecode(words.as_mut_ptr(), self.codec_type as i32) };
        if ret < 0 {
            return Err(AudiocodecError(ret));
        }

        // words[7] is updated by the codec to reflect bytes actually consumed.
        Ok(words[7] as usize)
    }

    /// Get the codec type.
    pub fn codec_type(&self) -> CodecType {
        self.codec_type
    }

    /// Get the max frame size reported by the codec after initialization.
    ///
    /// Stored in buffer word [14] by `sceAudiocodecInit`.
    pub fn max_frame_size(&self) -> u32 {
        self.buf.words[14]
    }

    /// Get the working memory size required by the codec.
    ///
    /// Stored in buffer word [4] by `sceAudiocodecCheckNeedMem`.
    pub fn needed_mem(&self) -> u32 {
        self.buf.words[4]
    }
}

impl Drop for AudiocodecDecoder {
    fn drop(&mut self) {
        if self.edram_allocated {
            unsafe {
                crate::sys::sceAudiocodecReleaseEDRAM(self.buf.words.as_mut_ptr());
            }
        }
    }
}
