//! One request on the port and the first reply.
//!
//! Treating a buffer with no closing `0xBB` as a finished frame made
//! `a_truncated_buffer_is_not_a_frame` fail, and that return was restored.

#[cfg(windows)]
use std::time::{Duration, Instant};

const MARKER: u8 = 0xBB;

#[cfg(windows)]
const EMPTY_READ_PAUSE: Duration = Duration::from_millis(10);

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
/// The read timeout stays as [`crate::open_usb`] set it, so an empty driver
/// buffer makes [`crate::Port::read`] return 0 at once. This sleeps 10 ms
/// between those empty reads and gives up when `timeout` has elapsed. It does
/// not start a thread. The only bytes it writes are the frame packed from
/// `header` and `payload`.
///
/// Each candidate is a `0xBB`…`0xBB` slice passed to [`crate::unpack`]. The
/// first slice that unpacks is the reply. Bytes after that frame are left
/// unread.
///
/// # Errors
///
/// Returns [`ExchangeError::Pack`] when [`crate::pack`] refuses the payload,
/// [`ExchangeError::Write`] when the write fails, [`ExchangeError::Read`] when
/// a read fails, and [`ExchangeError::Timeout`] when the deadline passes with
/// no complete frame.
#[cfg(windows)]
#[must_use = "a reply that is dropped was not read"]
pub fn exchange(
    port: &crate::Port,
    header: [u8; 4],
    payload: &[u8],
    timeout: Duration,
) -> Result<crate::Frame, ExchangeError> {
    let packed = crate::pack(header, payload).map_err(ExchangeError::Pack)?;
    let deadline = Instant::now() + timeout;
    write_packed(port, &packed, deadline)?;
    read_frame(port, deadline)
}

/// Why [`exchange`] did not return a frame.
#[cfg(windows)]
#[derive(Debug)]
pub enum ExchangeError {
    /// [`crate::pack`] refused the header and payload.
    Pack(crate::FrameError),
    /// The overlapped write failed.
    Write(crate::OpenError),
    /// The overlapped read failed.
    Read(crate::OpenError),
    /// The deadline passed before a complete frame was buffered.
    Timeout,
}

#[cfg(windows)]
impl std::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(error) => write!(f, "pack: {error}"),
            Self::Write(error) => write!(f, "write: {error}"),
            Self::Read(error) => write!(f, "read: {error}"),
            Self::Timeout => f.write_str("no complete frame before the deadline"),
        }
    }
}

#[cfg(windows)]
impl std::error::Error for ExchangeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pack(error) => Some(error),
            Self::Write(error) | Self::Read(error) => Some(error),
            Self::Timeout => None,
        }
    }
}

#[cfg(windows)]
fn write_packed(port: &crate::Port, bytes: &[u8], deadline: Instant) -> Result<(), ExchangeError> {
    let mut sent = 0;
    while sent < bytes.len() {
        let accepted = port.write(&bytes[sent..]).map_err(ExchangeError::Write)?;
        let advanced = accepted.min(bytes.len() - sent);
        if advanced == 0 {
            if Instant::now() >= deadline {
                return Err(ExchangeError::Timeout);
            }
            // A completed write of nothing is not progress. Pause so this thread does not spin.
            std::thread::sleep(EMPTY_READ_PAUSE);
            continue;
        }
        sent += advanced;
    }
    Ok(())
}

#[cfg(windows)]
fn read_frame(port: &crate::Port, deadline: Instant) -> Result<crate::Frame, ExchangeError> {
    let mut buf = Vec::new();
    loop {
        if Instant::now() >= deadline {
            return Err(ExchangeError::Timeout);
        }
        let mut scratch = [0_u8; 256];
        let got = port.read(&mut scratch).map_err(ExchangeError::Read)?;
        if got == 0 {
            std::thread::sleep(EMPTY_READ_PAUSE);
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

    #[cfg(windows)]
    #[test]
    fn timeout_says_the_deadline_passed() {
        let error = super::ExchangeError::Timeout;
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
