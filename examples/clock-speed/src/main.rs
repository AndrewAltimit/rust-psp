#![no_std]
#![no_main]

psp::module!("sample_clock_speed", 1, 1);

fn psp_main() {
    psp::enable_home_button();

    let clock = psp::power::get_clock();
    psp::dprintln!("PSP is operating at {}/{}MHz", clock.cpu_mhz, clock.bus_mhz);
    psp::dprintln!("Setting clock speed to maximum...");

    match psp::power::set_clock_frequency(333, 166, 333) {
        Ok(()) => {
            let clock = psp::power::get_clock();
            psp::dprintln!(
                "PSP is now operating at {}/{}MHz",
                clock.cpu_mhz,
                clock.bus_mhz
            );
        },
        Err(e) => psp::dprintln!("Failed to set clock: {:?}", e),
    }
}
