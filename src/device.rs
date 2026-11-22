//! The channel-open and Send/Recv exchange over the serial port.
//!
//! `doctor` already opens the USB port and runs one [`crate::exchange`] for
//! `GetInfo` ([`crate::cli`]). A vehicle read is the same port with more steps:
//! open an ISO 15765 data channel (`VCX_PRO_Open`, the recovered `0x8001` dword
//! from [`crate::open_code`]), write the caller's already-framed request, and read
//! the reply. Each step is one [`crate::exchange_with`] on the [`crate::SerialPort`]
//! seam, so an in-process fake drives the whole sequence over canned bytes in
//! tests, exactly as `doctor`'s single `GetInfo` round-trip is structured.
//!
//! This module is pure transport. It wraps no `$1A` content and transmits nothing
//! of its own: the caller supplies the framed request
//! ([`crate::collect::frame_identity_request`], whose bytes are not recovered from
//! the VXDIAG binaries), and the reply frame's payload is handed back for the
//! recovered envelope parser ([`crate::collect::reply_application_bytes`]). The
//! live vehicle send stays held behind [`crate::collect::vehicle_read_gate`]; the
//! only real-hardware path here is an `#[ignore]`d test that opens the channel
//! against the dongle and sends no request.

use std::time::Duration;

use crate::forward::{ForwardError, open_payload};
use crate::{Command, ExchangeError, Frame, Patience, SerialPort, pro_send_header};

/// `VCX_ERR_INVALID_CMD`, the command byte a refused reply echoes.
///
/// `src/command.rs` records that the DLL returns `0x91` for an invalid command.
/// A `VCX_PRO_Open` reply that echoes it in header byte 2 is the device refusing
/// the open (for example an already-open channel), reported as
/// [`DeviceError::Refused`].
pub const INVALID_COMMAND: u8 = 0x91;

/// A VCX data channel: the protocol to open and the header bytes each frame carries.
///
/// The reply to a command echoes the command byte in header byte 2 (the recovered
/// `GetInfo`, `GetTime`, and `GetPinVol` round-trips all do), so [`Channel::open`]
/// checks the open reply echoes `VCX_PRO_Open` (`0x40`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Channel {
    /// The pass-through protocol selector, e.g. [`crate::collect::ISO15765_PROTOCOL`].
    pub protocol: u32,
    /// Header byte 3 (the caller's channel) on `VCX_PRO_Open` and `VCX_PRO_Send`.
    pub channel: u8,
    /// `VCX_PRO_Send` header byte 1 (the caller byte). `VCX_PRO_Open` uses `0`.
    pub caller: u8,
}

impl Channel {
    /// A channel over `protocol`, framed for `channel` and `caller`.
    ///
    /// # Errors
    ///
    /// Returns [`ForwardError::UnknownProtocol`] when `protocol` has no
    /// `VCX_PRO_Open` dword ([`crate::open_code`]).
    pub fn new(protocol: u32, channel: u8, caller: u8) -> Result<Self, ForwardError> {
        // Validate the open dword resolves now, so the exchange cannot fail on it.
        open_payload(protocol)?;
        Ok(Self {
            protocol,
            channel,
            caller,
        })
    }

    /// Open the data channel against the dongle and return its reply frame.
    ///
    /// This writes `VCX_PRO_Open` with the recovered dword and reads the reply,
    /// pacing empty reads with a [`crate::Deadline`] built from `timeout`. It sends
    /// nothing to a vehicle: `VCX_PRO_Open` configures the channel on the dongle.
    ///
    /// # Errors
    ///
    /// See [`Channel::open_with`].
    pub fn open<S: SerialPort>(
        &self,
        port: &S,
        timeout: Duration,
    ) -> Result<Frame, DeviceError<S::Error>> {
        let mut patience = crate::Deadline::new(timeout);
        self.open_with(port, &mut patience)
    }

    /// Open the data channel, pacing empty reads with `patience`.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError::Protocol`] when the open dword does not resolve,
    /// [`DeviceError::Open`] on a port failure or timeout, [`DeviceError::Refused`]
    /// when the reply echoes [`INVALID_COMMAND`], and [`DeviceError::WrongEcho`]
    /// when it echoes neither that nor `VCX_PRO_Open` (`0x40`).
    pub fn open_with<S: SerialPort, P: Patience>(
        &self,
        port: &S,
        patience: &mut P,
    ) -> Result<Frame, DeviceError<S::Error>> {
        let open = open_payload(self.protocol).map_err(DeviceError::Protocol)?;
        let reply =
            crate::exchange_with(port, Command::ProOpen.header(self.channel), &open, patience)
                .map_err(DeviceError::Open)?;
        let got = reply.header[2];
        if got == INVALID_COMMAND {
            return Err(DeviceError::Refused);
        }
        let expected = Command::ProOpen.code();
        if got != expected {
            return Err(DeviceError::WrongEcho { expected, got });
        }
        Ok(reply)
    }

