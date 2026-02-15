//! Audio channel management with RAII for the PSP.
//!
//! Provides [`AudioChannel`] for reserving one of the 8 regular PCM hardware
//! channels, and [`SrcChannel`] for the global Sample Rate Conversion (SRC)
//! channel. The SRC channel is a singleton separate from the 8 PCM channels,
//! making it ideal for background audio in plugins that must not conflict with
//! game audio.
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

    /// Number of i16 elements per sample (2 for stereo, 1 for mono).
    fn channels(self) -> usize {
        match self {
            AudioFormat::Stereo => 2,
            AudioFormat::Mono => 1,
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
    format: AudioFormat,
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
            format,
            _marker: PhantomData,
        })
    }

    /// Output PCM audio data, blocking until the hardware buffer is free.
    ///
    /// `volume` ranges from 0 to 0x8000 (max).
    /// `buf` must contain at least `sample_count * channels` i16 values
    /// (stereo: 2 per sample, mono: 1 per sample).
    ///
    /// Returns [`AudioError`] if `buf` is too short or the hardware call fails.
    pub fn output_blocking(&self, volume: i32, buf: &[i16]) -> Result<(), AudioError> {
        let required = self.sample_count as usize * self.format.channels();
        if buf.len() < required {
            return Err(AudioError(-1));
        }
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
    /// `buf` must contain at least `sample_count * channels` i16 values.
    ///
    /// Returns [`AudioError`] if `buf` is too short or the hardware call fails.
    pub fn output_blocking_panning(
        &self,
        vol_left: i32,
        vol_right: i32,
        buf: &[i16],
    ) -> Result<(), AudioError> {
        let required = self.sample_count as usize * self.format.channels();
        if buf.len() < required {
            return Err(AudioError(-1));
        }
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

    /// Get the audio format (stereo or mono).
    pub fn format(&self) -> AudioFormat {
        self.format
    }
}

impl Drop for AudioChannel {
    fn drop(&mut self) {
        unsafe {
            crate::sys::sceAudioChRelease(self.channel);
        }
    }
}

// ---------------------------------------------------------------------------
// SRC (Sample Rate Conversion) channel
// ---------------------------------------------------------------------------

/// Output frequency for the SRC channel.
///
/// Mirrors [`crate::sys::AudioOutputFrequency`] with a more ergonomic API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFrequency {
    Khz48,
    Khz44_1,
    Khz32,
    Khz24,
    Khz22_05,
    Khz16,
    Khz12,
    Khz11_025,
    Khz8,
}

impl OutputFrequency {
    fn to_sys(self) -> crate::sys::AudioOutputFrequency {
        match self {
            OutputFrequency::Khz48 => crate::sys::AudioOutputFrequency::Khz48,
            OutputFrequency::Khz44_1 => crate::sys::AudioOutputFrequency::Khz44_1,
            OutputFrequency::Khz32 => crate::sys::AudioOutputFrequency::Khz32,
            OutputFrequency::Khz24 => crate::sys::AudioOutputFrequency::Khz24,
            OutputFrequency::Khz22_05 => crate::sys::AudioOutputFrequency::Khz22_05,
            OutputFrequency::Khz16 => crate::sys::AudioOutputFrequency::Khz16,
            OutputFrequency::Khz12 => crate::sys::AudioOutputFrequency::Khz12,
            OutputFrequency::Khz11_025 => crate::sys::AudioOutputFrequency::Khz11_025,
            OutputFrequency::Khz8 => crate::sys::AudioOutputFrequency::Khz8,
        }
    }
}

/// An RAII handle to the PSP's global SRC (Sample Rate Conversion) channel.
///
/// The SRC channel is a **singleton** — there is only one, separate from the
/// 8 regular PCM channels. This makes it ideal for background audio in kernel
/// plugins that must not conflict with game audio channels.
///
/// Audio is always stereo (interleaved i16 L/R pairs).
///
/// # Example
///
/// ```ignore
/// use psp::audio::{SrcChannel, OutputFrequency};
///
/// let src = SrcChannel::reserve(1152, OutputFrequency::Khz44_1).unwrap();
/// src.output_blocking(0x8000, &pcm_stereo).unwrap();
/// // Channel is released on drop.
/// ```
pub struct SrcChannel {
    sample_count: i32,
    _marker: PhantomData<*const ()>, // !Send + !Sync
}

impl SrcChannel {
    /// Reserve the global SRC channel.
    ///
    /// `sample_count` is the number of stereo sample frames per output call
    /// (min 17, max 4111). `freq` sets the output sample rate.
    ///
    /// Returns an error if the SRC channel is already reserved.
    pub fn reserve(sample_count: i32, freq: OutputFrequency) -> Result<Self, AudioError> {
        let ret = unsafe { crate::sys::sceAudioSRCChReserve(sample_count, freq.to_sys(), 2) };
        if ret < 0 {
            return Err(AudioError(ret));
        }
        Ok(Self {
            sample_count,
            _marker: PhantomData,
        })
    }

    /// Output stereo PCM audio, blocking until the hardware buffer is free.
    ///
    /// `volume` ranges from 0 to 0x8000 (max).
    /// `buf` must contain at least `sample_count * 2` i16 values (stereo pairs).
    pub fn output_blocking(&self, volume: i32, buf: &[i16]) -> Result<(), AudioError> {
        let required = self.sample_count as usize * 2;
        if buf.len() < required {
            return Err(AudioError(-1));
        }
        let ret =
            unsafe { crate::sys::sceAudioSRCOutputBlocking(volume, buf.as_ptr() as *mut c_void) };
        if ret < 0 {
            Err(AudioError(ret))
        } else {
            Ok(())
        }
    }

    /// Get the number of sample frames per output call.
    pub fn sample_count(&self) -> i32 {
        self.sample_count
    }
}

impl Drop for SrcChannel {
    fn drop(&mut self) {
        unsafe {
            crate::sys::sceAudioSRCChRelease();
        }
    }
}
