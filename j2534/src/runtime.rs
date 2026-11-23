//! The device/channel session the FFI exports hold between calls.
//!
//! This is the safe state layer: it owns the open Windows [`vcxtoolkit::Port`],
//! drives the recovered bring-up through the reusable [`vcxtoolkit::Channel`]
//! primitives (`connect_with`/`add_flow_control_filter_with`/`send_raw_with`/
//! `read_msg_with`), and hands the FFI a plain [`crate::status::Status`]. It never
//! touches raw pointers (that is [`crate::ffi`]), and nothing here opens a port
//! except [`open`] — so a unit test that does not call [`open`] never touches COM4
//! or the owner's dongle.
//!
//! The channel bring-up lives in the `vcxtoolkit` library (one copy, reused by any
//! consumer), not in this crate; this module only marshals the J2534 lifecycle
//! onto it. On a non-Windows build the serial port does not exist, so every entry
//! point is a stub returning `ERR_DEVICE_NOT_CONNECTED`; the crate still compiles
//! as a `cdylib` everywhere.

#[cfg(windows)]
mod imp {
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use vcxtoolkit::collect::{ISO15765_PROTOCOL, reply_application_bytes};
    use vcxtoolkit::{Channel, Deadline, DeviceError, ExchangeError, open_usb};

    use crate::status::{
        ERR_BUFFER_EMPTY, ERR_DEVICE_NOT_CONNECTED, ERR_FAILED, ERR_INVALID_MSG, ERR_NOT_SUPPORTED,
        STATUS_NOERROR, Status,
    };

    /// The GM HS-GMLAN diagnostic baud used when the app passes 0.
    pub const DEFAULT_BAUD: u32 = 500_000;
    /// The COM port the VCX-NANO enumerates as, used when no name is given.
    pub const DEFAULT_PORT: u32 = 4;

    const TIMEOUT: Duration = Duration::from_secs(2);

    /// One open device and its single ISO 15765 channel.
    struct Session {
        port: vcxtoolkit::Port,
        channel: Channel,
        next_filter: u8,
    }

    fn cell() -> &'static Mutex<Option<Session>> {
        static CELL: OnceLock<Mutex<Option<Session>>> = OnceLock::new();
        CELL.get_or_init(|| Mutex::new(None))
    }

    /// The four big-endian bytes at the front of a raw-mode CAN id field, as `u32`.
    fn can_id(bytes: &[u8]) -> Option<u32> {
        let id = bytes.get(..4)?;
        Some(u32::from_be_bytes([id[0], id[1], id[2], id[3]]))
    }

    /// Open the dongle on `port` and store the session.
    pub fn open(port: u32) -> Status {
        match open_usb(port, 0) {
            Ok(opened) => {
                // ISO 15765 always resolves a VCX_PRO_Open dword, so new() cannot fail
                // here; guard it rather than unwrap.
                let Ok(channel) = Channel::new(ISO15765_PROTOCOL, 0, 0) else {
                    return ERR_FAILED;
                };
                let mut guard = cell().lock().expect("session lock");
                *guard = Some(Session {
                    port: opened,
                    channel,
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

    /// Run the recovered ISO 15765 connect bring-up ([`Channel::connect_with`]).
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
        match session
            .channel
            .connect_with(&session.port, baud, &mut Deadline::new(TIMEOUT))
        {
            Ok(()) => STATUS_NOERROR,
            Err(_) => ERR_FAILED,
        }
    }

    /// Install one flow-control filter; returns its J2534 filter id.
    ///
    /// `pattern` is the ECU response id, `flow` the tester request id (each the
    /// 4-byte big-endian CAN id a J2534 `FLOW_CONTROL` filter carries); `mask` is the
    /// match mask the library fixes at all-ones, so it is validated, not forwarded.
    pub fn add_flow_filter(pattern: &[u8], _mask: &[u8], flow: &[u8]) -> Result<u32, Status> {
        let (Some(response_id), Some(request_id)) = (can_id(pattern), can_id(flow)) else {
            return Err(ERR_INVALID_MSG);
        };
        let mut guard = cell().lock().expect("session lock");
        let Some(session) = guard.as_mut() else {
            return Err(ERR_DEVICE_NOT_CONNECTED);
        };
        match session.channel.add_flow_control_filter_with(
            &session.port,
            request_id,
            response_id,
            &mut Deadline::new(TIMEOUT),
        ) {
            Ok(_slot_id) => {
                let id = u32::from(session.next_filter) + 1;
                session.next_filter = session.next_filter.wrapping_add(1);
                Ok(id)
            }
            Err(_) => Err(ERR_FAILED),
        }
    }

    /// Send one ISO 15765 message (the app's `[CAN id][PDU]` raw-mode data).
    pub fn write(data: &[u8]) -> Status {
        let mut guard = cell().lock().expect("session lock");
        let Some(session) = guard.as_mut() else {
            return ERR_DEVICE_NOT_CONNECTED;
        };
        // ISO 15765 copies the data verbatim to the Send PDU; flags 0 is a plain
        // request. The returned frame is the device ack, not the ECU reply.
        match session
            .channel
            .send_raw_with(&session.port, 0, data, &mut Deadline::new(TIMEOUT))
        {
            Ok(_ack) => STATUS_NOERROR,
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
            .channel
            .read_msg_with(&session.port, &mut Deadline::new(TIMEOUT))
        {
            Ok(frame) => reply_application_bytes(&frame.payload)
                .map(<[u8]>::to_vec)
                .map_err(|_| ERR_INVALID_MSG),
            Err(DeviceError::Send(ExchangeError::Timeout)) => Err(ERR_BUFFER_EMPTY),
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
