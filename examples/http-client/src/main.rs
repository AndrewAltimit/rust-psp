//! High-level HTTP GET using psp::http::HttpClient.
//!
//! Requires a real PSP with WiFi configured in network settings slot 1.
//! Will not work in PPSSPP emulator.

#![no_std]
#![no_main]

use psp::http::HttpClient;
use psp::net;

psp::module!("http_client_example", 1, 1);

fn psp_main() {
    psp::callback::setup_exit_callback().unwrap();

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

    // Create an HTTP client (initializes sceHttp subsystem).
    let client = match HttpClient::new() {
        Ok(c) => c,
        Err(e) => {
            psp::dprintln!("HttpClient::new failed: {:?}", e);
            net::term();
            return;
        },
    };

    // Perform a GET request (URL must be null-terminated).
    psp::dprintln!("Fetching http://example.com/ ...");
    match client.get(b"http://example.com/\0") {
        Ok(resp) => {
            psp::dprintln!("Status: {}", resp.status_code);
            if let Some(len) = resp.content_length {
                psp::dprintln!("Content-Length: {}", len);
            }
            // Print first 256 bytes of the body as text.
            let preview_len = resp.body.len().min(256);
            let text = core::str::from_utf8(&resp.body[..preview_len]).unwrap_or("<binary data>");
            psp::dprintln!("Body preview:\n{}", text);
        },
        Err(e) => psp::dprintln!("GET failed: {:?}", e),
    }

    // Client cleans up sceHttp on drop.
    drop(client);
    net::term();
    psp::dprintln!("Done.");
}
