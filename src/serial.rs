//! The USB serial open from `VCXCommOpen`.
//!
//! Type 0 fills a serial parameter block and `SerialCommOpen` opens `\\.\COM{port}`.
//! The read is overlapped, the line is 8N1, DTR is enabled, and a profile baud of 0
//! means 921600. A read after the open returns the bytes already buffered. The dongle
//! does not speak first, and this open does not write a frame.

use std::time::Duration;

/// Baud `VCXCommOpen` uses for a USB link when the profile baud is 0.
pub const USB_BAUD: u32 = 921_600;

const OPEN_ATTEMPTS: u32 = 3;
const OPEN_RETRY_MS: u64 = 200;

const EV_RXCHAR: u32 = 0x0001;
const EV_TXEMPTY: u32 = 0x0004;
const EV_ERR: u32 = 0x0080;
const EVENT_MASK: u32 = EV_RXCHAR | EV_TXEMPTY | EV_ERR;
const QUEUE_BYTES: u32 = 0x1000;

const DSR_FLOW: u32 = 1 << 3;
const DTR_FIELD: u32 = 0b11 << 4;
const DTR_ENABLE: u32 = 1 << 4;
const DSR_SENSITIVITY: u32 = 1 << 6;

/// Flag dword after the edits `SerialCommOpen` makes to the value `GetCommState` returned.
///
/// `fOutxDsrFlow` and `fDsrSensitivity` are cleared. `fDtrControl` is set to enabled.
/// Every other bit is left as it was, including `fRtsControl`.
const fn dcb_flags(flags: u32) -> u32 {
    (flags & !DSR_FLOW & !DSR_SENSITIVITY & !DTR_FIELD) | DTR_ENABLE
}

const fn usb_baud(profile: u32) -> u32 {
    if profile == 0 { USB_BAUD } else { profile }
}

fn device_path(port: u32) -> String {
    format!(r"\\.\COM{port}")
}

struct Setup {
    baud: u32,
    byte_size: u8,
    parity: u8,
    stop_bits: u8,
    read_interval_timeout: u32,
    read_total_timeout_multiplier: u32,
    read_total_timeout_constant: u32,
    write_total_timeout_multiplier: u32,
    write_total_timeout_constant: u32,
    event_mask: u32,
    in_queue: u32,
    out_queue: u32,
}

fn usb_setup(profile_baud: u32) -> Setup {
    Setup {
        baud: usb_baud(profile_baud),
        byte_size: 8,
        parity: 0,
        stop_bits: 0,
        read_interval_timeout: u32::MAX,
        read_total_timeout_multiplier: 0,
        read_total_timeout_constant: 0,
        write_total_timeout_multiplier: 0,
        write_total_timeout_constant: 0,
        event_mask: EVENT_MASK,
        in_queue: QUEUE_BYTES,
        out_queue: QUEUE_BYTES,
    }
}

/// Try `once` up to [`OPEN_ATTEMPTS`] times. `between` runs only when another try follows.
fn attempts<T, E>(
    mut once: impl FnMut() -> Result<T, E>,
    mut between: impl FnMut(),
) -> Result<T, E> {
    const _: () = assert!(OPEN_ATTEMPTS > 0);
    let mut pending = once();
    for _ in 1..OPEN_ATTEMPTS {
        if pending.is_ok() {
            return pending;
        }
        between();
        pending = once();
    }
    pending
}

#[cfg(windows)]
fn to_line(setup: &Setup) -> vcxtoolkit_open::Line {
    vcxtoolkit_open::Line {
        baud: setup.baud,
        byte_size: setup.byte_size,
        parity: setup.parity,
        stop_bits: setup.stop_bits,
        read_interval_timeout: setup.read_interval_timeout,
        read_total_timeout_multiplier: setup.read_total_timeout_multiplier,
        read_total_timeout_constant: setup.read_total_timeout_constant,
        write_total_timeout_multiplier: setup.write_total_timeout_multiplier,
        write_total_timeout_constant: setup.write_total_timeout_constant,
        event_mask: setup.event_mask,
        in_queue: setup.in_queue,
        out_queue: setup.out_queue,
    }
}

#[cfg(windows)]
pub use vcxtoolkit_open::{Call, Error as OpenError, Snapshot as State};

/// An open USB serial port. Dropping it closes the handle.
#[cfg(windows)]
#[derive(Debug)]
pub struct Port {
    com: vcxtoolkit_open::Com,
}

#[cfg(windows)]
impl Port {
    /// The line the driver reported after the open.
    ///
    /// # Errors
    ///
    /// Returns the query that failed.
    pub fn state(&self) -> Result<State, OpenError> {
        self.com.snapshot()
    }

    /// Read the bytes already buffered.
    ///
    /// Zero means the driver had nothing waiting. This does not write to the port.
    ///
    /// # Errors
    ///
    /// Returns the read call that failed.
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, OpenError> {
        self.com.read(buf)
    }

    /// Write bytes.
    ///
    /// The overlapped write is waited out before this returns. A completed write
    /// returns how many bytes the driver accepted. This does not pack a frame.
    ///
    /// # Errors
    ///
    /// Returns the write call that failed.
    pub fn write(&self, buf: &[u8]) -> Result<usize, OpenError> {
        self.com.write(buf)
    }
}

