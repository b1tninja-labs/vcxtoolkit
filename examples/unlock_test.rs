//! Live bench test of the firmware A0 unlock. Opens a COM port, reads the licence,
//! then sends the A0 command with the recovered `unlock_key` constants and checks
//! whether `VCX_PRO_Open` subsequently returns status `0x00` (unlocked) instead of
//! `0xFE` (locked). Windows-only. Usage: `cargo run --example unlock_test [COM]`.

#[cfg(windows)]
fn main() {
    use std::time::Duration;
    use vcxtoolkit::{exchange, open_usb, read_license, unlock_key, unlock_key_r};

    const A0: [u8; 4] = [0x80, 0x00, 0xA0, 0x00];
    const PRO_OPEN: [u8; 4] = [0x80, 0x00, 0x40, 0x00];
    const PRO_OPEN_ARG: [u8; 4] = [0x00, 0x00, 0x80, 0x01];

    let port: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let timeout = Duration::from_secs(3);

    let serial = match open_usb(port, 0) {
        Ok(serial) => serial,
        Err(error) => {
            eprintln!("could not open COM{port}: {error}");
            std::process::exit(1);
        }
    };
    std::thread::sleep(Duration::from_millis(400));
    drain(&serial);

    match read_license(&serial, timeout) {
        Ok(info) => println!("licence OK  serial {:02X?}", info.serial),
        Err(error) => println!("licence read failed: {error}"),
    }

    for (label, key) in [
        ("unlock_key  (FINAL_5)", unlock_key()),
        ("unlock_key_r(FINAL_R)", unlock_key_r()),
    ] {
        drain(&serial);
        println!("\n--- A0 unlock with {label} = {key:02X?} ---");
        match exchange(&serial, A0, &key, timeout) {
            Ok(frame) => println!(
                "  A0 reply: header {:02X?} payload {:02X?}",
                frame.header, frame.payload
            ),
            Err(error) => println!("  A0 exchange failed: {error}"),
        }
        match exchange(&serial, PRO_OPEN, &PRO_OPEN_ARG, timeout) {
            Ok(frame) => {
                let status = frame.payload.first().copied().unwrap_or(0xFF);
                println!(
                    "  VCX_PRO_Open: payload {:02X?}  => {}",
                    frame.payload,
                    if status == 0x00 {
                        "UNLOCKED (status 00)"
                    } else {
                        "still locked"
                    }
                );
            }
            Err(error) => println!("  VCX_PRO_Open failed: {error}"),
        }
    }
}

#[cfg(windows)]
fn drain(port: &vcxtoolkit::Port) {
    let mut scratch = [0_u8; 256];
    for _ in 0..64 {
        match port.read(&mut scratch) {
            Ok(n) if n > 0 => {}
            _ => break,
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("unlock_test is Windows-only (it opens a COM port)");
}
