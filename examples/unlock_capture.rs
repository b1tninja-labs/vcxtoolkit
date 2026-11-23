//! Capture vendor ground-truth bytes for the unlock-family opcodes. Bench diagnostic only.
//!
//! Sends `GetInfo`, then the licence challenge `80 00 A0 00` with a fresh nonce,
//! then empty `GetLicense`, then the three opcodes the firmware verifier uses as
//! reply/next-command bytes: `84`, `A1`, `A2` (empty payloads). Every request and
//! every reply frame is printed as hex so the bytes can be pinned as a fixture.
//! Usage: `cargo run --example unlock_capture [COM-number]`.

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
    // Settle after open, then drain boot bytes (mirrors the read_license hello).
    std::thread::sleep(Duration::from_millis(400));
    let mut scratch = [0u8; 256];
    for _ in 0..64 {
        match serial.read(&mut scratch) {
            Ok(n) if n > 0 => {}
            _ => break,
        }
    }

    let timeout = Duration::from_secs(3);
    let secret: u64 = 0x0123_4567_89AB_CDEF;

    let probes: [(&str, [u8; 4], Vec<u8>); 6] = [
        ("GetInfo", [0x80, 0x00, 0x8C, 0x00], vec![]),
        (
            "A0-challenge",
            [0x80, 0x00, 0xA0, 0x00],
            vcxtoolkit::license_nonce(secret).to_vec(),
        ),
        ("GetLicense", [0x80, 0x00, 0x83, 0x00], vec![]),
        ("opcode-84", [0x80, 0x00, 0x84, 0x00], vec![]),
        ("opcode-A1", [0x80, 0x00, 0xA1, 0x00], vec![]),
        ("opcode-A2", [0x80, 0x00, 0xA2, 0x00], vec![]),
    ];

    for (label, header, payload) in probes {
        println!("--- {label}: request {header:02X?} + {payload:02X?}");
        match vcxtoolkit::exchange(&serial, header, &payload, timeout) {
            Ok(frame) => println!(
                "    reply header {:02X?} payload({}) {:02X?}",
                frame.header,
                frame.payload.len(),
                frame.payload
            ),
            Err(error) => println!("    no reply: {error}"),
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("unlock_capture is Windows-only (it opens a COM port)");
}
