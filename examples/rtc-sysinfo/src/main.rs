//! RTC and system parameter queries using psp::rtc and psp::system_param.
//!
//! Demonstrates reading system settings (language, nickname, timezone,
//! date/time format) and using the extended RTC API for tick arithmetic,
//! day-of-week, leap year checks, and RFC 3339 formatting.

#![no_std]
#![no_main]

use psp::rtc::{self, Tick};
use psp::system_param;

psp::module!("rtc_sysinfo_example", 1, 1);

const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn psp_main() {
    psp::callback::setup_exit_callback().unwrap();

    // --- System parameters ---
    psp::dprintln!("=== System Parameters ===");

    match system_param::language() {
        Ok(lang) => psp::dprintln!("Language: {:?}", lang),
        Err(e) => psp::dprintln!("Language error: {:?}", e),
    }

    match system_param::nickname() {
        Ok(buf) => {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            let name = core::str::from_utf8(&buf[..end]).unwrap_or("<invalid>");
            psp::dprintln!("Nickname: {}", name);
        },
        Err(e) => psp::dprintln!("Nickname error: {:?}", e),
    }

    match system_param::timezone_offset() {
        Ok(tz) => {
            let hours = tz / 60;
            let mins = (tz % 60).abs();
            psp::dprintln!("Timezone: UTC{:+}:{:02}", hours, mins);
        },
        Err(e) => psp::dprintln!("Timezone error: {:?}", e),
    }

    match system_param::date_format() {
        Ok(fmt) => psp::dprintln!("Date format: {:?}", fmt),
        Err(e) => psp::dprintln!("Date format error: {:?}", e),
    }

    match system_param::time_format() {
        Ok(fmt) => psp::dprintln!("Time format: {:?}", fmt),
        Err(e) => psp::dprintln!("Time format error: {:?}", e),
    }

    match system_param::daylight_saving() {
        Ok(dst) => psp::dprintln!("Daylight saving: {}", if dst { "on" } else { "off" }),
        Err(e) => psp::dprintln!("DST error: {:?}", e),
    }

    // --- RTC operations ---
    psp::dprintln!("\n=== RTC ===");

    let now = match Tick::now() {
        Ok(t) => t,
        Err(e) => {
            psp::dprintln!("Tick::now() failed: {:?}", e);
            return;
        },
    };

    // Current date/time.
    if let Ok(dt) = now.to_datetime() {
        psp::dprintln!(
            "Now: {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            dt.year(),
            dt.month(),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second()
        );

        // Day of week.
        let dow = rtc::day_of_week(dt.year() as i32, dt.month() as i32, dt.day() as i32);
        if (0..7).contains(&dow) {
            psp::dprintln!("Day of week: {}", DAY_NAMES[dow as usize]);
        }

        // Leap year check.
        let year = dt.year() as i32;
        psp::dprintln!(
            "{} is{}a leap year",
            year,
            if rtc::is_leap_year(year) {
                " "
            } else {
                " not "
            }
        );
    }

    // Tick arithmetic: add 3 hours.
    if let Ok(later) = now.add_hours(3) {
        if let Ok(dt) = later.to_datetime() {
            psp::dprintln!(
                "+3 hours: {:02}:{:02}:{:02}",
                dt.hour(),
                dt.minute(),
                dt.second()
            );
        }
    }

    // RFC 3339 local time formatting.
    if let Ok(buf) = rtc::format_rfc3339_local(&now) {
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let s = core::str::from_utf8(&buf[..end]).unwrap_or("<invalid>");
        psp::dprintln!("RFC 3339 local: {}", s);
    }

    psp::dprintln!("Done.");
}
