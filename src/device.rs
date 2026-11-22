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

use crate::forward::{ComRecord, ForwardError, baud_record, flow_control_filter, open_payload};
use crate::{
    Command, ExchangeError, FILTER_PASS, Frame, Patience, SerialPort, add_filter, pro_send_header,
    send_payload, set_com_param, set_filter_payload, start_payload,
};

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

    /// Run one bring-up command (`VCX_PRO_*`) and return its reply frame.
    ///
    /// Writes `command` carrying `payload` on this channel, reads the reply, and
    /// surfaces an explicit [`INVALID_COMMAND`] refusal. Unlike [`Channel::open_with`]
    /// it does not assert a positive echo: the hardware answers a configured
    /// bring-up step (`SetComParam`/`SetFilter`/`AddFilter`/`Start`) with a device
    /// *status* ack, not a header echo of the command (a `Start`/`Send` ack is the
    /// status byte `0x9B`), so only the recovered refusal code is checked.
    fn config_step<S: SerialPort, P: Patience>(
        self,
        port: &S,
        command: Command,
        payload: &[u8],
        patience: &mut P,
    ) -> Result<Frame, DeviceError<S::Error>> {
        let reply = crate::exchange_with(port, command.header(self.channel), payload, patience)
            .map_err(DeviceError::Config)?;
        if reply.header[2] == INVALID_COMMAND {
            return Err(DeviceError::Refused);
        }
        Ok(reply)
    }

    /// `VCX_PRO_SetComParam`: set the channel's communication parameters.
    ///
    /// This is the recovered tail of `VCXPT32!PassThruConnect` (`FUN_10006dc0`,
    /// step 2) and the J2534 `PassThruIoctl(SET_CONFIG)` path: each
    /// [`ComRecord`] is a `(key, value)` pair ([`crate::set_com_param`] frames
    /// them). For an ISO 15765 connect the one record is [`baud_record`] with the
    /// J2534 `DATA_RATE` baud (SAE J2534-2002 §7.3.2, `DATA_RATE = 0x01`, range
    /// 5–500000; HS-GMLAN is 500000). Without it the dongle never configures the
    /// channel and a later `VCX_PRO_Send` only returns a status ack.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError::Forward`] when [`crate::set_com_param`] refuses the
    /// records, [`DeviceError::Config`] on a port failure or timeout, and
    /// [`DeviceError::Refused`] when the device echoes [`INVALID_COMMAND`].
    pub fn set_com_param_with<S: SerialPort, P: Patience>(
        &self,
        port: &S,
        records: &[ComRecord],
        patience: &mut P,
    ) -> Result<Frame, DeviceError<S::Error>> {
        let payload = set_com_param(records).map_err(DeviceError::Forward)?;
        self.config_step(port, Command::ProSetComParam, &payload, patience)
    }

    /// `VCX_PRO_SetFilter`: the one-byte filter mode `PassThruConnect` sends.
    ///
    /// `VCXPT32!PassThruConnect` step 3 sends [`FILTER_PASS`] (`1`). This is the
    /// per-protocol pass filter, distinct from the ISO 15765 flow-control filter a
    /// later [`Channel::add_flow_control_filter_with`] installs.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError::Forward`] when `mode` is not a recovered value,
    /// [`DeviceError::Config`] on a port failure, and [`DeviceError::Refused`] on
    /// an [`INVALID_COMMAND`] echo.
    pub fn set_filter_with<S: SerialPort, P: Patience>(
        &self,
        port: &S,
        mode: u8,
        patience: &mut P,
    ) -> Result<Frame, DeviceError<S::Error>> {
        let payload = set_filter_payload(mode).map_err(DeviceError::Forward)?;
        self.config_step(port, Command::ProSetFilter, &payload, patience)
    }

    /// `VCX_PRO_Start`: start the configured channel (step 4 of the connect).
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError::Config`] on a port failure or timeout and
    /// [`DeviceError::Refused`] on an [`INVALID_COMMAND`] echo.
    pub fn start_with<S: SerialPort, P: Patience>(
        &self,
        port: &S,
        patience: &mut P,
    ) -> Result<Frame, DeviceError<S::Error>> {
        self.config_step(port, Command::ProStart, &start_payload(), patience)
    }

    /// `VCX_PRO_AddFilter`: install the ISO 15765 flow-control filter so the dongle
    /// delivers the ECU's reply.
    ///
    /// This is the J2534 `PassThruStartMsgFilter(FLOW_CONTROL_FILTER)` mapping: the
    /// filter's pattern is the ECU `response_id`, its mask is all ones, and its
    /// flow-control bytes are the tester `request_id`
    /// ([`crate::flow_control_filter`]; SAE J2534-2002 §7.2.1.1 Figure 11,
    /// §9.2.2.1). It returns the filter id the dongle assigns (slot + 1). Without
    /// the filter a `VCX_PRO_Send` returns only a device status ack.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError::Forward`] when the filter record cannot be built,
    /// [`DeviceError::Config`] on a port failure, and [`DeviceError::Refused`] on
    /// an [`INVALID_COMMAND`] echo.
    pub fn add_flow_control_filter_with<S: SerialPort, P: Patience>(
        &self,
        port: &S,
        request_id: u32,
        response_id: u32,
        patience: &mut P,
    ) -> Result<u32, DeviceError<S::Error>> {
        let record =
            flow_control_filter(0, request_id, response_id).map_err(DeviceError::Forward)?;
        let payload = add_filter(&[record]).map_err(DeviceError::Forward)?;
        self.config_step(port, Command::ProAddFilter, &payload, patience)?;
        // The recovered AddFilter returns the stored slot plus one (forward::start_filter).
        Ok(1)
    }

    /// Run the full recovered ISO 15765 channel bring-up before any vehicle send.
    ///
    /// This is `VCXPT32!PassThruConnect` (`FUN_100035e0` → `FUN_10006dc0`) in
    /// order, over the [`SerialPort`] seam:
    ///
    /// 1. `VCX_PRO_Open` with the recovered dword ([`Channel::open_with`];
    ///    ISO 15765 = `0x8001`).
    /// 2. `VCX_PRO_SetComParam` with the CAN `baud` ([`Channel::set_com_param_with`]).
    /// 3. `VCX_PRO_SetFilter(FILTER_PASS)` ([`Channel::set_filter_with`]).
    /// 4. `VCX_PRO_Start` ([`Channel::start_with`]).
    ///
    /// The flow-control filter is a *separate* step
    /// ([`Channel::add_flow_control_filter_with`]), mirroring the J2534 split
    /// between `PassThruConnect` and `PassThruStartMsgFilter`. This sequence is
    /// hardware-validated on a VCX-NANO (fw 1.9.4.2): the dongle accepts every
    /// frame and the channel opens (the open reply echoes `0x40`). It transmits
    /// nothing to a vehicle; it configures the dongle's channel.
    ///
    /// # Errors
    ///
    /// The errors of [`Channel::open_with`], [`Channel::set_com_param_with`],
    /// [`Channel::set_filter_with`], and [`Channel::start_with`].
    pub fn connect_with<S: SerialPort, P: Patience>(
        &self,
        port: &S,
        baud: u32,
        patience: &mut P,
    ) -> Result<(), DeviceError<S::Error>> {
        self.open_with(port, patience)?;
        self.set_com_param_with(port, &[baud_record(baud)], patience)?;
        self.set_filter_with(port, FILTER_PASS, patience)?;
        self.start_with(port, patience)?;
        Ok(())
    }

    /// `VCX_PRO_Send`: write one already-framed request and read the device reply.
    ///
    /// `framed_request` is the `VCX_PRO_Send` payload — the flags word, length, and
    /// the raw-mode data bytes `[CAN id BE u32][application bytes]`, which
    /// [`crate::send_payload`] builds and [`crate::collect::frame_identity_request`]
    /// produces for a UDS read. This module synthesizes no request content.
    ///
    /// The returned frame is the device's **status ack** for the send, not the
    /// vehicle reply. On a VCX-NANO the ack payload is a single byte (`0x9B`); the
    /// ECU reply is a *separately pushed* serial frame read by
    /// [`Channel::read_msg_with`]. The command that fetches it is in the dongle's
    /// VMProtect-virtualised export and is not recovered, so this does not model
    /// the ack as the reply envelope.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError::Send`] when the `VCX_PRO_Send` round-trip fails.
    pub fn write_msg_with<S: SerialPort, P: Patience>(
        &self,
        port: &S,
        framed_request: &[u8],
        patience: &mut P,
    ) -> Result<Frame, DeviceError<S::Error>> {
        crate::exchange_with(
            port,
            pro_send_header(self.caller, self.channel),
            framed_request,
            patience,
        )
        .map_err(DeviceError::Send)
    }

    /// Build a `VCX_PRO_Send` payload from raw-mode data and send it.
    ///
    /// A convenience over [`Channel::write_msg_with`]: `data` is the raw-mode field
    /// `[CAN id BE u32][application bytes]`, which [`crate::send_payload`] wraps with
    /// the flags word and length. `flags` is the already-mapped PDU flag word
    /// ([`crate::pdu_flags`]). The bytes are the caller's; nothing is synthesized.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError::Forward`] when [`crate::send_payload`] refuses the
    /// data (over the `0x1020` cap) and [`DeviceError::Send`] on a port failure.
    pub fn send_raw_with<S: SerialPort, P: Patience>(
        &self,
        port: &S,
        flags: u32,
        data: &[u8],
        patience: &mut P,
    ) -> Result<Frame, DeviceError<S::Error>> {
        let framed = send_payload(flags, data).map_err(DeviceError::Forward)?;
        self.write_msg_with(port, &framed, patience)
    }

    /// Read one pushed reply frame from the dongle (the ECU reply after a send).
    ///
    /// The VCX dongle *pushes* an ECU reply as its own serial frame; the firmware
    /// has no receive command (the recovered command set has no `Recv`), so the
    /// reply is read, not polled ([`crate::read_message`]). The returned frame's
    /// payload is the recovered `VCX_PRO_Recv` envelope that
    /// [`crate::collect::reply_application_bytes`] consumes. Whether a poll byte is
    /// written to trigger the push is unconfirmed; this reads without writing.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError::Send`] when the read fails or no frame arrives before
    /// `patience` gives up.
    pub fn read_msg_with<S: SerialPort, P: Patience>(
        &self,
        port: &S,
        patience: &mut P,
    ) -> Result<Frame, DeviceError<S::Error>> {
        crate::read_message(port, patience).map_err(DeviceError::Send)
    }
}

