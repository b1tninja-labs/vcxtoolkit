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

/// Largest buffer `VCX_SendFrame` accepts.
///
/// The four header bytes count toward this limit. [`MAX_PAYLOAD`] is the same
/// number, applied by `VCXCommSendFrame` to the payload length alone.
pub const MAX_SEND: usize = 0x1026;

/// Why `VCX_SendFrame` does not hand a buffer to the inner send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SendCut {
    /// The length is 0 or above [`MAX_SEND`]. The export returns `0x94`.
    Refused,
    /// The length is 1, 2, or 3. Subtracting 4 wraps, the inner check fails,
    /// and the export returns `0x82`.
    Underflow,
}

impl std::fmt::Display for SendCut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Refused => "send buffer length is 0 or above 0x1026",
            Self::Underflow => "send buffer is shorter than its four header bytes",
        })
    }
}

impl std::error::Error for SendCut {}

/// Header and payload `VCX_SendFrame` takes from one caller buffer.
///
/// The length argument is the whole buffer. The first four bytes are the
/// header and the rest is the payload.
///
/// # Errors
///
/// Returns [`SendCut::Refused`] when `buffer` is empty or longer than
/// [`MAX_SEND`], and [`SendCut::Underflow`] when it is shorter than 4 bytes.
#[must_use = "a send cut that is dropped was not sent"]
pub fn send_cut(buffer: &[u8]) -> Result<([u8; 4], &[u8]), SendCut> {
    if buffer.is_empty() || buffer.len() > MAX_SEND {
        return Err(SendCut::Refused);
    }
    if buffer.len() < 4 {
        return Err(SendCut::Underflow);
    }
    Ok(([buffer[0], buffer[1], buffer[2], buffer[3]], &buffer[4..]))
}

/// One queued body after `0x7B2F4860` splits it.
///
/// The queued bytes are the header, the payload, and one checksum byte. The
/// length stored in the struct is the body length minus 5. This split does
/// not compare the checksum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueuedBody<'a> {
    /// The first four queued bytes.
    pub header: [u8; 4],
    /// The bytes between the header and the checksum.
    pub payload: &'a [u8],
    /// The last queued byte.
    pub checksum: u8,
}

/// Split a queued body the way `0x7B2F4860` does.
///
/// A body shorter than 5 bytes is `None`. The DLL subtracts 5 from that
/// length with no lower check, and the subtraction wraps.
#[must_use]
pub fn queued_body(body: &[u8]) -> Option<QueuedBody<'_>> {
    if body.len() < MIN_BODY {
        return None;
    }
    let checksum_at = body.len() - 1;
    Some(QueuedBody {
        header: [body[0], body[1], body[2], body[3]],
        payload: &body[4..checksum_at],
        checksum: body[checksum_at],
    })
}

/// Length `VCX_RecvFrame` stores. It is the payload length plus the four
/// header bytes it writes in front of that payload.
#[must_use]
pub fn recv_length(payload_len: usize) -> Option<u32> {
    u32::try_from(payload_len.checked_add(4)?).ok()
}

/// Bytes `0x7B2F4B60` writes around one ring chunk.
///
/// The chunk is copied unchanged. A `0xBB` is written before it and after it.
/// The stored length is the chunk length plus 2. There is no checksum and no
/// escape in this copy.
///
/// Returns `None` when that length does not fit in a `u32`.
#[must_use]
pub fn marker_wrap(chunk: &[u8]) -> Option<(Vec<u8>, u32)> {
    let stored = u32::try_from(chunk.len().checked_add(2)?).ok()?;
    let mut out = Vec::with_capacity(chunk.len() + 2);
    out.push(MARKER);
    out.extend_from_slice(chunk);
    out.push(MARKER);
    Some((out, stored))
}

/// Escape prefix the device uses when header byte 0 is not `0x80`.
pub const PLUS_ESCAPE: u8 = 0xAA;

/// Closing byte of that same frame. The opening byte is still `0xBB`.
pub const PLUS_END: u8 = 0xEE;

/// Where the device stores the checksum in its frame struct.
pub const DEVICE_CHECKSUM: usize = 0x102E;

/// `true` when the device's sender at `0x0801C17E` will build a frame.
///
/// The payload length has to be at least 1 and at most [`MAX_PAYLOAD`].
#[must_use]
pub fn device_will_send(payload_len: usize) -> bool {
    (1..=MAX_PAYLOAD).contains(&payload_len)
}

fn write_plus(out: &mut Vec<u8>, byte: u8) {
    let stuffed = match byte {
        PLUS_ESCAPE => [PLUS_ESCAPE, 0x0A],
        MARKER => [PLUS_ESCAPE, 0x0B],
        PLUS_END => [PLUS_ESCAPE, 0x0E],
        other => {
            out.push(other);
            return;
        }
    };
    out.extend_from_slice(&stuffed);
}

/// Frame the device builds in `0x0801C17E`.
///
/// A payload length of 0, or one past [`MAX_PAYLOAD`], returns `None`. When
/// header byte 0 is `0x80`, the bytes are [`pack`]. Otherwise the frame opens
/// with `0xBB`, escapes header byte 2, the payload, and the checksum with
/// [`PLUS_ESCAPE`], and closes with [`PLUS_END`].
#[must_use]
pub fn device_frame(header: [u8; 4], payload: &[u8]) -> Option<Vec<u8>> {
    if !device_will_send(payload.len()) {
        return None;
    }
    if header[0] == 0x80 {
        return pack(header, payload).ok();
    }
    let sum = checksum(header, payload);
    let mut out = Vec::with_capacity(payload.len() + 6);
    out.push(MARKER);
    write_plus(&mut out, header[2]);
    for byte in payload {
        write_plus(&mut out, *byte);
    }
    write_plus(&mut out, sum);
    out.push(PLUS_END);
    Some(out)
}

