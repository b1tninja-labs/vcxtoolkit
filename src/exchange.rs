//! One request on the port and the first reply.
//!
//! The read and write are driven through the [`SerialPort`] trait, so the same
//! logic runs against the real USB port (which implements it on Windows) and
//! against an in-process fake in tests. The pacing of empty reads is a separate
//! [`Patience`] seam: on hardware [`Deadline`] sleeps between tries and gives up
//! at a wall-clock deadline; a test can give up after a fixed number of tries
//! without sleeping.
//!
//! Treating a buffer with no closing `0xBB` as a finished frame made
//! `a_truncated_buffer_is_not_a_frame` fail, and that return was restored.

use std::time::{Duration, Instant};

const MARKER: u8 = 0xBB;

const EMPTY_READ_PAUSE: Duration = Duration::from_millis(10);

/// A byte-oriented serial port: the read and write [`exchange`] drives.
///
/// [`crate::Port`] implements this on Windows. A fake serial implements it in
/// tests, so the whole open/send/recv sequence runs over canned bytes.
pub trait SerialPort {
    /// The error a read or write can return.
    type Error;

    /// Read the bytes already buffered into `buf`, returning how many.
    ///
    /// Zero means none were ready yet; more may arrive on a later read.
    ///
    /// # Errors
    ///
    /// Returns the port's own read failure.
    fn read(&self, buf: &mut [u8]) -> Result<usize, Self::Error>;

    /// Write `buf`, returning how many bytes the port accepted.
    ///
    /// # Errors
    ///
    /// Returns the port's own write failure.
    fn write(&self, buf: &[u8]) -> Result<usize, Self::Error>;
}

#[cfg(windows)]
impl SerialPort for crate::Port {
    type Error = crate::OpenError;

    fn read(&self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        crate::Port::read(self, buf)
    }

    fn write(&self, buf: &[u8]) -> Result<usize, Self::Error> {
        crate::Port::write(self, buf)
    }
}

/// Decides whether to keep waiting after the port made no progress.
///
/// The read loop and the write loop consult it only after a read returned no
/// bytes, or a write accepted none. Returning `false` ends the exchange with
/// [`ExchangeError::Timeout`].
pub trait Patience {
    /// Called after the port made no progress. `true` tries again, `false` gives up.
    fn keep_waiting(&mut self) -> bool;
}

/// A [`Patience`] that waits until a wall-clock deadline, pausing between tries.
///
/// This is the hardware pacing: it sleeps a short fixed pause between empty
/// reads so the thread does not spin, and gives up once `timeout` has elapsed.
#[derive(Clone, Copy, Debug)]
pub struct Deadline {
    until: Instant,
}

impl Deadline {
    /// A deadline `timeout` from now.
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self {
            until: Instant::now() + timeout,
        }
    }
}

impl Patience for Deadline {
    fn keep_waiting(&mut self) -> bool {
        if Instant::now() >= self.until {
            return false;
        }
        // A completed read or write of nothing is not progress. Pause so this
        // thread does not spin, then check the deadline again.
        std::thread::sleep(EMPTY_READ_PAUSE);
        Instant::now() < self.until
    }
}

/// The first slice of `buf` that [`crate::unpack`] accepts.
///
/// The slice runs from one `0xBB` through the next `0xBB`. A buffer that has
/// opened a frame and not yet reached the closing marker is not a frame.
/// When more than one frame is already in `buf`, this returns the first.
fn first_frame(buf: &[u8]) -> Option<crate::Frame> {
    let mut start = 0;
    while start < buf.len() {
        let rel = buf[start..].iter().position(|byte| *byte == MARKER)?;
        let open = start + rel;
        let end_rel = buf[open + 1..].iter().position(|byte| *byte == MARKER)?;
        let close = open + 1 + end_rel;
        if let Ok(frame) = crate::unpack(&buf[open..=close]) {
            return Some(frame);
        }
        start = open + 1;
    }
    None
}

/// Pack `header` and `payload`, write every packed byte, and read one reply.
///
/// This is [`exchange_with`] with a [`Deadline`] built from `timeout`, the pacing
/// the live USB port uses. The only bytes it writes are the frame packed from
/// `header` and `payload`; it opens no channel of its own.
///
/// # Errors
///
/// Returns [`ExchangeError::Pack`] when [`crate::pack`] refuses the payload,
/// [`ExchangeError::Write`] when the write fails, [`ExchangeError::Read`] when
/// a read fails, and [`ExchangeError::Timeout`] when the deadline passes with
/// no complete frame.
#[must_use = "a reply that is dropped was not read"]
pub fn exchange<S: SerialPort>(
    port: &S,
    header: [u8; 4],
    payload: &[u8],
    timeout: Duration,
) -> Result<crate::Frame, ExchangeError<S::Error>> {
    let mut patience = Deadline::new(timeout);
    exchange_with(port, header, payload, &mut patience)
}