/// Why a [`Channel`] exchange did not return a reply.
#[derive(Debug)]
pub enum DeviceError<E> {
    /// The protocol has no `VCX_PRO_Open` dword.
    Protocol(ForwardError),
    /// The `VCX_PRO_Open` round-trip failed on the port.
    Open(ExchangeError<E>),
    /// A channel bring-up round-trip (`SetComParam`/`SetFilter`/`AddFilter`/`Start`)
    /// failed on the port.
    Config(ExchangeError<E>),
    /// The `VCX_PRO_Send`/`VCX_PRO_Recv` round-trip failed on the port.
    Send(ExchangeError<E>),
    /// A `forward` builder refused the records a bring-up or send step carries.
    Forward(ForwardError),
    /// The open reply did not echo the `VCX_PRO_Open` command byte.
    WrongEcho {
        /// The command byte that was sent (`0x40`).
        expected: u8,
        /// The command byte the reply carried.
        got: u8,
    },
    /// The device refused a command with `VCX_ERR_INVALID_CMD` ([`INVALID_COMMAND`])
    /// — the open, or a bring-up step.
    Refused,
}

impl<E: std::fmt::Display> std::fmt::Display for DeviceError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Protocol(error) => write!(f, "protocol: {error}"),
            Self::Open(error) => write!(f, "open: {error}"),
            Self::Config(error) => write!(f, "config: {error}"),
            Self::Send(error) => write!(f, "send: {error}"),
            Self::Forward(error) => write!(f, "forward: {error}"),
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
            Self::Protocol(error) | Self::Forward(error) => Some(error),
            Self::Open(error) | Self::Config(error) | Self::Send(error) => Some(error),
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
            Err(DeviceError::Forward(_)) => "forward",
            Err(DeviceError::Config(ExchangeError::Timeout)) => "config-timeout",
            Err(DeviceError::Config(_)) => "config-other",
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

    // A bring-up step status ack: a frame whose header byte 2 is not the refusal
    // code, carrying the device status byte 0x9B the hardware returns for Start/Send.
    fn status_ack() -> Vec<u8> {
        pack([0x80, 0x00, 0x00, CHANNEL], &[0x9B]).unwrap()
    }

    #[test]
    fn connect_writes_the_recovered_open_setcomparam_setfilter_start_frames() {
        use crate::{Command, baud_record, pack, set_com_param, set_filter_payload, start_payload};

        // open ack, then a status ack for each of SetComParam, SetFilter, Start.
        let fake = FakeSerial::new(vec![
            Chunk::Bytes(open_ack()),
            Chunk::Bytes(status_ack()),
            Chunk::Bytes(status_ack()),
            Chunk::Bytes(status_ack()),
        ]);
        channel()
            .connect_with(&fake, 500_000, &mut Tries(0))
            .expect("bring-up");

        let writes = fake.writes.borrow();
        assert_eq!(writes.len(), 4, "open + setcomparam + setfilter + start");
        // 1. VCX_PRO_Open with the ISO 15765 dword 0x8001.
        assert_eq!(
            writes[0],
            pack(
                Command::ProOpen.header(CHANNEL),
                &crate::open_payload(ISO15765_PROTOCOL).unwrap()
            )
            .unwrap()
        );
        // 2. VCX_PRO_SetComParam {key=3, value=500000} -> 00 03 00 07 A1 20.
        assert_eq!(
            writes[1],
            pack(
                Command::ProSetComParam.header(CHANNEL),
                &set_com_param(&[baud_record(500_000)]).unwrap()
            )
            .unwrap()
        );
        assert_eq!(
            writes[1],
            pack(
                [0x80, 0x00, 0x45, CHANNEL],
                &[0x00, 0x03, 0x00, 0x07, 0xA1, 0x20]
            )
            .unwrap()
        );
        // 3. VCX_PRO_SetFilter(FILTER_PASS = 1).
        assert_eq!(
            writes[2],
            pack(
                Command::ProSetFilter.header(CHANNEL),
                &set_filter_payload(crate::FILTER_PASS).unwrap()
            )
            .unwrap()
        );
        // 4. VCX_PRO_Start(0).
        assert_eq!(
            writes[3],
            pack(Command::ProStart.header(CHANNEL), &start_payload()).unwrap()
        );
    }

    #[test]
    fn a_flow_control_filter_write_carries_the_recovered_addfilter_frame() {
        use crate::{Command, add_filter, flow_control_filter, pack};

        let fake = FakeSerial::new(vec![Chunk::Bytes(status_ack())]);
        let id = channel()
            .add_flow_control_filter_with(&fake, 0x7E0, 0x7E8, &mut Tries(0))
            .expect("flow-control filter");
        assert_eq!(id, 1, "the dongle returns slot 0 + 1");

        let writes = fake.writes.borrow();
        assert_eq!(writes.len(), 1);
        assert_eq!(
            writes[0],
            pack(
                Command::ProAddFilter.header(CHANNEL),
                &add_filter(&[flow_control_filter(0, 0x7E0, 0x7E8).unwrap()]).unwrap()
            )
            .unwrap()
        );
        // Pinned in full: count, record head, pattern(7E8), mask, flow(7E0).
        assert_eq!(
            writes[0],
            pack(
                [0x80, 0x00, 0x48, CHANNEL],
                &[
                    0x01, 0x00, 0x02, 0x02, 0x04, 0x00, 0x00, 0x07, 0xE8, 0xFF, 0xFF, 0xFF, 0xFF,
                    0x00, 0x00, 0x07, 0xE0
                ]
            )
            .unwrap()
        );
    }

    #[test]
    fn write_msg_sends_the_framed_request_and_returns_the_status_ack() {
        // write_msg writes VCX_PRO_Send with the caller's framed request and returns
        // the device status ack (0x9B) — NOT the vehicle reply, which is pushed later.
        let framed = send_raw_identity();
        let fake = FakeSerial::new(vec![Chunk::Bytes(status_ack())]);
        let ack = channel()
            .write_msg_with(&fake, &framed, &mut Tries(0))
            .expect("send");
        assert_eq!(ack.payload, [0x9B]);

        let writes = fake.writes.borrow();
        assert_eq!(writes.len(), 1);
        assert_eq!(
            writes[0],
            pack(pro_send_header(CALLER, CHANNEL), &framed).unwrap()
        );
    }

    #[test]
    fn read_msg_reads_a_pushed_reply_without_writing() {
        use crate::collect::reply_application_bytes;

        // The ECU reply is a separately pushed frame; read_msg reads it with no write.
        let app = [crate::collect::IDENTITY_POSITIVE_SID, 0xC1, 0x31, 0x32];
        let mut pdu = Vec::new();
        pdu.extend_from_slice(&0_u32.to_le_bytes());
        pdu.extend_from_slice(&u16::try_from(app.len()).unwrap().to_le_bytes());
        pdu.extend_from_slice(&app);
        let pushed = crate::pack([0x80, 0x00, 0x00, CHANNEL], &pdu).unwrap();

        let fake = FakeSerial::new(vec![Chunk::Bytes(pushed)]);
        let reply = channel()
            .read_msg_with(&fake, &mut Tries(0))
            .expect("pushed reply");
        assert_eq!(reply_application_bytes(&reply.payload).unwrap(), &app);
        // Nothing was written to trigger the push.
        assert!(fake.writes.borrow().is_empty());
    }

    #[test]
    fn a_refused_bring_up_step_is_surfaced() {
        // The dongle echoes VCX_ERR_INVALID_CMD (0x91) to the SetComParam after the
        // open: connect_with stops with Refused.
        let fake = FakeSerial::new(vec![
            Chunk::Bytes(open_ack()),
            Chunk::Bytes(echo_frame(INVALID_COMMAND)),
        ]);
        let error = channel()
            .connect_with(&fake, 500_000, &mut Tries(0))
            .unwrap_err();
        assert!(matches!(error, DeviceError::Refused), "{error:?}");
        // The open and the refused SetComParam were written; no SetFilter followed.
        assert_eq!(fake.writes.borrow().len(), 2);
    }

    // A framed ISO 15765 request fixture: the raw-mode data `00 00 07 E0 1A C1`
    // (id 7E0 then the UDS bytes) wrapped by send_payload. The UDS bytes are an
    // EXAMPLE fixture; this module only frames them.
    fn send_raw_identity() -> Vec<u8> {
        crate::send_payload(0, &[0x00, 0x00, 0x07, 0xE0, 0x1A, 0xC1]).unwrap()
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
