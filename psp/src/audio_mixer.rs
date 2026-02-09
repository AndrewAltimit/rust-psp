//! Audio mixing engine for the PSP.
//!
//! Provides a multi-channel PCM audio mixer that can run on the main CPU
//! or (in kernel mode) offload mixing to the Media Engine. The mixer
//! accepts PCM streams from multiple sources, handles volume, panning,
//! and fade in/out, and writes mixed output to the PSP audio hardware.
//!
//! # Architecture
//!
//! The mixer uses a double-buffered approach:
//! 1. The main CPU submits audio data to channels
//! 2. The mixing callback reads all active channels, mixes them, and
//!    writes to the output buffer
//! 3. The output buffer is submitted to the PSP audio hardware via
//!    `sceAudioOutputBlocking`
//!
//! # Example
//!
//! ```ignore
//! use psp::audio_mixer::{Mixer, ChannelConfig};
//!
//! let mut mixer = Mixer::new(1024).unwrap();
//!
//! let ch = mixer.alloc_channel(ChannelConfig {
//!     volume_left: 0x6000,
//!     volume_right: 0x6000,
//!     ..Default::default()
//! }).unwrap();
//!
//! mixer.submit_samples(ch, &pcm_data);
//! mixer.start();
//! ```

use crate::sync::SpinMutex;
use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

/// Maximum number of mixer channels.
pub const MAX_CHANNELS: usize = 8;

/// Default sample count per audio output call (must be 64-aligned).
pub const DEFAULT_SAMPLE_COUNT: i32 = 1024;

/// Channel state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ChannelState {
    /// Channel is free and can be allocated.
    Free = 0,
    /// Channel is allocated but has no data queued.
    Idle = 1,
    /// Channel is actively playing audio.
    Playing = 2,
    /// Channel is fading out and will become idle when done.
    FadingOut = 3,
}

/// Configuration for a mixer channel.
#[derive(Debug, Clone, Copy)]
pub struct ChannelConfig {
    /// Left channel volume (0..=0x8000).
    pub volume_left: i32,
    /// Right channel volume (0..=0x8000).
    pub volume_right: i32,
    /// Whether to loop when the buffer runs out.
    pub looping: bool,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            volume_left: 0x8000,
            volume_right: 0x8000,
            looping: false,
        }
    }
}

/// A handle to a mixer channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelHandle(pub u8);

/// Full-volume value for the fade multiplier.
const FADE_MAX: i32 = 256;

/// Fixed-point fractional bits for fade arithmetic (16.16).
const FADE_FP_SHIFT: i32 = 16;

/// Full volume in fixed-point representation (`256 << 16`).
const FADE_MAX_FP: i32 = FADE_MAX << FADE_FP_SHIFT;

/// Per-channel state stored in the mixer.
struct Channel {
    state: ChannelState,
    config: ChannelConfig,
    /// PCM sample buffer (interleaved stereo i16: L, R, L, R, ...)
    buffer: &'static [i16],
    /// Current read position in the buffer (in samples, not bytes).
    position: usize,
    /// Fade volume multiplier in 16.16 fixed-point (0..=FADE_MAX_FP).
    fade_level: i32,
    /// Fade step per output frame in 16.16 fixed-point (negative = fade out).
    fade_step: i32,
}

impl Channel {
    const fn new() -> Self {
        Self {
            state: ChannelState::Free,
            config: ChannelConfig {
                volume_left: 0x8000,
                volume_right: 0x8000,
                looping: false,
            },
            buffer: &[],
            position: 0,
            fade_level: FADE_MAX_FP,
            fade_step: 0,
        }
    }
}

/// Multi-channel PCM audio mixer.
///
/// Manages up to [`MAX_CHANNELS`] concurrent audio streams and mixes
/// them into a single stereo output buffer for the PSP audio hardware.
pub struct Mixer {
    channels: SpinMutex<[Channel; MAX_CHANNELS]>,
    /// Number of samples per output call (64-aligned).
    sample_count: i32,
    /// Hardware channel ID from sceAudioChReserve.
    hw_channel: AtomicI32,
    /// Master volume (0..=0x8000).
    master_volume: AtomicU32,
}

// SAFETY: Mixer uses internal synchronization (SpinMutex + atomics).
unsafe impl Sync for Mixer {}
unsafe impl Send for Mixer {}

/// Error type for mixer operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerError {
    /// No free channels available.
    NoFreeChannels,
    /// The specified channel handle is invalid.
    InvalidChannel,
    /// The PSP audio hardware returned an error.
    AudioError(i32),
    /// The mixer is already running.
    AlreadyRunning,
}

