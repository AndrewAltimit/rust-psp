#![no_main]
#![no_std]

psp::module!("sample_time", 1, 1);

fn psp_main() {
    psp::enable_home_button();

    match psp::time::DateTime::now() {
        Ok(now) => {
            psp::dprintln!(
                "Current time is {:02}:{:02}:{:02}",
                now.hour(),
                now.minute(),
                now.second()
            );
        },
        Err(e) => psp::dprintln!("Failed to get time: {:?}", e),
    }
}
