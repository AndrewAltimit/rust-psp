//! Connect to WiFi and fetch an HTTP response.
//!
//! Requires a real PSP with WiFi configured in network settings slot 1.
//! Will not work in PPSSPP emulator.

#![no_std]
#![no_main]

use psp::net::{self, TcpStream};

psp::module!("net_http_example", 1, 1);

fn psp_main() {
    psp::enable_home_button();

    // Initialize networking subsystem (256 KiB pool).
    if let Err(e) = net::init(256 * 1024) {
        psp::dprintln!("net::init failed: {:?}", e);
        return;
    }

    // Connect to WiFi access point (slot 1).
    psp::dprintln!("Connecting to WiFi...");
    if let Err(e) = net::connect_ap(1) {
        psp::dprintln!("connect_ap failed: {:?}", e);
        net::term();
        return;
    }
    psp::dprintln!("WiFi connected.");

    // Resolve hostname.
    let host = b"example.com\0";
    let addr = match net::resolve_hostname(host) {
        Ok(a) => a,
        Err(e) => {
            psp::dprintln!("DNS resolve failed: {:?}", e);
            net::term();
            return;
        },
    };

    // TCP connect to port 80.
    let stream = match TcpStream::connect(addr, 80) {
        Ok(s) => s,
        Err(e) => {
            psp::dprintln!("TCP connect failed: {:?}", e);
            net::term();
            return;
        },
    };

    // Send HTTP GET request.
    let request = b"GET / HTTP/1.0\r\nHost: example.com\r\n\r\n";
    if let Err(e) = stream.write(request) {
        psp::dprintln!("write failed: {:?}", e);
        net::term();
        return;
    }

    // Read and print response (first 512 bytes).
    let mut buf = [0u8; 512];
    match stream.read(&mut buf) {
        Ok(n) => {
            let text = core::str::from_utf8(&buf[..n]).unwrap_or("<binary data>");
            psp::dprintln!("Response ({} bytes):\n{}", n, text);
        },
        Err(e) => psp::dprintln!("read failed: {:?}", e),
    }

    // Stream closed on drop, then terminate networking.
    drop(stream);
    net::term();
    psp::dprintln!("Done.");
}