/// Open `\\.\COM{port}` the way `VCXCommOpen` opens a USB link.
///
/// `baud` is the profile baud. Zero selects [`USB_BAUD`]. The handle is overlapped
/// for read and write, the line is 8N1, and DTR is enabled. A failed open is tried
/// three times, with 200 ms between tries. This function does not write a frame.
///
/// # Errors
///
/// Returns the Win32 call that failed. `CreateFile` is the error when the port is
/// not there after three tries.
#[cfg(windows)]
#[must_use = "the port is closed when this value is dropped"]
pub fn open_usb(port: u32, baud: u32) -> Result<Port, OpenError> {
    let path = device_path(port);
    let com = attempts(
        || vcxtoolkit_open::Com::open(&path),
        || std::thread::sleep(Duration::from_millis(OPEN_RETRY_MS)),
    )?;
    let line = to_line(&usb_setup(baud));
    com.configure(&line, dcb_flags)?;
    Ok(Port { com })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{
        OPEN_ATTEMPTS, OPEN_RETRY_MS, USB_BAUD, attempts, dcb_flags, device_path, usb_baud,
        usb_setup,
    };

    #[test]
    fn usb_baud_is_921600_when_the_profile_is_zero() {
        assert_eq!(usb_baud(0), USB_BAUD);
        assert_eq!(USB_BAUD, 921_600);
    }

    #[test]
    fn a_set_profile_baud_is_kept() {
        assert_eq!(usb_baud(115_200), 115_200);
        assert_eq!(usb_setup(115_200).baud, 115_200);
    }

    #[test]
    fn usb_line_is_8n1_with_the_immediate_read_timeout() {
        let line = usb_setup(0);
        assert_eq!(line.baud, 921_600);
        assert_eq!(line.byte_size, 8);
        assert_eq!(line.parity, 0);
        assert_eq!(line.stop_bits, 0);
        assert_eq!(line.read_interval_timeout, u32::MAX);
        assert_eq!(line.read_total_timeout_multiplier, 0);
        assert_eq!(line.read_total_timeout_constant, 0);
        assert_eq!(line.write_total_timeout_multiplier, 0);
        assert_eq!(line.write_total_timeout_constant, 0);
        assert_eq!(line.event_mask, 0x85);
        assert_eq!(line.in_queue, 0x1000);
        assert_eq!(line.out_queue, 0x1000);
    }

    #[test]
    fn device_path_is_the_win32_com_name() {
        assert_eq!(device_path(4), "\\\\.\\COM4");
        assert_eq!(device_path(10), "\\\\.\\COM10");
    }

    #[test]
    fn dtr_is_enabled_and_dsr_flow_is_cleared() {
        let touched = (1 << 3) | (0b11 << 4) | (1 << 6);
        for flags in [0, 0xFFFF_FFFF, 0b11 << 4, 1 << 3, 1 << 6, 0x28] {
            let out = dcb_flags(flags);
            assert_eq!((out >> 4) & 0b11, 1, "{flags:#x}");
            assert_eq!(out & (1 << 3), 0, "{flags:#x}");
            assert_eq!(out & (1 << 6), 0, "{flags:#x}");
            assert_eq!(out & !touched, flags & !touched, "{flags:#x}");
        }
    }

    #[test]
    fn rts_is_left_as_it_was() {
        const RTS: u32 = 0b11 << 12;
        assert_eq!(dcb_flags(RTS) & RTS, RTS);
        assert_eq!(dcb_flags(u32::MAX) & RTS, RTS);
        assert_eq!(dcb_flags(0) & RTS, 0);
    }

    #[test]
    fn a_failed_create_is_tried_three_times() {
        let calls = Cell::new(0_u32);
        let sleeps = Cell::new(0_u32);
        let result: Result<(), &str> = attempts(
            || {
                calls.set(calls.get() + 1);
                Err("missing")
            },
            || sleeps.set(sleeps.get() + 1),
        );
        assert_eq!(result, Err("missing"));
        assert_eq!(calls.get(), OPEN_ATTEMPTS);
        assert_eq!(calls.get(), 3);
        assert_eq!(sleeps.get(), 2);
        assert_eq!(OPEN_RETRY_MS, 200);
    }

    #[test]
    fn a_create_that_works_the_first_time_does_not_sleep() {
        let calls = Cell::new(0_u32);
        let sleeps = Cell::new(0_u32);
        let result = attempts(
            || {
                calls.set(calls.get() + 1);
                Ok::<_, &str>(())
            },
            || sleeps.set(sleeps.get() + 1),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(calls.get(), 1);
        assert_eq!(sleeps.get(), 0);
    }

    #[test]
    fn the_second_try_sleeps_once() {
        let calls = Cell::new(0_u32);
        let sleeps = Cell::new(0_u32);
        let result = attempts(
            || {
                let seen = calls.get();
                calls.set(seen + 1);
                if seen == 0 { Err("busy") } else { Ok("open") }
            },
            || sleeps.set(sleeps.get() + 1),
        );
        assert_eq!(result, Ok("open"));
        assert_eq!(calls.get(), 2);
        assert_eq!(sleeps.get(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn the_line_passed_to_win32_is_the_usb_line() {
        let setup = usb_setup(0);
        let line = super::to_line(&setup);
        assert_eq!(line.baud, setup.baud);
        assert_eq!(line.byte_size, setup.byte_size);
        assert_eq!(line.parity, setup.parity);
        assert_eq!(line.stop_bits, setup.stop_bits);
        assert_eq!(line.read_interval_timeout, setup.read_interval_timeout);
        assert_eq!(
            line.read_total_timeout_multiplier,
            setup.read_total_timeout_multiplier
        );
        assert_eq!(
            line.read_total_timeout_constant,
            setup.read_total_timeout_constant
        );
        assert_eq!(
            line.write_total_timeout_multiplier,
            setup.write_total_timeout_multiplier
        );
        assert_eq!(
            line.write_total_timeout_constant,
            setup.write_total_timeout_constant
        );
        assert_eq!(line.event_mask, setup.event_mask);
        assert_eq!(line.in_queue, setup.in_queue);
        assert_eq!(line.out_queue, setup.out_queue);
    }
}
