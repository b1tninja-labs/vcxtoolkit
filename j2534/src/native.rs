//! The native VCX driver behind the J2534 exports: the recovered connect
//! sequence, the flow-control filter, the send, and the separate reply read,
//! driven over the [`vcxtoolkit::SerialPort`] seam.
//!
//! Everything here is generic over the serial port, so an in-process fake drives
//! the whole sequence in tests, exactly as `vcxtoolkit`'s own `device`/`exchange`
//! modules are structured. The FFI layer ([`crate::ffi`]) owns the real Windows
//! [`vcxtoolkit::Port`] and calls these methods; nothing here is `unsafe`.
//!
//! What is recovered and hardware-accepted (the GM diagnostic bring-up, confirmed
//! accepted on the real VCX-NANO on COM4): `VCX_PRO_Open(0x8001)` →
//! `VCX_PRO_SetComParam{key 3, baud}` → `VCX_PRO_AddFilter` (flow control) →
//! `VCX_PRO_Start`, each answered by a one-byte ack frame whose header is
//! `80 00 <command> 00` (so the VCX channel and caller bytes are `0x00`), then
//! `VCX_PRO_Send` answered by a one-byte ack. The ECU reply then arrives as a
//! **separate** frame carrying the recovered Recv envelope
//! ([`vcxtoolkit::collect::reply_application_bytes`]).
//!
//! The one piece not statically confirmed this pass is the read *trigger*: whether
//! the dongle pushes that reply frame unsolicited (the model here — read the next
//! queued frame) or expects a `VCX_PRO_Recv` request whose bytes are not recovered.
//! This driver therefore **reads** a queued frame and never sends a guessed Recv
//! request. If the dongle turns out to need one, [`Link::read`] is the single
//! function to revisit once the request is recovered — no invented bytes ship.

use std::time::Duration;

use vcxtoolkit::collect::{ISO15765_PROTOCOL, ReplyError, reply_application_bytes};
use vcxtoolkit::{
    Command, ExchangeError, FilterCopy, ForwardError, Frame, Patience, SerialPort, add_filter,
    baud_record, copied_data, exchange_with, open_payload, pro_send_header, send_payload,
    set_com_param, set_filter_payload, start_payload,
};
use vcxtoolkit::{Deadline, FILTER_PASS};

/// The VCX frame marker. A frame runs from one `0xBB` through the next
/// (`vcxtoolkit`'s `frame` module; the marker is not public, so it is named here).
const MARKER: u8 = 0xBB;

/// Default wall-clock timeout for one command round-trip.
pub const STEP_TIMEOUT: Duration = Duration::from_secs(2);

/// Why a native driver step did not complete.
#[derive(Debug)]
pub enum LinkError<E> {
    /// A payload could not be built (e.g. the protocol has no open dword, or the
    /// filter copy is malformed).
    Build(ForwardError),
    /// A command round-trip failed on the port. The `u8` is the command byte.
    Exchange(u8, ExchangeError<E>),
    /// The `VCX_PRO_Send` round-trip failed on the port.
    Send(ExchangeError<E>),
    /// A command reply did not echo the command byte in header byte 2.
    WrongEcho {
        /// The command byte that was sent.
        command: u8,
        /// The header byte 2 the reply carried.
        got: u8,
    },
    /// The device answered `VCX_ERR_INVALID_CMD` (`0x91`) to a command.
    Refused(u8),
    /// Reading the reply frame failed on the port.
    Read(ExchangeError<E>),
    /// The reply frame did not carry a well-formed recovered Recv envelope.
    BadReply(ReplyError),
}

impl<E: std::fmt::Display> std::fmt::Display for LinkError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Build(error) => write!(f, "build: {error}"),
            Self::Exchange(command, error) => write!(f, "command {command:#04x} exchange: {error}"),
            Self::Send(error) => write!(f, "send: {error}"),
            Self::WrongEcho { command, got } => {
                write!(f, "reply echoed {got:#04x}, not command {command:#04x}")
            }
            Self::Refused(command) => write!(f, "device refused command {command:#04x}"),
            Self::Read(error) => write!(f, "read: {error}"),
            Self::BadReply(error) => write!(f, "reply envelope: {error}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for LinkError<E> {}

/// `VCX_ERR_INVALID_CMD`, echoed in header byte 2 when a command is refused.
const INVALID_COMMAND: u8 = 0x91;

/// A VCX ISO 15765 link: the recovered connect/send/read sequence over the seam.
///
/// The channel and caller header bytes are `0x00`, the values the live ack frames
/// (`80 00 <command> 00`) show the GM J2534 pass-through uses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Link {
    /// The pass-through protocol selector. ISO 15765 (`6`) opens as `0x8001`.
    pub protocol: u32,
    /// Header byte 3 on every command. `0x00`, per the live ack headers.
    pub channel: u8,
    /// Header byte 1 on `VCX_PRO_Send`. `0x00`, per the live ack headers.
    pub caller: u8,
}