    /// Open the channel, send `request`, and return the reply frame.
    ///
    /// `request` is the already-framed `VCX_PRO_Send` payload — the bytes
    /// [`crate::collect::frame_identity_request`] builds from a caller's UDS
    /// request. This module does not synthesize that content. The returned frame's
    /// payload is the reply PDU for [`crate::collect::reply_application_bytes`].
    ///
    /// # Errors
    ///
    /// See [`Channel::exchange_with`].
    pub fn exchange<S: SerialPort>(
        &self,
        port: &S,
        request: &[u8],
        timeout: Duration,
    ) -> Result<Frame, DeviceError<S::Error>> {
        let mut open = crate::Deadline::new(timeout);
        let mut send = crate::Deadline::new(timeout);
        self.exchange_with(port, request, &mut open, &mut send)
    }

    /// Open the channel and send `request`, pacing each step with its own patience.
    ///
    /// The open is [`Channel::open_with`]; the send writes `VCX_PRO_Send`
    /// (`80 caller 00 channel`) carrying `request` and reads the reply frame.
    ///
    /// # Errors
    ///
    /// The errors of [`Channel::open_with`], and [`DeviceError::Send`] when the
    /// send round-trip fails on the port.
    pub fn exchange_with<S: SerialPort, P: Patience>(
        &self,
        port: &S,
        request: &[u8],
        open_patience: &mut P,
        send_patience: &mut P,
    ) -> Result<Frame, DeviceError<S::Error>> {
        self.open_with(port, open_patience)?;
        crate::exchange_with(
            port,
            pro_send_header(self.caller, self.channel),
            request,
            send_patience,
        )
        .map_err(DeviceError::Send)
    }
}

/// Why a [`Channel`] exchange did not return a reply.
#[derive(Debug)]
pub enum DeviceError<E> {
    /// The protocol has no `VCX_PRO_Open` dword.
    Protocol(ForwardError),
    /// The `VCX_PRO_Open` round-trip failed on the port.
    Open(ExchangeError<E>),
    /// The `VCX_PRO_Send`/`VCX_PRO_Recv` round-trip failed on the port.
    Send(ExchangeError<E>),
    /// The open reply did not echo the `VCX_PRO_Open` command byte.
    WrongEcho {
        /// The command byte that was sent (`0x40`).
        expected: u8,
        /// The command byte the reply carried.
        got: u8,
    },
    /// The device refused the open with `VCX_ERR_INVALID_CMD` ([`INVALID_COMMAND`]).
    Refused,
}

