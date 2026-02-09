#![no_std]
#![no_main]

use core::f32::consts::PI;
use core::ffi::c_void;

use psp::sys::{
    AUDIO_NEXT_CHANNEL, AUDIO_VOLUME_MAX, AudioFormat, audio_sample_align, sceAudioChRelease,
    sceAudioChReserve, sceAudioOutputBlocking,
};

psp::module!("audio_tone_example", 1, 1);

const SAMPLE_RATE: f32 = 44100.0;
const TONE_HZ: f32 = 440.0;
const SAMPLE_COUNT: i32 = 1024;
const PLAY_SECONDS: u32 = 3;

fn psp_main() {
    psp::enable_home_button();

    // Reserve an audio channel (stereo, 1024 samples per buffer).
    let channel = unsafe {
        sceAudioChReserve(
            AUDIO_NEXT_CHANNEL,
            audio_sample_align(SAMPLE_COUNT),
            AudioFormat::Stereo,
        )
    };

    if channel < 0 {
        psp::dprintln!("Failed to reserve audio channel: {}", channel);
        return;
    }

    psp::dprintln!(
        "Playing {}Hz tone for {}s on channel {}",
        TONE_HZ,
        PLAY_SECONDS,
        channel
    );

    // Generate and play sine wave buffers.
    let aligned_count = audio_sample_align(SAMPLE_COUNT) as usize;
    let mut buf = [0i16; 2048]; // stereo pairs: 1024 * 2
    let mut phase: f32 = 0.0;
    let phase_inc = 2.0 * PI * TONE_HZ / SAMPLE_RATE;
    let total_buffers = (SAMPLE_RATE as u32 * PLAY_SECONDS) / aligned_count as u32;

    for _ in 0..total_buffers {
        for i in 0..aligned_count {
            let sample = unsafe { (psp::math::sinf(phase) * 16000.0) as i16 };
            buf[i * 2] = sample; // left
            buf[i * 2 + 1] = sample; // right
            phase += phase_inc;
            if phase >= 2.0 * PI {
                phase -= 2.0 * PI;
            }
        }

        unsafe {
            sceAudioOutputBlocking(
                channel,
                AUDIO_VOLUME_MAX as i32,
                buf.as_mut_ptr() as *mut c_void,
            );
        }
    }

    unsafe { sceAudioChRelease(channel) };
    psp::dprintln!("Audio playback complete");
}