impl Default for Link {
    fn default() -> Self {
        Self {
            protocol: ISO15765_PROTOCOL,
            channel: 0,
            caller: 0,
        }
    }
}

impl Link {
    /// One command round-trip that must echo `command` in the reply's header byte 2.
    fn step<S: SerialPort, P: Patience>(
        self,
        port: &S,
        command: Command,
        payload: &[u8],
        patience: &mut P,
    ) -> Result<Frame, LinkError<S::Error>> {
        let reply = exchange_with(port, command.header(self.channel), payload, patience)
            .map_err(|error| LinkError::Exchange(command.code(), error))?;
        let got = reply.header[2];
        if got == INVALID_COMMAND {
            return Err(LinkError::Refused(command.code()));
        }
        if got != command.code() {
            return Err(LinkError::WrongEcho {
                command: command.code(),
                got,
            });
        }
        Ok(reply)
    }

    /// Run the recovered connect bring-up: open, set the CAN baud, pass filters, start.
    ///
    /// This is the GM diagnostic channel bring-up accepted on the real dongle. Each
    /// step's ack frame is validated to echo its command byte. Nothing is sent to a
    /// vehicle: these commands configure the channel on the dongle.
    ///
    /// # Errors
    ///
    /// [`LinkError`] for a port failure, a refused command, or a wrong echo.
    pub fn connect<S: SerialPort, P: Patience>(
        self,
        port: &S,
        baud: u32,
        mut mk: impl FnMut() -> P,
    ) -> Result<(), LinkError<S::Error>> {
        let open = open_payload(self.protocol).map_err(LinkError::Build)?;
        self.step(port, Command::ProOpen, &open, &mut mk())?;
        let com = set_com_param(&[baud_record(baud)]).map_err(LinkError::Build)?;
        self.step(port, Command::ProSetComParam, &com, &mut mk())?;
        let filt = set_filter_payload(FILTER_PASS).map_err(LinkError::Build)?;
        self.step(port, Command::ProSetFilter, &filt, &mut mk())?;
        let start = start_payload();
        self.step(port, Command::ProStart, &start, &mut mk())?;
        Ok(())
    }

    /// Install one ISO 15765 flow-control filter (mask / pattern / flow CAN ids).
    ///
    /// `pattern` is the response id the dongle accepts, `mask` the match mask
    /// (`FF FF FF FF`), `flow` the request id the dongle flow-controls with — the
    /// bytes a J2534 `PassThruStartMsgFilter` `FLOW_CONTROL` filter carries.
    ///
    /// # Errors
    ///
    /// [`LinkError::Build`] if the filter copy is malformed, else the command step's errors.
    pub fn add_flow_filter<S: SerialPort, P: Patience>(
        self,
        port: &S,
        slot: u8,
        pattern: &[u8],
        mask: &[u8],
        flow: &[u8],
        patience: &mut P,
    ) -> Result<(), LinkError<S::Error>> {
        let record = vcxtoolkit::start_filter(&FilterCopy {
            slot,
            kind: 3,
            pattern,
            mask,
            flow,
            extend_first: false,
        })
        .map_err(LinkError::Build)?;
        let payload = add_filter(&[record]).map_err(LinkError::Build)?;
        self.step(port, Command::ProAddFilter, &payload, patience)?;
        Ok(())
    }

    /// Send one ISO 15765 message and read its one-byte ack.
    ///
    /// `data` is the J2534 `PASSTHRU_MSG.Data` — `[4-byte CAN id][application PDU]`
    /// in raw mode. It is copied to the Send PDU's offset 6 unchanged and framed by
    /// `VCX_PRO_Send`; the ECU reply is read separately by [`Link::read`]. The ack
    /// frame's status byte is returned (its meaning is not formally recovered).
    ///
    /// # Errors
    ///
    /// [`LinkError::Build`] if the data is over the forward cap, else as the send
    /// round-trip.
    pub fn send<S: SerialPort, P: Patience>(
        self,
        port: &S,
        data: &[u8],
        patience: &mut P,
    ) -> Result<u8, LinkError<S::Error>> {
        let copied = copied_data(self.protocol, data, false).map_err(LinkError::Build)?;
        let pdu = send_payload(0, &copied).map_err(LinkError::Build)?;
        // The Send reply header byte 2 is 0x00 (not a command echo), so read the
        // frame directly rather than through `step`.
        let reply = exchange_with(
            port,
            pro_send_header(self.caller, self.channel),
            &pdu,
            patience,
        )
        .map_err(LinkError::Send)?;
        Ok(reply.payload.first().copied().unwrap_or(0))
    }

