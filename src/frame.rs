//! One VCX serial frame.
//!
//! No specification for this framing was found. The packing, the escapes and the checksum were
//! read from `VCX.dll` and checked against a live `GetInfo` answer on 2026-11-20. A frame is the
//! four header bytes, the payload, and one checksum byte, wrapped in `0xBB` markers. A `0xBB`,
//! `0xDD` or `0xEE` inside that body is written as `0xDD` followed by the bitwise complement.

/// Largest payload `VCXCommSendFrame` will send.
pub const MAX_PAYLOAD: usize = 0x1026;

const MARKER: u8 = 0xBB;
const ESCAPE: u8 = 0xDD;
const ESCAPE_ALSO: u8 = 0xEE;
const MIN_BODY: usize = 5;
const MAX_BODY: usize = 0x102B;

/// A decoded frame. The header is the four bytes that precede the payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    /// The four bytes before the payload.
    pub header: [u8; 4],
    /// The bytes between the header and the checksum.
    pub payload: Vec<u8>,
}

/// Why a frame was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    /// The payload is longer than [`MAX_PAYLOAD`].
    PayloadTooLong,
    /// The slice is not one complete `0xBB` … `0xBB` frame.
    Truncated,
    /// A byte inside the body is not a stuffed `0xBB`, `0xDD` or `0xEE`.
    BadMarker,
    /// The unpacked body does not start with `0x80`.
    BadHeader,
    /// The unpacked body is shorter than 5 bytes or longer than `0x102B`.
    BadLength,
    /// The checksum byte does not match the header and the payload.
    BadChecksum,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::PayloadTooLong => "payload longer than 0x1026",
            Self::Truncated => "frame is not one complete BB-delimited buffer",
            Self::BadMarker => "an unescaped marker byte inside the frame",
            Self::BadHeader => "unpacked frame does not start with 0x80",
            Self::BadLength => "unpacked frame length is outside 5..=0x102B",
            Self::BadChecksum => "checksum does not match",
        })
    }
}

impl std::error::Error for FrameError {}

/// The checksum byte for a header and a payload.
///
/// When the first header byte is `0x80`, the sum starts at `header[3] - 0x80 + header[1]`,
/// wrapping. The third header byte is added. Payload bytes are then taken in pairs: the even
/// index of each pair goes into one sum and the odd index into another. A final unpaired byte
/// is added to the running total, and then the two pair sums are added. Every addition is mod 256.
#[must_use]
pub fn checksum(header: [u8; 4], payload: &[u8]) -> u8 {
    let mut total = header[2];
    if header[0] == 0x80 {
        total = total.wrapping_add(header[3].wrapping_sub(0x80).wrapping_add(header[1]));
    }
    let mut even = 0_u8;
    let mut odd = 0_u8;
    let pairs = payload.len() / 2;
    for pair in 0..pairs {
        even = even.wrapping_add(payload[pair * 2]);
        odd = odd.wrapping_add(payload[pair * 2 + 1]);
    }
    if payload.len() % 2 == 1 {
        total = total.wrapping_add(payload[payload.len() - 1]);
    }
    total.wrapping_add(even).wrapping_add(odd)
}

/// Pack a header and a payload into one wire frame.
///
/// # Errors
///
/// Returns [`FrameError::PayloadTooLong`] when `payload` is longer than [`MAX_PAYLOAD`].
#[must_use = "a frame that is built and dropped was not sent"]
pub fn pack(header: [u8; 4], payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    if payload.len() > MAX_PAYLOAD {
        return Err(FrameError::PayloadTooLong);
    }
    let sum = checksum(header, payload);
    let mut out = Vec::with_capacity(payload.len() + 6);
    out.push(MARKER);
    write_stuffed(&mut out, &header);
    write_stuffed(&mut out, payload);
    write_stuffed(&mut out, &[sum]);
    out.push(MARKER);
    Ok(out)
}

/// Unpack one wire frame.
///
/// The slice must be exactly one frame: it starts and ends with `0xBB`, and nothing follows
/// the closing marker.
///
/// # Errors
///
/// Returns a [`FrameError`] when the slice is not one well-formed frame.
#[must_use = "a frame that is unpacked and dropped was not read"]
pub fn unpack(bytes: &[u8]) -> Result<Frame, FrameError> {
    if bytes.first() != Some(&MARKER) || bytes.last() != Some(&MARKER) || bytes.len() < 2 {
        return Err(FrameError::Truncated);
    }
    let mut body = Vec::with_capacity(bytes.len());
    let mut index = 1;
    let end = bytes.len() - 1;
    while index < end {
        let byte = bytes[index];
        if byte == MARKER {
            return Err(FrameError::BadMarker);
        }
        if byte == ESCAPE {
            let next = bytes.get(index + 1).ok_or(FrameError::Truncated)?;
            if index + 1 >= end {
                return Err(FrameError::Truncated);
            }
            body.push(!next);
            index += 2;
        } else {
            body.push(byte);
            index += 1;
        }
    }
    if body.first() != Some(&0x80) {
        return Err(FrameError::BadHeader);
    }
    if !(MIN_BODY..=MAX_BODY).contains(&body.len()) {
        return Err(FrameError::BadLength);
    }
    let checksum_at = body.len() - 1;
    let payload = body[4..checksum_at].to_vec();
    let header = [body[0], body[1], body[2], body[3]];
    if body[checksum_at] != checksum(header, &payload) {
        return Err(FrameError::BadChecksum);
    }
    Ok(Frame { header, payload })
}