impl Mixer {
    /// Create a new mixer with the given sample count per output call.
    ///
    /// `sample_count` must be between 64 and 65472, aligned to 64.
    /// Use [`DEFAULT_SAMPLE_COUNT`] (1024) for a good balance of
    /// latency and efficiency.
    pub fn new(sample_count: i32) -> Result<Self, MixerError> {
        let sample_count = crate::sys::audio_sample_align(sample_count);

        Ok(Self {
            channels: SpinMutex::new([const { Channel::new() }; MAX_CHANNELS]),
            sample_count,
            hw_channel: AtomicI32::new(-1),
            master_volume: AtomicU32::new(0x8000),
        })
    }

    /// Allocate a mixer channel with the given configuration.
    ///
    /// Returns a [`ChannelHandle`] for submitting samples and controlling
    /// the channel.
    pub fn alloc_channel(&self, config: ChannelConfig) -> Result<ChannelHandle, MixerError> {
        let mut channels = self.channels.lock();
        for (i, ch) in channels.iter_mut().enumerate() {
            if ch.state == ChannelState::Free {
                ch.state = ChannelState::Idle;
                ch.config = config;
                ch.buffer = &[];
                ch.position = 0;
                ch.fade_level = FADE_MAX_FP;
                ch.fade_step = 0;
                return Ok(ChannelHandle(i as u8));
            }
        }
        Err(MixerError::NoFreeChannels)
    }

    /// Free a mixer channel.
    pub fn free_channel(&self, handle: ChannelHandle) -> Result<(), MixerError> {
        let mut channels = self.channels.lock();
        let ch = channels
            .get_mut(handle.0 as usize)
            .ok_or(MixerError::InvalidChannel)?;
        ch.state = ChannelState::Free;
        ch.buffer = &[];
        ch.position = 0;
        Ok(())
    }

    /// Submit PCM samples to a channel.
    ///
    /// `samples` must be interleaved stereo i16 data (L, R, L, R, ...).
    /// The buffer must live for at least as long as the channel is playing
    /// (use `'static` lifetime or ensure it's pinned).
    ///
    /// # Safety
    ///
    /// The caller must ensure `samples` remains valid for the lifetime of
    /// playback. Passing stack-allocated data will cause use-after-free.
    pub unsafe fn submit_samples(
        &self,
        handle: ChannelHandle,
        samples: &'static [i16],
    ) -> Result<(), MixerError> {
        let mut channels = self.channels.lock();
        let ch = channels
            .get_mut(handle.0 as usize)
            .ok_or(MixerError::InvalidChannel)?;
        if ch.state == ChannelState::Free {
            return Err(MixerError::InvalidChannel);
        }
        ch.buffer = samples;
        ch.position = 0;
        ch.state = ChannelState::Playing;
        Ok(())
    }

    /// Set the volume for a channel.
    pub fn set_channel_volume(
        &self,
        handle: ChannelHandle,
        left: i32,
        right: i32,
    ) -> Result<(), MixerError> {
        let mut channels = self.channels.lock();
        let ch = channels
            .get_mut(handle.0 as usize)
            .ok_or(MixerError::InvalidChannel)?;
        ch.config.volume_left = left;
        ch.config.volume_right = right;
        Ok(())
    }

    /// Start a fade-out on a channel.
    ///
    /// `frames` is the number of output frames over which to fade.
    /// After the fade completes, the channel transitions to `Idle`.
    pub fn fade_out(&self, handle: ChannelHandle, frames: u16) -> Result<(), MixerError> {
        let mut channels = self.channels.lock();
        let ch = channels
            .get_mut(handle.0 as usize)
            .ok_or(MixerError::InvalidChannel)?;
        if frames == 0 {
            ch.fade_level = 0;
            ch.state = ChannelState::Idle;
        } else {
            ch.fade_step = -(FADE_MAX_FP / frames as i32);
            ch.state = ChannelState::FadingOut;
        }
        Ok(())
    }

    /// Start a fade-in on a channel.
    pub fn fade_in(&self, handle: ChannelHandle, frames: u16) -> Result<(), MixerError> {
        let mut channels = self.channels.lock();
        let ch = channels
            .get_mut(handle.0 as usize)
            .ok_or(MixerError::InvalidChannel)?;
        if frames == 0 {
            ch.fade_level = FADE_MAX_FP;
        } else {
            ch.fade_level = 0;
            ch.fade_step = FADE_MAX_FP / frames as i32;
        }
        Ok(())
    }

    /// Set master volume (0..=0x8000).
    pub fn set_master_volume(&self, volume: u32) {
        self.master_volume
            .store(volume.min(0x8000), Ordering::Relaxed);
    }

    /// Get master volume.
    pub fn master_volume(&self) -> u32 {
        self.master_volume.load(Ordering::Relaxed)
    }

