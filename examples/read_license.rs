//! Read and print the on-device VCX licence over a COM port. Bench diagnostic only;
//! it sends `GetInfo` + the `80 00 A0` licence challenge (the device hello) and decodes
//! the licence buffer. Usage: `cargo run --example read_license [COM-number]`.

#[cfg(windows)]
fn main() {
    use std::time::Duration;

    let port: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    println!("opening COM{port} ...");
    let serial = match vcxtoolkit::open_usb(port, 0) {
        Ok(serial) => serial,
        Err(error) => {
            eprintln!("could not open COM{port}: {error}");
            std::process::exit(1);
        }
    };
    // Settle after open, then drain boot bytes (mirrors the gmdiag hello).
    std::thread::sleep(Duration::from_millis(400));
    let mut scratch = [0u8; 256];
    for _ in 0..64 {
        match serial.read(&mut scratch) {
            Ok(n) if n > 0 => {}
            _ => break,
        }
    }
    match vcxtoolkit::read_license(&serial, Duration::from_secs(3)) {
        Ok(info) => {
            println!("licence read OK:");
            println!("  serial         : {:02X?}", info.serial);
            println!("  issued         : {}", info.issued);
            println!("  updated        : {}", info.updated);
            println!("  period_seconds : {}", info.period_seconds);
            for product in &info.products {
                println!("  product        : {} ({})", product.name, product.date);
            }
        }
        Err(error) => eprintln!("licence read failed: {error}"),
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("read_license is Windows-only (it opens a COM port)");
}
