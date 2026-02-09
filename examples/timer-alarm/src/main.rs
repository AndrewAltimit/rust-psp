//! One-shot alarm and virtual timer demonstration.

#![no_std]
#![no_main]

use psp::thread;
use psp::timer::{Alarm, VTimer};

psp::module!("timer_alarm_example", 1, 1);

fn psp_main() {
    psp::enable_home_button();

    // One-shot alarm: fires after 2 seconds.
    psp::dprintln!("Setting alarm for 2 seconds...");
    let _alarm = match Alarm::after_micros(2_000_000, || {
        psp::dprintln!("Alarm fired!");
    }) {
        Ok(a) => a,
        Err(e) => {
            psp::dprintln!("Failed to create alarm: {:?}", e);
            return;
        },
    };

    // Virtual timer: start and read elapsed time.
    let vtimer = match VTimer::new(b"demo_vtimer\0") {
        Ok(v) => v,
        Err(e) => {
            psp::dprintln!("Failed to create VTimer: {:?}", e);
            return;
        },
    };

    if let Err(e) = vtimer.start() {
        psp::dprintln!("Failed to start VTimer: {:?}", e);
        return;
    }

    // Wait 3 seconds so the alarm fires and the timer accumulates.
    thread::sleep_ms(3000);

    let elapsed = vtimer.time_us();
    psp::dprintln!("VTimer elapsed: {} us (~3s expected)", elapsed);

    if let Err(e) = vtimer.stop() {
        psp::dprintln!("Failed to stop VTimer: {:?}", e);
    }

    psp::dprintln!("Timer demo complete.");
}
