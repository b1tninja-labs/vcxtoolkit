//! The device/channel session the FFI exports hold between calls.
//!
//! This is the safe state layer: it owns the open Windows [`vcxtoolkit::Port`],
//! runs the recovered [`crate::native::Link`] sequence against it, and hands the
//! FFI a plain [`crate::status::Status`]. It never touches raw pointers (that is
//! [`crate::ffi`]), and nothing here opens a port except [`open`] — so a unit test
//! that does not call [`open`] never touches COM4 or the owner's dongle.
//!
//! On a non-Windows build the serial port does not exist, so every entry point is
//! a stub returning `ERR_DEVICE_NOT_CONNECTED`; the crate still compiles as a
//! `cdylib` everywhere, and the real driver is the Windows one.

#[cfg(windows)]
mod imp {
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use vcxtoolkit::{Deadline, ExchangeError, open_usb};

    use crate::native::{Link, LinkError};
    use crate::status::{
        ERR_BUFFER_EMPTY, ERR_DEVICE_NOT_CONNECTED, ERR_FAILED, ERR_NOT_SUPPORTED, STATUS_NOERROR,
        Status,
    };

    /// The GM HS-GMLAN diagnostic baud used when the app passes 0.
    pub const DEFAULT_BAUD: u32 = 500_000;
    /// The COM port the VCX-NANO enumerates as, used when no name is given.
    pub const DEFAULT_PORT: u32 = 4;

    const TIMEOUT: Duration = Duration::from_secs(2);

    /// One open device and its single ISO 15765 channel.
    struct Session {
        port: vcxtoolkit::Port,
        link: Link,
        connected: bool,
        next_filter: u8,
    }

    fn cell() -> &'static Mutex<Option<Session>> {
        static CELL: OnceLock<Mutex<Option<Session>>> = OnceLock::new();
        CELL.get_or_init(|| Mutex::new(None))
    }

    /// Open the dongle on `port` and store the session.
    pub fn open(port: u32) -> Status {
        match open_usb(port, 0) {
            Ok(opened) => {
                let mut guard = cell().lock().expect("session lock");
                *guard = Some(Session {
                    port: opened,
                    link: Link::default(),
                    connected: false,
                    next_filter: 0,
                });
                STATUS_NOERROR
            }
            Err(_) => ERR_DEVICE_NOT_CONNECTED,
        }
    }

    /// Drop the open session (closing the port).
    pub fn close() -> Status {
        *cell().lock().expect("session lock") = None;
        STATUS_NOERROR
    }

    /// Run the recovered ISO 15765 connect bring-up.
    pub fn connect(protocol: u32, baud: u32) -> Status {
        if protocol != 6 {
            // Only ISO 15765 has the recovered GM bring-up wired natively.
            return ERR_NOT_SUPPORTED;
        }
        let mut guard = cell().lock().expect("session lock");
        let Some(session) = guard.as_mut() else {
            return ERR_DEVICE_NOT_CONNECTED;
        };
        let baud = if baud == 0 { DEFAULT_BAUD } else { baud };
        match session.link.connect_timed(&session.port, baud, TIMEOUT) {
            Ok(()) => {
                session.connected = true;
                STATUS_NOERROR
            }
            Err(_) => ERR_FAILED,
        }
    }

    /// Install one flow-control filter; returns its J2534 filter id.
    pub fn add_flow_filter(pattern: &[u8], mask: &[u8], flow: &[u8]) -> Result<u32, Status> {
        let mut guard = cell().lock().expect("session lock");
        let Some(session) = guard.as_mut() else {
            return Err(ERR_DEVICE_NOT_CONNECTED);
        };
        let slot = session.next_filter;
        match session.link.add_flow_filter(
            &session.port,
            slot,
            pattern,
            mask,
            flow,
            &mut Deadline::new(TIMEOUT),
        ) {
            Ok(()) => {
                session.next_filter = slot.wrapping_add(1);
                // start_filter's returned id is the slot plus 1.
                Ok(u32::from(slot) + 1)
            }
            Err(_) => Err(ERR_FAILED),
        }
    }

    /// Send one ISO 15765 message (the app's `[CAN id][PDU]` data).
    pub fn write(data: &[u8]) -> Status {
        let mut guard = cell().lock().expect("session lock");
        let Some(session) = guard.as_mut() else {
            return ERR_DEVICE_NOT_CONNECTED;
        };
        match session
            .link
            .send(&session.port, data, &mut Deadline::new(TIMEOUT))
        {
            Ok(_status) => STATUS_NOERROR,
            Err(_) => ERR_FAILED,
        }
    }

    /// Read one queued reply's application bytes, or an error status.
    pub fn read() -> Result<Vec<u8>, Status> {
        let mut guard = cell().lock().expect("session lock");
        let Some(session) = guard.as_mut() else {
            return Err(ERR_DEVICE_NOT_CONNECTED);
        };
        match session
            .link
            .read(&session.port, &mut Deadline::new(TIMEOUT))
        {
            Ok(app) => Ok(app),
            Err(LinkError::Read(ExchangeError::Timeout)) => Err(ERR_BUFFER_EMPTY),
            Err(_) => Err(ERR_FAILED),
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use crate::status::{ERR_DEVICE_NOT_CONNECTED, STATUS_NOERROR, Status};

    pub const DEFAULT_BAUD: u32 = 500_000;
    pub const DEFAULT_PORT: u32 = 4;

    pub fn open(_port: u32) -> Status {
        ERR_DEVICE_NOT_CONNECTED
    }
    pub fn close() -> Status {
        STATUS_NOERROR
    }
    pub fn connect(_protocol: u32, _baud: u32) -> Status {
        ERR_DEVICE_NOT_CONNECTED
    }
    pub fn add_flow_filter(_pattern: &[u8], _mask: &[u8], _flow: &[u8]) -> Result<u32, Status> {
        Err(ERR_DEVICE_NOT_CONNECTED)
    }
    pub fn write(_data: &[u8]) -> Status {
        ERR_DEVICE_NOT_CONNECTED
    }
    pub fn read() -> Result<Vec<u8>, Status> {
        Err(ERR_DEVICE_NOT_CONNECTED)
    }
}

pub(crate) use imp::{DEFAULT_PORT, add_flow_filter, close, connect, open, read, write};