fn write_stuffed(out: &mut Vec<u8>, bytes: &[u8]) {
    for byte in bytes {
        if *byte == MARKER || *byte == ESCAPE || *byte == ESCAPE_ALSO {
            out.push(ESCAPE);
            out.push(!byte);
        } else {
            out.push(*byte);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ESCAPE, ESCAPE_ALSO, FrameError, MARKER, MAX_PAYLOAD, checksum, pack, unpack};

    #[test]
    fn echo_frame_round_trips() {
        let wire = [
            0xBB, 0x80, 0x00, 0x80, 0x00, 0x54, 0x45, 0x53, 0x54, 0x40, 0xBB,
        ];
        assert_eq!(checksum([0x80, 0x00, 0x80, 0x00], b"TEST"), 0x40);
        assert_eq!(pack([0x80, 0x00, 0x80, 0x00], b"TEST").unwrap(), wire);
        let frame = unpack(&wire).unwrap();
        assert_eq!(frame.header, [0x80, 0x00, 0x80, 0x00]);
        assert_eq!(frame.payload, b"TEST");
    }

    #[test]
    fn get_info_request_is_the_empty_command() {
        let wire = [0xBB, 0x80, 0x00, 0x8C, 0x00, 0x0C, 0xBB];
        assert_eq!(checksum([0x80, 0x00, 0x8C, 0x00], &[]), 0x0C);
        assert_eq!(pack([0x80, 0x00, 0x8C, 0x00], &[]).unwrap(), wire);
        let frame = unpack(&wire).unwrap();
        assert_eq!(frame.payload, [] as [u8; 0]);
    }

    #[test]
    fn live_get_info_reply_checks() {
        // Captured on COM4, 2026-11-20, 921600 8N1. The checksum byte is 0x58.
        let wire = [
            0xBB, 0x80, 0x00, 0x8C, 0x00, 0x00, 0x6D, 0x75, 0x98, 0x00, 0x00, 0x40, 0xC1, 0x1D,
            0x11, 0x95, 0xC4, 0x07, 0x45, 0x03, 0x00, 0x00, 0x4E, 0x43, 0x41, 0x47, 0x4D, 0x67,
            0x8B, 0x28, 0x12, 0x00, 0x00, 0x00, 0x56, 0x43, 0x58, 0x2D, 0x4E, 0x41, 0x4E, 0x4F,
            0x00, 0x00, 0x00, 0x11, 0x00, 0x01, 0x01, 0x6B, 0x56, 0x43, 0x58, 0x41, 0x2E, 0x42,
            0x49, 0x4E, 0x02, 0x04, 0x09, 0x01, 0x00, 0x1F, 0x03, 0x35, 0x01, 0x00, 0x00, 0x00,
            0x58, 0xBB,
        ];
        let frame = unpack(&wire).unwrap();
        assert_eq!(frame.header, [0x80, 0x00, 0x8C, 0x00]);
        assert_eq!(frame.payload.len(), 65);
        assert_eq!(pack(frame.header, &frame.payload).unwrap(), wire);
    }

    #[test]
    fn marker_bytes_in_the_payload_are_stuffed() {
        let payload = [MARKER, ESCAPE, ESCAPE_ALSO, 0x01];
        let wire = pack([0x80, 0x00, 0x80, 0x00], &payload).unwrap();
        assert!(wire.windows(2).any(|pair| pair == [ESCAPE, !MARKER]));
        assert_eq!(unpack(&wire).unwrap().payload, payload);
    }

    #[test]
    fn an_odd_payload_adds_the_last_byte_once() {
        let payload = [0x01, 0x02, 0x03];
        let wire = pack([0x80, 0x00, 0x01, 0x00], &payload).unwrap();
        assert_eq!(unpack(&wire).unwrap().payload, payload);
    }

    #[test]
    fn a_bad_checksum_is_refused() {
        let mut wire = pack([0x80, 0x00, 0x8C, 0x00], &[]).unwrap();
        let last = wire.len() - 2;
        wire[last] ^= 0xFF;
        assert_eq!(unpack(&wire), Err(FrameError::BadChecksum));
    }

    #[test]
    fn a_payload_past_the_limit_is_refused() {
        let payload = vec![0_u8; MAX_PAYLOAD + 1];
        assert_eq!(
            pack([0x80, 0x00, 0x80, 0x00], &payload),
            Err(FrameError::PayloadTooLong)
        );
    }
}