    /// Mix all active channels into the output buffer.
    ///
    /// `output` must have space for `sample_count * 2` i16 values
    /// (interleaved stereo).
    pub fn mix_into(&self, output: &mut [i16]) {
        // Clear the output buffer
        for sample in output.iter_mut() {
            *sample = 0;
        }

        let master_vol = self.master_volume.load(Ordering::Relaxed) as i32;
        let mut channels = self.channels.lock();

        for ch in channels.iter_mut() {
            if ch.state != ChannelState::Playing && ch.state != ChannelState::FadingOut {
                continue;
            }

            if ch.buffer.is_empty() {
                ch.state = ChannelState::Idle;
                continue;
            }

            let vol_l = ch.config.volume_left;
            let vol_r = ch.config.volume_right;
            let fade = ch.fade_level >> FADE_FP_SHIFT;

            // Mix this channel's samples into the output
            let stereo_samples = output.len() / 2;
            for i in 0..stereo_samples {
                let buf_pos = ch.position * 2; // stereo pairs

                if buf_pos + 1 >= ch.buffer.len() {
                    if ch.config.looping {
                        ch.position = 0;
                    } else {
                        ch.state = ChannelState::Idle;
                        break;
                    }
                    continue;
                }

                let src_l = ch.buffer[buf_pos] as i32;
                let src_r = ch.buffer[buf_pos + 1] as i32;

                // Apply channel volume, fade, and master volume.
                // Use i64 intermediates to prevent overflow when
                // src ~ 32000 and vol = 0x8000.
                let mixed_l = (src_l as i64 * vol_l as i64 / 0x8000 * fade as i64 / 256
                    * master_vol as i64
                    / 0x8000)
                    .clamp(i16::MIN as i64, i16::MAX as i64) as i16;
                let mixed_r = (src_r as i64 * vol_r as i64 / 0x8000 * fade as i64 / 256
                    * master_vol as i64
                    / 0x8000)
                    .clamp(i16::MIN as i64, i16::MAX as i64) as i16;

                // Saturating add to output
                let out_idx = i * 2;
                output[out_idx] = output[out_idx].saturating_add(mixed_l);
                output[out_idx + 1] = output[out_idx + 1].saturating_add(mixed_r);

                ch.position += 1;
            }

            // Update fade
            if ch.state == ChannelState::FadingOut {
                let new_fade = ch.fade_level + ch.fade_step;
                if new_fade <= 0 {
                    ch.fade_level = 0;
                    ch.state = ChannelState::Idle;
                } else {
                    ch.fade_level = new_fade;
                }
            } else if ch.fade_step > 0 {
                let new_fade = ch.fade_level + ch.fade_step;
                if new_fade >= FADE_MAX_FP {
                    ch.fade_level = FADE_MAX_FP;
                    ch.fade_step = 0;
                } else {
                    ch.fade_level = new_fade;
                }
            }
        }
    }

    /// Reserve a hardware audio channel.
    ///
    /// Must be called before [`output_blocking`](Self::output_blocking).
    pub fn reserve_hw_channel(&self) -> Result<(), MixerError> {
        let ch = unsafe {
            crate::sys::sceAudioChReserve(
                crate::sys::AUDIO_NEXT_CHANNEL,
                self.sample_count,
                crate::sys::AudioFormat::Stereo,
            )
        };
        if ch < 0 {
            return Err(MixerError::AudioError(ch));
        }
        self.hw_channel.store(ch, Ordering::Release);
        Ok(())
    }

    /// Release the hardware audio channel.
    pub fn release_hw_channel(&self) {
        let ch = self.hw_channel.swap(-1, Ordering::AcqRel);
        if ch >= 0 {
            unsafe {
                crate::sys::sceAudioChRelease(ch);
            }
        }
    }

    /// Output the given buffer to the audio hardware (blocking).
    ///
    /// The buffer must contain at least `sample_count * 2` i16 samples
    /// (interleaved stereo). Returns [`MixerError::AudioError`] if the
    /// buffer is too small. This call blocks until the hardware is
    /// ready for the next buffer.
    pub fn output_blocking(&self, buffer: &[i16]) -> Result<(), MixerError> {
        let required = self.sample_count as usize * 2; // stereo
        if buffer.len() < required {
            return Err(MixerError::AudioError(-1));
        }
        let ch = self.hw_channel.load(Ordering::Acquire);
        if ch < 0 {
            return Err(MixerError::AudioError(-1));
        }
        let ret = unsafe {
            crate::sys::sceAudioOutputPannedBlocking(
                ch,
                0x8000, // full left
                0x8000, // full right
                buffer.as_ptr() as *mut core::ffi::c_void,
            )
        };
        if ret < 0 {
            Err(MixerError::AudioError(ret))
        } else {
            Ok(())
        }
    }

    /// Get the configured sample count per output call.
    pub fn sample_count(&self) -> i32 {
        self.sample_count
    }
}

impl Drop for Mixer {
    fn drop(&mut self) {
        self.release_hw_channel();
    }
}
