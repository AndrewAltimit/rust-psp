//! This example only demonstrates functionality regarding the WLAN chip. It is
//! not a networking example. You might want to look into `psp::net` for actual
//! network access.

#![no_std]
#![no_main]

psp::module!("sample_wlan", 1, 1);

fn psp_main() {
    psp::enable_home_button();

    let status = psp::wlan::status();

    psp::dprintln!(
        "WLAN switch enabled: {}, WLAN active: {}, \
        MAC address: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        status.power_on,
        status.switch_on,
        status.mac_address[0],
        status.mac_address[1],
        status.mac_address[2],
        status.mac_address[3],
        status.mac_address[4],
        status.mac_address[5],
    );
}