    /// Read one queued reply frame and return its application bytes.
    ///
    /// The returned bytes are the recovered Recv envelope's payload — for ISO 15765
    /// the whole `[4-byte rx CAN id][application PDU]`, which the J2534 app expects
    /// in `Data` (the consumer strips the id). This reads the next frame the dongle
    /// pushed; it sends nothing, and in particular sends no `VCX_PRO_Recv` request
    /// (its bytes are not recovered — see the module docs).
    ///
    /// # Errors
    ///
    /// [`LinkError::Read`] on a port failure or timeout, [`LinkError::BadReply`]
    /// when the frame is not a well-formed envelope (e.g. only the one-byte ack
    /// arrived, or no reply yet).
    pub fn read<S: SerialPort, P: Patience>(
        self,
        port: &S,
        patience: &mut P,
    ) -> Result<Vec<u8>, LinkError<S::Error>> {
        let frame = read_one_frame(port, patience).map_err(LinkError::Read)?;
        reply_application_bytes(&frame.payload)
            .map(<[u8]>::to_vec)
            .map_err(LinkError::BadReply)
    }

    /// The real-hardware connect: [`Link::connect`] paced by [`Deadline`]s.
    ///
    /// # Errors
    ///
    /// As [`Link::connect`].
    pub fn connect_timed<S: SerialPort>(
        self,
        port: &S,
        baud: u32,
        timeout: Duration,
    ) -> Result<(), LinkError<S::Error>> {
        self.connect(port, baud, || Deadline::new(timeout))
    }
}

/// Read bytes until one `0xBB`…`0xBB` frame unpacks, pacing empty reads with `patience`.
///
/// This is the read half of `vcxtoolkit`'s `exchange` (whose frame scanner is not
/// public), kept here so a J2534 `PassThruReadMsgs` can read a queued reply without
/// first writing a request.
fn read_one_frame<S: SerialPort, P: Patience>(
    port: &S,
    patience: &mut P,
) -> Result<Frame, ExchangeError<S::Error>> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let mut scratch = [0_u8; 256];
        let got = port.read(&mut scratch).map_err(ExchangeError::Read)?;
        if got == 0 {
            if !patience.keep_waiting() {
                return Err(ExchangeError::Timeout);
            }
            continue;
        }
        buf.extend_from_slice(&scratch[..got]);
        if let Some(frame) = first_frame(&buf) {
            return Ok(frame);
        }
    }
}

