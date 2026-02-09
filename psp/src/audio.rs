//! Audio channel management with RAII for the PSP.
//!
//! Provides [`AudioChannel`] for reserving, outputting to, and
//! automatically releasing PSP hardware audio channels.
//!
//! # Example
//!
//! ```ignore
//! use psp::audio::{AudioChannel, AudioFormat};
//!
//! let ch = AudioChannel::reserve(1024, AudioFormat::Stereo).unwrap();
//! ch.output_blocking(0x8000, &pcm_buf).unwrap();
//! // Channel is released on drop.
//! ```

use core::ffi::c_void;
use core::marker::PhantomData;

/// Audio output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    /// Stereo interleaved (L, R, L, R, ...).
    Stereo,
    /// Mono output.
    Mono,
}

impl AudioFormat {
    fn to_sys(self) -> crate::sys::AudioFormat {
        match self {
            AudioFormat::Stereo => crate::sys::AudioFormat::Stereo,
            AudioFormat::Mono => crate::sys::AudioFormat::Mono,
        }
    }
}

/// Error from an audio operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct AudioError(pub i32);

impl core::fmt::Debug for AudioError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "AudioError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for AudioError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "audio error {:#010x}", self.0 as u32)
    }
}

/// Align a sample count to the PSP hardware requirement (multiple of 64).
pub fn align_sample_count(count: i32) -> i32 {
    crate::sys::audio_sample_align(count)
}

/// An RAII handle to a reserved PSP hardware audio channel.
///
/// Audio data is output via [`output_blocking`](Self::output_blocking) or
/// [`output_blocking_panning`](Self::output_blocking_panning). The channel
/// is automatically released when dropped.
pub struct AudioChannel {
    channel: i32,
    sample_count: i32,
    _marker: PhantomData<*const ()>, // !Send + !Sync
}

impl AudioChannel {
    /// Reserve a hardware audio channel.
    ///
    /// `sample_count` is automatically aligned to a multiple of 64.
    /// Pass `AudioFormat::Stereo` or `AudioFormat::Mono`.
    ///
    /// Returns the channel handle, or an error if no channels are available.
    pub fn reserve(sample_count: i32, format: AudioFormat) -> Result<Self, AudioError> {
        let aligned = align_sample_count(sample_count);
        let ch = unsafe {
            crate::sys::sceAudioChReserve(crate::sys::AUDIO_NEXT_CHANNEL, aligned, format.to_sys())
        };
        if ch < 0 {
            return Err(AudioError(ch));
        }
        Ok(Self {
            channel: ch,
            sample_count: aligned,
            _marker: PhantomData,
        })
    }

    /// Output PCM audio data, blocking until the hardware buffer is free.
    ///
    /// `volume` ranges from 0 to 0x8000 (max).
    /// `buf` must contain at least `sample_count` samples (stereo: 2x i16 per sample).
    pub fn output_blocking(&self, volume: i32, buf: &[i16]) -> Result<(), AudioError> {
        let ret = unsafe {
            crate::sys::sceAudioOutputBlocking(self.channel, volume, buf.as_ptr() as *mut c_void)
        };
        if ret < 0 {
            Err(AudioError(ret))
        } else {
            Ok(())
        }
    }

    /// Output PCM audio with separate left/right volume, blocking.
    ///
    /// `vol_left` and `vol_right` range from 0 to 0x8000.
    pub fn output_blocking_panning(
        &self,
        vol_left: i32,
        vol_right: i32,
        buf: &[i16],
    ) -> Result<(), AudioError> {
        let ret = unsafe {
            crate::sys::sceAudioOutputPannedBlocking(
                self.channel,
                vol_left,
                vol_right,
                buf.as_ptr() as *mut c_void,
            )
        };
        if ret < 0 {
            Err(AudioError(ret))
        } else {
            Ok(())
        }
    }

    /// Change the sample count for this channel.
    ///
    /// The new count is automatically aligned to a multiple of 64.
    pub fn set_sample_count(&mut self, count: i32) -> Result<(), AudioError> {
        let aligned = align_sample_count(count);
        let ret = unsafe { crate::sys::sceAudioSetChannelDataLen(self.channel, aligned) };
        if ret < 0 {
            Err(AudioError(ret))
        } else {
            self.sample_count = aligned;
            Ok(())
        }
    }

    /// Get the number of samples remaining to be played.
    pub fn remaining_samples(&self) -> i32 {
        unsafe { crate::sys::sceAudioGetChannelRestLen(self.channel) }
    }

    /// Get the raw channel number.
    pub fn channel_id(&self) -> i32 {
        self.channel
    }

    /// Get the current sample count per buffer.
    pub fn sample_count(&self) -> i32 {
        self.sample_count
    }
}

impl Drop for AudioChannel {
    fn drop(&mut self) {
        unsafe {
            crate::sys::sceAudioChRelease(self.channel);
        }
    }
}