impl<E: std::fmt::Display> std::fmt::Display for DeviceError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Protocol(error) => write!(f, "protocol: {error}"),
            Self::Open(error) => write!(f, "open: {error}"),
            Self::Send(error) => write!(f, "send: {error}"),
            Self::WrongEcho { expected, got } => {
                write!(f, "open reply echoed {got:#04x}, not {expected:#04x}")
            }
            Self::Refused => f.write_str("device refused the open with VCX_ERR_INVALID_CMD"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for DeviceError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Protocol(error) => Some(error),
            Self::Open(error) | Self::Send(error) => Some(error),
            Self::WrongEcho { .. } | Self::Refused => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::{Channel, DeviceError, INVALID_COMMAND};
    use crate::collect::{
        IDENTITY_POSITIVE_SID, ISO15765_PROTOCOL, ReplyError, frame_identity_request,
        read_identity_reply, reply_application_bytes,
    };
    use crate::{
        Command, ExchangeError, ForwardError, Patience, SerialPort, pack, pro_send_header,
    };

    const CHANNEL: u8 = 0x11;
    const CALLER: u8 = 0x12;

    // One queued read from the fake: either bytes to deliver, or a port failure.
    enum Chunk {
        Bytes(Vec<u8>),
        Fail,
    }

    #[derive(Debug)]
    struct FakeError;

    impl std::fmt::Display for FakeError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("fake serial failure")
        }
    }

    impl std::error::Error for FakeError {}

    // An in-process serial port. Writes are recorded; each read pops the next
    // queued chunk. An empty queue reads zero, so the pacing seam gives up.
    struct FakeSerial {
        writes: RefCell<Vec<Vec<u8>>>,
        reads: RefCell<VecDeque<Chunk>>,
    }

    impl FakeSerial {
        fn new(chunks: Vec<Chunk>) -> Self {
            Self {
                writes: RefCell::new(Vec::new()),
                reads: RefCell::new(chunks.into_iter().collect()),
            }
        }
    }

    impl SerialPort for FakeSerial {
        type Error = FakeError;

        fn write(&self, buf: &[u8]) -> Result<usize, FakeError> {
            self.writes.borrow_mut().push(buf.to_vec());
            Ok(buf.len())
        }

        fn read(&self, buf: &mut [u8]) -> Result<usize, FakeError> {
            let mut reads = self.reads.borrow_mut();
            match reads.front_mut() {
                None => Ok(0),
                Some(Chunk::Fail) => {
                    reads.pop_front();
                    Err(FakeError)
                }
                Some(Chunk::Bytes(bytes)) => {
                    let n = buf.len().min(bytes.len());
                    buf[..n].copy_from_slice(&bytes[..n]);
                    if n == bytes.len() {
                        reads.pop_front();
                    } else {
                        bytes.drain(..n);
                    }
                    Ok(n)
                }
            }
        }
    }

    // A deterministic Patience: give up after `0` extra tries, never sleeping.
    struct Tries(u32);

    impl Patience for Tries {
        fn keep_waiting(&mut self) -> bool {
            if self.0 == 0 {
                return false;
            }
            self.0 -= 1;
            true
        }
    }

    // A valid open reply: a frame echoing VCX_PRO_Open (0x40) in header byte 2.
    fn open_ack() -> Vec<u8> {
        pack([0x80, 0x00, 0x40, CHANNEL], &[]).unwrap()
    }

    // A frame echoing `command` in header byte 2 (the wrong echo / refusal cases).
    fn echo_frame(command: u8) -> Vec<u8> {
        pack([0x80, 0x00, command, CHANNEL], &[]).unwrap()
    }

    // A send reply frame whose payload is the recovered Recv PDU envelope:
    // flags u32 LE, len u16 LE, then the application bytes (collect::reply_pdu).
    fn send_reply(app: &[u8]) -> Vec<u8> {
        let mut pdu = Vec::new();
        pdu.extend_from_slice(&0_u32.to_le_bytes());
        pdu.extend_from_slice(&u16::try_from(app.len()).unwrap().to_le_bytes());
        pdu.extend_from_slice(app);
        pack([0x80, 0x00, 0x00, CHANNEL], &pdu).unwrap()
    }

    fn channel() -> Channel {
        Channel::new(ISO15765_PROTOCOL, CHANNEL, CALLER).unwrap()
    }

    // The UDS bytes here are an EXAMPLE FIXTURE, not recovered from the binary:
    // a classical single frame `02 1A C1`. The device exchange only wraps them.
    fn identity_request() -> Vec<u8> {
        frame_identity_request(&[0x02, 0x1A, 0xC1]).unwrap()
    }

    fn run(chunks: Vec<Chunk>) -> Result<crate::Frame, DeviceError<FakeError>> {
        let fake = FakeSerial::new(chunks);
        let request = identity_request();
        channel().exchange_with(&fake, &request, &mut Tries(0), &mut Tries(0))
    }

    // A one-word label for each outcome, so the error paths can be table-driven.
    fn label(result: &Result<crate::Frame, DeviceError<FakeError>>) -> &'static str {
        match result {
            Ok(_) => "ok",
            Err(DeviceError::WrongEcho { .. }) => "wrong-echo",
            Err(DeviceError::Refused) => "refused",
            Err(DeviceError::Protocol(_)) => "protocol",
            Err(DeviceError::Open(ExchangeError::Timeout)) => "open-timeout",
            Err(DeviceError::Open(ExchangeError::Read(_))) => "open-closed",
            Err(DeviceError::Open(_)) => "open-other",
            Err(DeviceError::Send(ExchangeError::Timeout)) => "send-timeout",
            Err(DeviceError::Send(ExchangeError::Read(_))) => "send-closed",
            Err(DeviceError::Send(_)) => "send-other",
        }
    }

    #[test]
    fn a_correct_sequence_opens_sends_and_yields_the_application_bytes() {
        // open ack, then a positive $1A C1 reply carrying two data bytes.
        let app = [IDENTITY_POSITIVE_SID, 0xC1, 0x31, 0x32];
        let fake = FakeSerial::new(vec![
            Chunk::Bytes(open_ack()),
            Chunk::Bytes(send_reply(&app)),
        ]);
        let request = identity_request();
        let reply = channel()
            .exchange_with(&fake, &request, &mut Tries(0), &mut Tries(0))
            .expect("exchange");

        // The recovered Recv envelope yields the controller's application bytes,
        // and the $1A reader returns the data after the echoed identifier.
        assert_eq!(reply_application_bytes(&reply.payload).unwrap(), &app);
        assert_eq!(
            read_identity_reply(&reply.payload, 0xC1),
            Ok(vec![0x31, 0x32])
        );

        // Exactly two frames were written: the open, then the caller's request.
        let writes = fake.writes.borrow();
        assert_eq!(writes.len(), 2);
        let open = crate::open_payload(ISO15765_PROTOCOL).unwrap();
        assert_eq!(
            writes[0],
            pack(Command::ProOpen.header(CHANNEL), &open).unwrap()
        );
        assert_eq!(
            writes[1],
            pack(pro_send_header(CALLER, CHANNEL), &request).unwrap()
        );
    }

    #[test]
    fn a_negative_reply_is_surfaced_through_the_recovered_envelope() {
        // 0x7F, service $1A, code 0x31 (requestOutOfRange).
        let fake = FakeSerial::new(vec![
            Chunk::Bytes(open_ack()),
            Chunk::Bytes(send_reply(&[0x7F, 0x1A, 0x31])),
        ]);
        let request = identity_request();
        let reply = channel()
            .exchange_with(&fake, &request, &mut Tries(0), &mut Tries(0))
            .expect("exchange");
        assert_eq!(
            read_identity_reply(&reply.payload, 0xC1),
            Err(ReplyError::Negative {
                service: 0x1A,
                code: 0x31,
            }),
        );
    }

    #[test]
    fn the_error_paths_are_each_handled() {
        // (label, the queued reads the fake serves)
        let cases: Vec<(&str, Vec<Chunk>)> = vec![
            // The open reply echoes GetInfo (0x8C), not VCX_PRO_Open (0x40).
            (
                "wrong-echo",
                vec![Chunk::Bytes(echo_frame(Command::GetInfo.code()))],
            ),
            // The device refuses the open with VCX_ERR_INVALID_CMD (e.g. already open).
            ("refused", vec![Chunk::Bytes(echo_frame(INVALID_COMMAND))]),
            // A short/garbled open reply never closes its frame, then the port dries up.
            ("open-timeout", vec![Chunk::Bytes(vec![0xBB, 0x80, 0x00])]),
            // The port is closed: the first read fails.
            ("open-closed", vec![Chunk::Fail]),
            // The open succeeds, then the send round-trip's read fails.
            ("send-closed", vec![Chunk::Bytes(open_ack()), Chunk::Fail]),
            // The open succeeds, then the reply never arrives.
            ("send-timeout", vec![Chunk::Bytes(open_ack())]),
        ];
        for (expected, chunks) in cases {
            assert_eq!(label(&run(chunks)), expected, "case {expected}");
        }
    }

    #[test]
    fn an_unknown_protocol_has_no_channel() {
        assert_eq!(
            Channel::new(0xDEAD, 0, 0),
            Err(ForwardError::UnknownProtocol)
        );
    }

    // The channel-open against real hardware. It opens the ISO 15765 channel on
    // the DONGLE and reads its echo; it does NOT call exchange(), which would
    // transmit the caller's request. The live vehicle read stays held behind the
    // owner gate, asserted closed here.
    #[cfg(windows)]
    #[test]
    #[ignore = "needs the CH343 dongle on COM4; opens the dongle channel, sends no vehicle request"]
    fn com4_opens_the_iso15765_channel_without_sending() {
        // With the shipped layout flag and even an owner opt-in, the gate refuses:
        // this crate never performs the live vehicle send.
        assert!(
            crate::collect::vehicle_read_gate(crate::collect::READ_LAYOUT_RECOVERED, true).is_err()
        );
        let port = crate::open_usb(4, 0).expect("COM4");
        let reply = channel()
            .open(&port, std::time::Duration::from_secs(2))
            .expect("open ISO 15765 channel");
        assert_eq!(reply.header[2], Command::ProOpen.code());
    }
}