/// Pack `header` and `payload`, write every packed byte, and read one reply,
/// pacing empty reads with `patience`.
///
/// Each candidate reply is a `0xBB`…`0xBB` slice passed to [`crate::unpack`]. The
/// first slice that unpacks is the reply. Bytes after that frame are left unread.
/// It does not start a thread.
///
/// # Errors
///
/// Returns [`ExchangeError::Pack`] when [`crate::pack`] refuses the payload,
/// [`ExchangeError::Write`] when the write fails, [`ExchangeError::Read`] when
/// a read fails, and [`ExchangeError::Timeout`] when `patience` gives up before
/// a complete frame is buffered.
#[must_use = "a reply that is dropped was not read"]
pub fn exchange_with<S: SerialPort, P: Patience>(
    port: &S,
    header: [u8; 4],
    payload: &[u8],
    patience: &mut P,
) -> Result<crate::Frame, ExchangeError<S::Error>> {
    let packed = crate::pack(header, payload).map_err(ExchangeError::Pack)?;
    write_all(port, &packed, patience)?;
    read_frame(port, patience)
}

/// Why [`exchange`] did not return a frame.
#[derive(Debug)]
pub enum ExchangeError<E> {
    /// [`crate::pack`] refused the header and payload.
    Pack(crate::FrameError),
    /// The write failed.
    Write(E),
    /// The read failed.
    Read(E),
    /// [`Patience`] gave up before a complete frame was buffered.
    Timeout,
}

impl<E: std::fmt::Display> std::fmt::Display for ExchangeError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(error) => write!(f, "pack: {error}"),
            Self::Write(error) => write!(f, "write: {error}"),
            Self::Read(error) => write!(f, "read: {error}"),
            Self::Timeout => f.write_str("no complete frame before the deadline"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for ExchangeError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pack(error) => Some(error),
            Self::Write(error) | Self::Read(error) => Some(error),
            Self::Timeout => None,
        }
    }
}

fn write_all<S: SerialPort, P: Patience>(
    port: &S,
    bytes: &[u8],
    patience: &mut P,
) -> Result<(), ExchangeError<S::Error>> {
    let mut sent = 0;
    while sent < bytes.len() {
        let accepted = port.write(&bytes[sent..]).map_err(ExchangeError::Write)?;
        let advanced = accepted.min(bytes.len() - sent);
        if advanced == 0 {
            if !patience.keep_waiting() {
                return Err(ExchangeError::Timeout);
            }
            continue;
        }
        sent += advanced;
    }
    Ok(())
}

fn read_frame<S: SerialPort, P: Patience>(
    port: &S,
    patience: &mut P,
) -> Result<crate::Frame, ExchangeError<S::Error>> {
    let mut buf = Vec::new();
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

#[cfg(test)]
mod tests {
    const GET_INFO: [u8; 7] = [0xBB, 0x80, 0x00, 0x8C, 0x00, 0x0C, 0xBB];

    #[test]
    fn a_growing_buffer_finds_the_get_info_request_when_it_is_complete() {
        assert_eq!(
            crate::pack([0x80, 0x00, 0x8C, 0x00], &[]).unwrap(),
            GET_INFO
        );
        let mut buf = Vec::new();
        for (index, byte) in GET_INFO.iter().enumerate() {
            buf.push(*byte);
            let found = super::first_frame(&buf);
            if index + 1 < GET_INFO.len() {
                assert!(found.is_none(), "prefix was treated as a frame: {buf:02x?}");
            } else {
                let frame = found.expect("complete GetInfo request");
                assert_eq!(frame.header, [0x80, 0x00, 0x8C, 0x00]);
                assert_eq!(frame.payload, [] as [u8; 0]);
            }
        }
    }

    #[test]
    fn a_truncated_buffer_is_not_a_frame() {
        assert!(super::first_frame(&GET_INFO[..GET_INFO.len() - 1]).is_none());
        assert!(super::first_frame(&[]).is_none());
        assert!(super::first_frame(&[0xBB]).is_none());
    }

    #[test]
    fn the_first_of_two_frames_is_the_result() {
        let second = crate::pack([0x80, 0x00, 0x80, 0x00], b"TEST").unwrap();
        let mut buf = GET_INFO.to_vec();
        buf.extend_from_slice(&second);
        let frame = super::first_frame(&buf).expect("first frame");
        assert_eq!(frame.header, [0x80, 0x00, 0x8C, 0x00]);
        assert_eq!(frame.payload, [] as [u8; 0]);
    }

    #[test]
    fn timeout_says_the_deadline_passed() {
        let error: super::ExchangeError<std::io::Error> = super::ExchangeError::Timeout;
        assert_eq!(error.to_string(), "no complete frame before the deadline");
        assert!(std::error::Error::source(&error).is_none());
    }

    // GetInfo only: header 80 00 8C 00, empty payload. No other command.
    #[cfg(windows)]
    #[test]
    #[ignore = "needs the CH343 dongle on COM4"]
    fn com4_get_info_round_trip() {
        let port = crate::open_usb(4, 0).expect("COM4");
        let frame = super::exchange(
            &port,
            [0x80, 0x00, 0x8C, 0x00],
            &[],
            std::time::Duration::from_secs(2),
        )
        .expect("GetInfo");
        let info = crate::device_info(&frame).expect("device info");
        assert_eq!(info.hw_name, "VCX-NANO");
        assert_eq!(info.fw_version, [1, 9, 4, 2]);
    }
}
