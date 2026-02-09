#![no_std]
#![no_main]

use core::f32::consts::PI;

use psp::audio::{AudioChannel, AudioFormat};
use psp::sys::AUDIO_VOLUME_MAX;

psp::module!("audio_tone_example", 1, 1);

const SAMPLE_RATE: f32 = 44100.0;
const TONE_HZ: f32 = 440.0;
const SAMPLE_COUNT: i32 = 1024;
const PLAY_SECONDS: u32 = 3;

fn psp_main() {
    psp::enable_home_button();

    let channel = match AudioChannel::reserve(SAMPLE_COUNT, AudioFormat::Stereo) {
        Ok(ch) => ch,
        Err(e) => {
            psp::dprintln!("Failed to reserve audio channel: {:?}", e);
            return;
        },
    };

    psp::dprintln!(
        "Playing {}Hz tone for {}s on channel {}",
        TONE_HZ,
        PLAY_SECONDS,
        channel.channel_id()
    );

    let aligned_count = channel.sample_count() as usize;
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

        if let Err(e) = channel.output_blocking(AUDIO_VOLUME_MAX as i32, &buf) {
            psp::dprintln!("Audio output error: {:?}", e);
            return;
        }
    }

    // Channel is released on drop
    psp::dprintln!("Audio playback complete");
}
