#![no_std]
#![no_main]

psp::module!("file_io_example", 1, 1);

fn psp_main() {
    psp::enable_home_button();

    let path = "host0:/test_output.txt";
    let message = b"Hello from rust-psp file I/O!";

    // Write a message to a file.
    if let Err(e) = psp::io::write_bytes(path, message) {
        psp::dprintln!("Failed to write file: {:?}", e);
        return;
    }
    psp::dprintln!("Wrote {} bytes", message.len());

    // Read the file back.
    match psp::io::read_to_vec(path) {
        Ok(data) => {
            let text = core::str::from_utf8(&data).unwrap_or("<invalid utf8>");
            psp::dprintln!("Read back: {}", text);
        },
        Err(e) => psp::dprintln!("Failed to read file: {:?}", e),
    }
}