/// Most bytes `0x0801BECA` keeps after undoing [`PLUS_ESCAPE`].
///
/// The compare is `unstuffed < 0x1003`.
pub const PLUS_BODY_MAX: usize = 0x1002;

/// Undo the `0xAA` escape in one interior, the bytes between `0xBB` and `0xEE`.
///
/// A stuffed interior shorter than 2 bytes is `None`. An `0xAA` with no
/// following byte is `None`. The device reads that next byte with no separate
/// end check. Each escaped byte is `next | (next << 4)`, so `0x0A` is `0xAA`,
/// `0x0B` is `0xBB`, and `0x0E` is `0xEE`. The first plain byte is header byte
/// 2. The device also stores header byte 0 as 0. The last plain byte is the
/// checksum. The bytes between them are the payload, and the stored length is
/// the plain length minus 2. A plain length below 2, or above
/// [`PLUS_BODY_MAX`], is `None`.
#[must_use]
pub fn plus_body(stuffed: &[u8]) -> Option<(u8, Vec<u8>, u8)> {
    if stuffed.len() < 2 {
        return None;
    }
    let mut plain = Vec::with_capacity(stuffed.len());
    let mut index = 0;
    while index < stuffed.len() {
        if stuffed[index] == PLUS_ESCAPE {
            let next = *stuffed.get(index + 1)?;
            plain.push(next | (next << 4));
            index += 2;
        } else {
            plain.push(stuffed[index]);
            index += 1;
        }
    }
    if plain.len() < 2 || plain.len() > PLUS_BODY_MAX {
        return None;
    }
    let command = plain[0];
    let checksum_byte = plain[plain.len() - 1];
    let payload = plain[1..plain.len() - 1].to_vec();
    Some((command, payload, checksum_byte))
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
    use super::{
        ESCAPE, ESCAPE_ALSO, FrameError, MARKER, MAX_PAYLOAD, MAX_SEND, PLUS_BODY_MAX, PLUS_END,
        PLUS_ESCAPE, SendCut, checksum, device_frame, device_will_send, marker_wrap, pack,
        plus_body, queued_body, recv_length, send_cut, unpack,
    };

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

    #[test]
    fn the_export_counts_the_header_in_the_length() {
        let request = [0x80, 0x00, 0x8C, 0x00];
        assert_eq!(send_cut(&request).unwrap(), (request, &[][..]));
        assert_eq!(send_cut(&[]), Err(SendCut::Refused));
        assert_eq!(send_cut(&[0x80, 0x00, 0x8C]), Err(SendCut::Underflow));
        assert_eq!(send_cut(&vec![0_u8; MAX_SEND + 1]), Err(SendCut::Refused));
        let mut full = vec![0_u8; MAX_SEND];
        full[..4].copy_from_slice(&request);
        let (header, payload) = send_cut(&full).unwrap();
        assert_eq!(header, request);
        assert_eq!(payload.len(), MAX_SEND - 4);

        let body = [0x80, 0x00, 0x8C, 0x00, 0x11, 0x22];
        let queued = queued_body(&body).unwrap();
        assert_eq!(queued.header, request);
        assert_eq!(queued.payload, &[0x11]);
        assert_eq!(queued.checksum, 0x22);
        assert_eq!(queued_body(&[0x80, 0x00, 0x8C, 0x00]), None);
        assert_eq!(recv_length(0), Some(4));
        assert_eq!(recv_length(queued.payload.len()), Some(5));
        assert_eq!(
            marker_wrap(b"ABC").unwrap(),
            (vec![MARKER, b'A', b'B', b'C', MARKER], 5)
        );
    }

    #[test]
    fn the_device_closes_a_non_80_header_with_ee() {
        let header = [0x80, 0x00, 0x8C, 0x00];
        let payload = [0x11, MARKER];
        assert_eq!(
            device_frame(header, &payload).unwrap(),
            pack(header, &payload).unwrap()
        );
        assert!(!device_will_send(0));
        assert_eq!(device_frame(header, &[]), None);
        assert!(!device_will_send(MAX_PAYLOAD + 1));
        let other = [0x00, 0x00, MARKER, 0x00];
        assert_eq!(
            device_frame(other, &[PLUS_END, 0x01]).unwrap(),
            vec![MARKER, 0xAA, 0x0B, 0xAA, 0x0E, 0x01, 0xAA, 0x0A, PLUS_END]
        );
    }

    #[test]
    fn an_aa_escape_comes_back_out_of_the_interior() {
        let header = [0x00, 0x00, 0x11, 0x00];
        let payload = [PLUS_ESCAPE, MARKER, PLUS_END];
        let wire = device_frame(header, &payload).unwrap();
        let (command, body, sum) = plus_body(&wire[1..wire.len() - 1]).unwrap();
        assert_eq!(command, 0x11);
        assert_eq!(body, payload);
        assert_eq!(sum, checksum(header, &payload));
        assert_eq!(plus_body(&[0x11, 0x11]), Some((0x11, vec![], 0x11)));
        assert_eq!(plus_body(&[PLUS_ESCAPE]), None);
        assert_eq!(plus_body(&[0x11]), None);
        assert_eq!(plus_body(&vec![0x01; PLUS_BODY_MAX + 1]), None);
    }
}