/// The first `0xBB`…`0xBB` slice of `buf` that [`vcxtoolkit::unpack`] accepts.
fn first_frame(buf: &[u8]) -> Option<Frame> {
    let mut start = 0;
    while start < buf.len() {
        let open = start + buf[start..].iter().position(|&b| b == MARKER)?;
        let close = open + 1 + buf[open + 1..].iter().position(|&b| b == MARKER)?;
        if let Ok(frame) = vcxtoolkit::unpack(&buf[open..=close]) {
            return Some(frame);
        }
        start = open + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::{Link, LinkError};
    use vcxtoolkit::collect::IDENTITY_POSITIVE_SID;
    use vcxtoolkit::{Command, Patience, SerialPort, pack};

    #[derive(Debug)]
    struct FakeError;
    impl std::fmt::Display for FakeError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("fake serial failure")
        }
    }
    impl std::error::Error for FakeError {}

    // An in-process serial port: writes are recorded, reads pop canned frames.
    struct FakeSerial {
        writes: RefCell<Vec<Vec<u8>>>,
        reads: RefCell<VecDeque<Vec<u8>>>,
    }
    impl FakeSerial {
        fn new(frames: Vec<Vec<u8>>) -> Self {
            Self {
                writes: RefCell::new(Vec::new()),
                reads: RefCell::new(frames.into_iter().collect()),
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
                Some(bytes) => {
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

    // Give up after 0 extra tries, never sleeping.
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

    // An ack frame echoing `command` in header byte 2 with one status byte, as the
    // live dongle answers (e.g. SetComParam -> 80 00 45 00 / 0x95).
    fn ack(command: u8, status: u8) -> Vec<u8> {
        pack([0x80, 0x00, command, 0x00], &[status]).unwrap()
    }

    // A Send ack: header byte 2 is 0x00, one status byte (0x9B live).
    fn send_ack(status: u8) -> Vec<u8> {
        pack([0x80, 0x00, 0x00, 0x00], &[status]).unwrap()
    }

    // A separate ECU reply frame: the recovered Recv envelope (flags u32 LE,
    // len u16 LE, then the application bytes) as the frame payload.
    fn ecu_reply(app: &[u8]) -> Vec<u8> {
        let mut pdu = Vec::new();
        pdu.extend_from_slice(&0_u32.to_le_bytes());
        pdu.extend_from_slice(&u16::try_from(app.len()).unwrap().to_le_bytes());
        pdu.extend_from_slice(app);
        pack([0x80, 0x00, 0x00, 0x00], &pdu).unwrap()
    }

    #[test]
    fn the_connect_sequence_is_open_setcomparam_setfilter_start() {
        let fake = FakeSerial::new(vec![
            ack(Command::ProOpen.code(), 0x00),
            ack(Command::ProSetComParam.code(), 0x95),
            ack(Command::ProSetFilter.code(), 0x00),
            ack(Command::ProStart.code(), 0x9B),
        ]);
        Link::default()
            .connect(&fake, 500_000, || Tries(0))
            .expect("connect");
        // Four commands were written, in the recovered order.
        let writes = fake.writes.borrow();
        assert_eq!(writes.len(), 4);
        for (frame, code) in writes.iter().zip([
            Command::ProOpen.code(),
            Command::ProSetComParam.code(),
            Command::ProSetFilter.code(),
            Command::ProStart.code(),
        ]) {
            let unpacked = vcxtoolkit::unpack(frame).unwrap();
            assert_eq!(unpacked.header[2], code);
        }
        // The open carries the recovered ISO 15765 dword 0x8001 big-endian.
        let open = vcxtoolkit::unpack(&writes[0]).unwrap();
        assert_eq!(open.payload, vcxtoolkit::open_payload(6).unwrap());
    }

    #[test]
    fn a_refused_open_is_surfaced() {
        let fake = FakeSerial::new(vec![ack(0x91, 0x00)]);
        let err = Link::default()
            .connect(&fake, 500_000, || Tries(0))
            .unwrap_err();
        assert!(matches!(err, LinkError::Refused(0x40)));
    }

    #[test]
    fn a_flow_filter_sends_the_can_ids() {
        let fake = FakeSerial::new(vec![ack(Command::ProAddFilter.code(), 0x96)]);
        Link::default()
            .add_flow_filter(
                &fake,
                0,
                &[0x00, 0x00, 0x07, 0xE8], // response id
                &[0xFF, 0xFF, 0xFF, 0xFF], // mask
                &[0x00, 0x00, 0x07, 0xE0], // request id (flow control)
                &mut Tries(0),
            )
            .expect("filter");
        assert_eq!(fake.writes.borrow().len(), 1);
        let f = vcxtoolkit::unpack(&fake.writes.borrow()[0]).unwrap();
        assert_eq!(f.header[2], Command::ProAddFilter.code());
    }

    #[test]
    fn send_then_a_separate_read_yields_the_ecu_reply() {
        // Send gets a one-byte 0x9B ack; the ECU reply is a SEPARATE frame.
        let app = [
            0x00,
            0x00,
            0x07,
            0xE8,
            IDENTITY_POSITIVE_SID,
            0xC1,
            0x31,
            0x32,
        ];
        let fake = FakeSerial::new(vec![send_ack(0x9B), ecu_reply(&app)]);
        let link = Link::default();
        // J2534 Data in raw mode: [4-byte req id][service...]. The id 00 00 07 E0,
        // then 1A C1 — the app's bytes, copied verbatim to the Send PDU.
        let status = link
            .send(&fake, &[0x00, 0x00, 0x07, 0xE0, 0x1A, 0xC1], &mut Tries(0))
            .expect("send");
        assert_eq!(status, 0x9B, "the Send ack byte is surfaced, not the reply");
        // The separate read returns the whole reply Data, rx id included.
        let reply = link.read(&fake, &mut Tries(0)).expect("read");
        assert_eq!(reply, app);
        // The Send PDU copies the app's bytes to offset 6 verbatim (flags 0, len 6).
        let sent = vcxtoolkit::unpack(&fake.writes.borrow()[0]).unwrap();
        assert_eq!(
            sent.payload,
            vec![
                0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x07, 0xE0, 0x1A, 0xC1
            ]
        );
    }

    #[test]
    fn a_read_with_only_the_ack_queued_is_not_a_reply() {
        // If only the one-byte ack is present, the envelope parse rejects it: no
        // reply yet, not a fabricated one.
        let fake = FakeSerial::new(vec![send_ack(0x9B)]);
        let err = Link::default().read(&fake, &mut Tries(0)).unwrap_err();
        assert!(matches!(err, LinkError::BadReply(_)));
    }

    #[test]
    fn a_read_with_nothing_queued_times_out() {
        let fake = FakeSerial::new(vec![]);
        let err = Link::default().read(&fake, &mut Tries(0)).unwrap_err();
        assert!(matches!(
            err,
            LinkError::Read(vcxtoolkit::ExchangeError::Timeout)
        ));
    }
}
