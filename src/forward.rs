//! Bytes `VCXPT32.dll` copies into a `VCX_PRO_*` payload.
//!
//! `PassThruWriteMsgs` (`0x100039e0`) calls `FUN_10005df0`, which copies the
//! caller's data to PDU offset 6. `VCX_PRO_Send` (`0x7B2F5740`) then stores that
//! dword big-endian, the length big-endian, and those bytes. `FUN_10005a20`
//! is the dword `VCX_PRO_Open` is given. `wire_id` is the id the device stores
//! from a queued record. Nothing here opens a port.

/// Largest data copy `FUN_10005df0` and `VCX_PRO_Send` accept.
pub const MAX_FORWARD: usize = 0x1020;

/// Why a copied payload was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForwardError {
    /// The data is longer than [`MAX_FORWARD`].
    TooLong,
    /// `FUN_10005a20` logs `Unknown Protocol` and returns 0.
    UnknownProtocol,
    /// A count byte cannot hold the record list.
    TooMany,
    /// A tail is shorter than the length byte that selects it.
    ShortCopy,
    /// The filter kind is not 1, 2, or 3, or the copy is longer than 12 bytes.
    BadFilter,
    /// `FUN_10005840` has no builder and sends the buffer already stored.
    KeptBuffer,
    /// A field the pass-through writes was not recovered.
    Unrecovered,
    /// `SetFilter` and `SetTimer` copy `1` or `2`, and no other byte was recovered.
    Mode,
    /// The message protocol is not the channel protocol, and mixed format does not apply.
    ProtocolMismatch,
    /// `FAST_INIT` returns this code and does not build a payload.
    FastStatus(u8),
}

impl std::fmt::Display for ForwardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TooLong => "forwarded data is longer than 0x1020",
            Self::UnknownProtocol => "protocol has no VCX_PRO_Open dword",
            Self::TooMany => "the record count does not fit in one byte",
            Self::ShortCopy => "a tail is shorter than its length byte",
            Self::BadFilter => "the filter kind or its length is not one this pass-through sends",
            Self::KeptBuffer => "this protocol has no SET_CONFIG builder",
            Self::Unrecovered => "a SET_CONFIG field was not recovered",
            Self::Mode => "SetFilter and SetTimer copy 1 or 2",
            Self::ProtocolMismatch => "the message protocol is not accepted on this channel",
            Self::FastStatus(_) => "FAST_INIT refuses the message",
        })
    }
}

impl std::error::Error for ForwardError {}

/// The dword `VCX_PRO_Open` stores, from the name `FUN_10005a20` logs.
///
/// # Errors
///
/// Returns [`ForwardError::UnknownProtocol`] when the pass-through logs
/// `Unknown Protocol`.
pub fn open_code(protocol: u32) -> Result<u32, ForwardError> {
    Ok(match protocol {
        1 | 0x8000 => 0x8505,
        2 | 0x8001 => 0x8606,
        3 | 0x8002 | 0x800b | 0x10003 => 0x9104,
        4 | 0x8003 => 0x9004,
        5 | 0x8004 | 0x8008 | 0x800f => 0x8101,
        6 | 0x8005 | 0x8007 | 0x8010 | 0x10000 => 0x8001,
        7..=10 => 0x8a0a,
        0x800a | 0x10004 => 0x9204,
        0x800e => 0x8401,
        0x8000_0001 | 0x8000_0002 => 0x9d04,
        _ => return Err(ForwardError::UnknownProtocol),
    })
}

/// Four big-endian bytes of [`open_code`].
///
/// # Errors
///
/// Returns [`ForwardError::UnknownProtocol`] from [`open_code`].
pub fn open_payload(protocol: u32) -> Result<[u8; 4], ForwardError> {
    Ok(open_code(protocol)?.to_be_bytes())
}

/// The `VCX_PRO_Send` payload: flags, length, then `data`.
///
/// The flags dword is stored big-endian. That is the byte reverse `VCX_PRO_Send`
/// writes at `0x7B2F57C0`. The length is a big-endian `u16`. `data` is the copy
/// from PDU offset 6.
///
/// # Errors
///
/// Returns [`ForwardError::TooLong`] when `data` is longer than [`MAX_FORWARD`].
pub fn send_payload(flags: u32, data: &[u8]) -> Result<Vec<u8>, ForwardError> {
    if data.len() > MAX_FORWARD {
        return Err(ForwardError::TooLong);
    }
    let Ok(len) = u16::try_from(data.len()) else {
        return Err(ForwardError::TooLong);
    };
    let mut payload = Vec::with_capacity(6 + data.len());
    payload.extend_from_slice(&flags.to_be_bytes());
    payload.extend_from_slice(&len.to_be_bytes());
    payload.extend_from_slice(data);
    Ok(payload)
}

/// One `VCX_PRO_SetComParam` record: a `u16` and a `u32`, both stored big-endian.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComRecord {
    /// The `u16` at record offset 0.
    pub key: u16,
    /// The `u32` at record offset 4. Record bytes 2 and 3 are not copied.
    pub value: u32,
}

/// The `VCX_PRO_SetComParam` payload. The count stays in the argument and is not a payload byte.
///
/// # Errors
///
/// Returns [`ForwardError::TooMany`] when there are more than 255 records.
pub fn set_com_param(records: &[ComRecord]) -> Result<Vec<u8>, ForwardError> {
    if u8::try_from(records.len()).is_err() {
        return Err(ForwardError::TooMany);
    }
    let mut payload = Vec::with_capacity(records.len() * 6);
    for record in records {
        payload.extend_from_slice(&record.key.to_be_bytes());
        payload.extend_from_slice(&record.value.to_be_bytes());
    }
    Ok(payload)
}

/// One `VCX_PRO_AddFilter` record.
///
/// The four fixed bytes are the bytes at record offsets 0, 4, 8, and 12.
/// Offset 12 is also the length of each tail. The signed dword at offset 4
/// selects how many tails are copied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilterRecord {
    /// Record offset 0.
    pub at0: u8,
    /// The first byte of the dword at offset 4.
    pub at4: u8,
    /// The first byte of the dword at offset 8.
    pub at8: u8,
    /// Record offset 12, and the length of each copied tail.
    pub at12: u8,
    /// The signed dword at offset 4. A negative value copies no tail.
    pub selector: i32,
    /// Bytes at record offset `0x0D`, copied when `selector >= 0`.
    pub from_0x0d: Vec<u8>,
    /// Bytes at record offset `0x19`, copied when `selector >= 1`.
    pub from_0x19: Vec<u8>,
    /// Bytes at record offset `0x25`, copied when `selector >= 2`.
    pub from_0x25: Vec<u8>,
}

/// The `VCX_PRO_AddFilter` payload. The first byte is the record count.
///
/// # Errors
///
/// Returns [`ForwardError::TooMany`] when there are more than 255 records, and
/// [`ForwardError::ShortCopy`] when a selected tail is shorter than `at12`.
pub fn add_filter(records: &[FilterRecord]) -> Result<Vec<u8>, ForwardError> {
    let count = u8::try_from(records.len()).map_err(|_| ForwardError::TooMany)?;
    let mut payload = vec![count];
    for record in records {
        payload.extend_from_slice(&[record.at0, record.at4, record.at8, record.at12]);
        let tails = [
            (0, record.from_0x0d.as_slice()),
            (1, record.from_0x19.as_slice()),
            (2, record.from_0x25.as_slice()),
        ];
        for (need, bytes) in tails {
            if record.selector >= need {
                let length = usize::from(record.at12);
                if bytes.len() < length {
                    return Err(ForwardError::ShortCopy);
                }
                payload.extend_from_slice(&bytes[..length]);
            }
        }
    }
    Ok(payload)
}

/// One `VCX_PRO_AddTimer` record.
///
/// `first[2]` is the number of bytes copied from offset 8. `first[3]` is the
/// number copied from offset `0x18`. The dword at offset 4 is copied in memory order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimerRecord {
    /// The first four record bytes.
    pub first: [u8; 4],
    /// The four bytes at record offset 4.
    pub dword: [u8; 4],
    /// Bytes at record offset 8.
    pub from_8: Vec<u8>,
    /// Bytes at record offset `0x18`.
    pub from_0x18: Vec<u8>,
}

/// The `VCX_PRO_AddTimer` payload. The first byte is the low byte of the count.
///
/// # Errors
///
/// Returns [`ForwardError::TooMany`] when the count does not fit in that byte, and
/// [`ForwardError::ShortCopy`] when a selected extra is short.
pub fn add_timer(records: &[TimerRecord]) -> Result<Vec<u8>, ForwardError> {
    let count = u8::try_from(records.len()).map_err(|_| ForwardError::TooMany)?;
    let mut payload = vec![count];
    for record in records {
        payload.extend_from_slice(&record.first);
        payload.extend_from_slice(&record.dword);
        let extras = [
            (record.first[2], record.from_8.as_slice()),
            (record.first[3], record.from_0x18.as_slice()),
        ];
        for (length, bytes) in extras {
            if length != 0 {
                let length = usize::from(length);
                if bytes.len() < length {
                    return Err(ForwardError::ShortCopy);
                }
                payload.extend_from_slice(&bytes[..length]);
            }
        }
    }
    Ok(payload)
}

/// The extra byte `FUN_10005c10` appends for the UART protocols.
///
/// It is the sum of the data bytes, modulo 256. ISO 15765 does not append it.
#[must_use]
pub fn uart_sum(data: &[u8]) -> u8 {
    data.iter().fold(0, |sum, byte| sum.wrapping_add(*byte))
}

/// The extra byte `FUN_10005ce0` appends for J1850.
///
/// The register starts at `0x7E`. Each data bit is shifted in, and a set high
/// bit xors `0x1D`. Eight more shifts follow, and the result is inverted.
#[must_use]
pub fn j1850_check(data: &[u8]) -> u8 {
    let mut crc = 0x7e_u8;
    for &byte in data {
        let mut mask = 0x80_u8;
        while mask != 0 {
            let shifted = crc.wrapping_shl(1);
            let with_bit = if mask & byte == 0 {
                shifted
            } else {
                shifted | 1
            };
            crc = if crc & 0x80 == 0 {
                with_bit
            } else {
                with_bit ^ 0x1d
            };
            mask >>= 1;
        }
    }
    for _ in 0..8 {
        let shifted = crc.wrapping_shl(1);
        crc = if crc & 0x80 == 0 {
            shifted
        } else {
            shifted ^ 0x1d
        };
    }
    !crc
}

/// Third argument of `VCX_PRO_SetFilter` on connect (`0x10006e6e`).
pub const FILTER_PASS: u8 = 1;

/// Third argument of `VCX_PRO_SetFilter` for `CLEAR_MSG_FILTERS`.
pub const FILTER_CLEAR: u8 = 2;

/// Third argument of `VCX_PRO_Start` when connect calls it (`0x10006e93`).
pub const START_MODE: u8 = 0;

/// Third argument of `VCX_PRO_SetTimer` after `AddTimer`.
pub const TIMER_PASS: u8 = 1;

/// Third argument of `VCX_PRO_SetTimer` for `CLEAR_PERIODIC_MSGS`.
pub const TIMER_CLEAR: u8 = 2;

/// `VCX_PRO_Close` sends no payload.
#[must_use]
pub fn close_payload() -> &'static [u8] {
    &[]
}

/// `VCX_PRO_Stop` sends no payload.
#[must_use]
pub fn stop_payload() -> &'static [u8] {
    &[]
}

/// `VCX_PRO_Start` as connect calls it: one byte, [`START_MODE`].
#[must_use]
pub fn start_payload() -> [u8; 1] {
    [START_MODE]
}

/// `VCX_PRO_SetFilter`'s one-byte payload. Connect sends [`FILTER_PASS`] or [`FILTER_CLEAR`].
///
/// # Errors
///
/// Returns [`ForwardError::Mode`] for any other byte.
pub fn set_filter_payload(mode: u8) -> Result<[u8; 1], ForwardError> {
    if mode == FILTER_PASS || mode == FILTER_CLEAR {
        Ok([mode])
    } else {
        Err(ForwardError::Mode)
    }
}

/// `VCX_PRO_SetTimer`'s one-byte payload. The recovered values are [`TIMER_PASS`] and [`TIMER_CLEAR`].
///
/// # Errors
///
/// Returns [`ForwardError::Mode`] for any other byte.
pub fn set_timer_payload(mode: u8) -> Result<[u8; 1], ForwardError> {
    if mode == TIMER_PASS || mode == TIMER_CLEAR {
        Ok([mode])
    } else {
        Err(ForwardError::Mode)
    }
}

/// The `SetComParam` record connect stores: key `3`, value the caller's baud.
#[must_use]
pub fn baud_record(baud: u32) -> ComRecord {
    ComRecord {
        key: 3,
        value: baud,
    }
}

/// `RxStatus` bits `FUN_10006070` sets from the PDU flags dword.
///
/// Protocols 7 through 10 clear the result. The log names are
/// `RXSTATUS_TX_INDICATION` (`0x8`), `RXSTATUS_RX_BREAK` (`0x4`),
/// `RXSTATUS_START_OF_MESSAGE` (`0x2`), `RXSTATUS_TX_MSG_TYPE` (`0x1`),
/// `RXSTATUS_CAN_29BIT_ID` (`0x100`), `RXSTATUS_ISO15765_ADDR_TYPE` (`0x80`),
/// and `RXSTATUS_SW_CAN_HV_RX` (`0x10000`).
#[must_use]
pub fn rx_status(protocol: u32, flags: u32) -> u32 {
    if matches!(protocol, 7..=10) {
        return 0;
    }
    let mut status = 0_u32;
    if flags & 0x8 != 0 {
        status |= 0x8;
    }
    if flags & 0x2 != 0 {
        status |= 0x4;
    }
    if flags & 0x4 != 0 {
        status |= 0x2;
    }
    if flags & 0x2000 != 0 {
        status |= 0x1;
    }
    if flags & 0x100 != 0 {
        status |= 0x100;
    }
    if flags & 0x400 != 0 {
        status |= 0x80;
    }
    if flags & 0x1000 != 0 {
        status |= 0x1_0000;
    }
    status
}

/// Protocol id written into the queued reply.
///
/// PDU bit 0 rewrites channel ids `6` and `0x10000` to `5`, and ids `0x8005`,
/// `0x8007`, and `0x8010` to `0x8004`.
#[must_use]
pub fn reply_protocol(protocol: u32, flags: u32) -> u32 {
    if flags & 1 == 0 {
        return protocol;
    }
    match protocol {
        6 | 0x10000 => 5,
        0x8005 | 0x8007 | 0x8010 => 0x8004,
        other => other,
    }
}

/// How many data bytes are left after the read path drops a trailing check.
///
/// `strip_uart` is channel flag `0x200` being clear. [`None`] is the UART case
/// whose subtraction leaves 0: `FUN_10006070` returns 0 and queues nothing.
#[must_use]
pub fn delivered_len(protocol: u32, length: u16, strip_uart: bool) -> Option<u16> {
    let uart = matches!(
        protocol,
        3 | 4 | 0x8002 | 0x8003 | 0x800b | 0x10003 | 0x8000_0001 | 0x8000_0002
    );
    let drop = match protocol {
        1 | 0x8000 => u16::from(length > 0),
        2 | 0x8001 => u16::from(length >= 2) * 2,
        _ if uart && strip_uart && length > 0 => 1,
        _ => 0,
    };
    let left = length - drop;
    if uart && strip_uart && left == 0 && length > 0 {
        None
    } else {
        Some(left)
    }
}

const SCALE_500: u32 = 500;
const SCALE_1000: u32 = 1000;
const SCALE_2000: u32 = 2000;

/// Channel fields `FUN_10004c10` and `FUN_10005840` read.
///
/// [`ChannelFields::new`] sets every field except [`baud`](Self::baud) to 0.
/// Those zeros are empty caller fields, not a second set of device defaults.
/// [`ChannelFields::apply_connect_defaults`] writes the defaults that were
/// recovered. The store at `+0xa8` is a dword, so [`at_0xa9`](Self::at_0xa9)
/// is cleared with it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelFields {
    /// The caller's baud, channel `+0x14`.
    pub baud: u32,
    /// Channel `+0x18`.
    pub at_0x18: u32,
    /// Channel `+0x1c`.
    pub at_0x1c: u32,
    /// Channel `+0x28`.
    pub at_0x28: u32,
    /// Channel `+0x30`.
    pub at_0x30: u32,
    /// Channel `+0x34`.
    pub at_0x34: u32,
    /// Channel `+0x3c`.
    pub at_0x3c: u32,
    /// Channel `+0x48`.
    pub at_0x48: u32,
    /// Channel `+0x58`.
    pub at_0x58: u32,
    /// Channel `+0x5c`.
    pub at_0x5c: u32,
    /// Channel `+0x60`.
    pub at_0x60: u32,
    /// Channel `+0x64`.
    pub at_0x64: u32,
    /// Channel `+0x84`.
    pub at_0x84: u32,
    /// Channel `+0x88`.
    pub at_0x88: u32,
    /// Channel `+0x8c`.
    pub at_0x8c: u32,
    /// Channel `+0x94`. A value of `0xFFFF` is stored as 0.
    pub at_0x94: u32,
    /// Channel `+0x98`. A value of `0xFFFF` is stored as 0.
    pub at_0x98: u32,
    /// Channel `+0xa8`.
    pub at_0xa8: u8,
    /// Channel `+0xa9`. The dword store at `+0xa8` clears it.
    pub at_0xa9: u8,
    /// Channel `+0xd8`.
    pub at_0xd8: u32,
    /// Channel `+0xdc`.
    pub at_0xdc: u32,
    /// Channel `+0xe0`.
    pub at_0xe0: u32,
    /// Channel `+0xe4`.
    pub at_0xe4: u32,
    /// Channel `+0xe8`.
    pub at_0xe8: u32,
    /// Channel `+0xf4`.
    pub at_0xf4: u32,
    /// Channel `+0xf8`.
    pub at_0xf8: u32,
    /// Channel `+0xfc`.
    pub at_0xfc: u32,
    /// Channel `+0x11c`. The `0x8405` record uses it and sits outside the count.
    pub at_0x11c: u32,
    /// Channel `+0x13c`.
    pub at_0x13c: u32,
    /// Channel `+0x140`.
    pub at_0x140: u32,
    /// Channel `+0x14c`. The config record uses its low byte.
    pub at_0x14c: u32,
    /// Channel `+0x154`. A nonzero value adds the tenth ISO 15765 record.
    pub at_0x154: u32,
}

impl ChannelFields {
    /// Every field is 0 except the baud.
    #[must_use]
    pub fn new(baud: u32) -> Self {
        Self {
            baud,
            at_0x18: 0,
            at_0x1c: 0,
            at_0x28: 0,
            at_0x30: 0,
            at_0x34: 0,
            at_0x3c: 0,
            at_0x48: 0,
            at_0x58: 0,
            at_0x5c: 0,
            at_0x60: 0,
            at_0x64: 0,
            at_0x84: 0,
            at_0x88: 0,
            at_0x8c: 0,
            at_0x94: 0,
            at_0x98: 0,
            at_0xa8: 0,
            at_0xa9: 0,
            at_0xd8: 0,
            at_0xdc: 0,
            at_0xe0: 0,
            at_0xe4: 0,
            at_0xe8: 0,
            at_0xf4: 0,
            at_0xf8: 0,
            at_0xfc: 0,
            at_0x11c: 0,
            at_0x13c: 0,
            at_0x140: 0,
            at_0x14c: 0,
            at_0x154: 0,
        }
    }

    /// Writes the defaults `FUN_10004c10` stores before `SET_CONFIG`.
    ///
    /// The dword at `+0xa8` is 0, so `+0xa9` is 0 as well.
    pub fn apply_connect_defaults(&mut self, protocol: u32) {
        self.at_0x18 = 0;
        self.at_0x1c = 0;
        self.at_0x84 = 0;
        self.at_0x88 = 0;
        self.at_0x8c = 0;
        self.at_0x94 = 0;
        self.at_0x98 = 0;
        self.at_0xa8 = 0;
        self.at_0xa9 = 0;
        self.at_0x11c = 0x14;
        self.at_0x13c = 100;
        self.at_0x140 = 0;
        self.at_0x14c = 0;
        self.at_0x154 = 0;
        self.at_0x28 = 0x28;
        self.at_0x30 = 0x64;
        self.at_0x34 = match protocol {
            6 | 0x8005 | 0x8007 | 0x8010 | 0x10000 | 0x9280 | 0x9380 | 0x9500 => 0x14,
            1 | 2 | 0x8000 | 0x8001 | 0x9080 | 0x9100 => 0,
            _ => 0x6e,
        };
        self.at_0x3c = 0x0a;
        self.at_0x48 = 0x14;
        self.at_0x58 = 0x12c;
        self.at_0x5c = 0x19;
        self.at_0x60 = 0x32;
        self.at_0x64 = 0;
        self.at_0xd8 = 0x0a;
        self.at_0xdc = 0x190;
        self.at_0xe0 = 0xc8;
        self.at_0xe4 = 0xc8;
        self.at_0xe8 = 1;
        self.at_0xf4 = 1;
        self.at_0xf8 = 0x28;
        self.at_0xfc = 1;
    }
}

fn rec(key: u16, value: u32) -> ComRecord {
    ComRecord { key, value }
}

fn high_byte(byte: u8) -> u32 {
    u32::from(byte) << 24
}

fn pin_pair(byte2: u8, byte3: u8) -> u32 {
    u32::from(byte2) << 16 | u32::from(byte3) << 24
}

fn phase(protocol: u32, fields: &ChannelFields) -> u32 {
    if u16::from_le_bytes([fields.at_0xa8, fields.at_0xa9]) == 0x060e {
        return 0;
    }
    match protocol {
        0x8004 | 0x8005 => 1,
        0x8008 | 0x8007 => 2,
        0x800f | 0x8010 => 3,
        _ => 0,
    }
}

fn id_two(protocol: u32, fields: &ChannelFields) -> u32 {
    match protocol {
        1 => high_byte(2),
        2 => pin_pair(0x0a, 0x02),
        3 | 4 => high_byte(7),
        0x8000 | 0x8002 | 0x8003 | 0x800a | 0x800b | 0x10003 | 0x10004 => high_byte(fields.at_0xa9),
        _ => pin_pair(fields.at_0xa8, fields.at_0xa9),
    }
}

fn clear_ffff(value: u32) -> u32 {
    if value == 0xffff { 0 } else { value }
}

fn scaled(value: u32, factor: u32) -> u32 {
    value.wrapping_mul(factor)
}

fn uart_timing(fields: &ChannelFields) -> [ComRecord; 8] {
    [
        rec(0x41, scaled(fields.at_0x28, SCALE_500)),
        rec(0x42, scaled(fields.at_0x30, SCALE_500)),
        rec(0x43, scaled(fields.at_0x34, SCALE_500)),
        rec(0x44, scaled(fields.at_0x3c, SCALE_500)),
        rec(0x45, scaled(fields.at_0x58, SCALE_1000)),
        rec(0x47, scaled(fields.at_0x5c, SCALE_1000)),
        rec(0x46, scaled(fields.at_0x60, SCALE_1000)),
        rec(0x4a, scaled(fields.at_0x48, SCALE_2000)),
    ]
}

fn pack_400(fields: &ChannelFields) -> u32 {
    let third = fields.at_0x64 & 0xff;
    let fourth = if fields.at_0x64 == 0 || fields.at_0x8c != 0 {
        8
    } else {
        9
    };
    0x01 | (0x01 << 8) | (third << 16) | (fourth << 24)
}

fn iso9141(protocol: u32, fields: &ChannelFields) -> Vec<ComRecord> {
    let mut records = vec![
        baud_record(fields.baud),
        rec(2, id_two(protocol, fields)),
        rec(0x401, 0x0100_0002),
    ];
    records.extend(uart_timing(fields));
    records.push(rec(0x400, pack_400(fields)));
    records
}

fn iso15765(protocol: u32, fields: &ChannelFields) -> Vec<ComRecord> {
    let low = fields.at_0x14c & 0xff;
    let mut records = vec![
        baud_record(fields.baud),
        rec(2, id_two(protocol, fields)),
        rec(0x100, phase(protocol, fields)),
        rec(0x104, (low << 16) | (1 << 24)),
        rec(0x8005, fields.at_0x84),
        rec(0x8006, fields.at_0x88),
        rec(0x8007, clear_ffff(fields.at_0x94)),
        rec(0x8008, clear_ffff(fields.at_0x98)),
        rec(0x23, 4),
    ];
    if fields.at_0x154 != 0 {
        records.push(rec(0x8001, 2));
    }
    records
}

fn tp20(fields: &ChannelFields) -> Result<Vec<ComRecord>, ForwardError> {
    if u16::from_le_bytes([fields.at_0xa8, fields.at_0xa9]) != 0x060e {
        return Err(ForwardError::Unrecovered);
    }
    Ok(vec![
        baud_record(fields.baud),
        rec(2, pin_pair(fields.at_0xa8, fields.at_0xa9)),
        rec(0x100, 0),
        rec(0x8401, scaled(fields.at_0x13c, SCALE_1000)),
        rec(0x8403, scaled(fields.at_0x140, SCALE_1000)),
    ])
}

/// The `SetComParam` records `FUN_10005840` builds.
///
/// Protocol `0x800E` stores its phase record only when the pin word at `+0xa8`
/// is `0x060E`. Otherwise that dword is left as it was, and this returns
/// [`ForwardError::Unrecovered`]. The `0x8405` record is filled after the count
/// and is not returned.
///
/// # Errors
///
/// Returns [`ForwardError::KeptBuffer`] when the pass-through has no builder
/// for `protocol`, and [`ForwardError::Unrecovered`] for an `0x800E` phase that
/// the builder does not store.
pub fn config_records(
    protocol: u32,
    fields: &ChannelFields,
) -> Result<Vec<ComRecord>, ForwardError> {
    let two = id_two(protocol, fields);
    Ok(match protocol {
        1 | 0x8000 => vec![
            baud_record(fields.baud),
            rec(2, two),
            rec(0x22, fields.at_0x18),
            rec(0x21, 0),
        ],
        2 | 0x8001 => vec![
            baud_record(fields.baud),
            rec(2, two),
            rec(0x22, fields.at_0x18),
            rec(0x21, 0),
            rec(0x8501, fields.at_0x1c),
        ],
        3 | 0x8002 | 4 | 0x8003 => iso9141(protocol, fields),
        5 | 0x8004 | 0x8008 | 0x800f => vec![
            baud_record(fields.baud),
            rec(2, two),
            rec(0x100, phase(protocol, fields)),
        ],
        6 | 0x8005 | 0x8007 | 0x8010 | 0x10000 => iso15765(protocol, fields),
        7..=10 => vec![baud_record(fields.baud)],
        0x800a | 0x10004 => vec![
            baud_record(fields.baud),
            rec(2, two),
            rec(0x401, 0x0100_0002),
            rec(0x45, scaled(fields.at_0xd8, SCALE_1000)),
            rec(0x49, scaled(fields.at_0xdc, SCALE_500)),
            rec(0x4a, scaled(fields.at_0xe0, SCALE_500)),
            rec(0x4b, scaled(fields.at_0xe4, SCALE_500)),
            rec(0x4c, scaled(fields.at_0xe8, SCALE_1000)),
            rec(0x9201, scaled(fields.at_0xf4, SCALE_1000)),
            rec(0x9202, scaled(fields.at_0xf8, SCALE_1000)),
            rec(0x43, scaled(fields.at_0xfc, SCALE_1000)),
        ],
        0x800b | 0x10003 => {
            let mut records = vec![
                baud_record(fields.baud),
                rec(2, two),
                rec(0x401, 0x0100_0001),
            ];
            records.extend(uart_timing(fields).into_iter().take(7));
            records
        }
        0x800e => return tp20(fields),
        _ => return Err(ForwardError::KeptBuffer),
    })
}

/// The one-record lookup buffer. Clear and delete pass 0. Add passes the caller's byte.
#[must_use]
pub fn lookup_record(byte: u8) -> ComRecord {
    rec(0x8502, u32::from(byte))
}

/// The caller's filter copy, after the one byte-0 change `FUN_10007410` makes.
pub struct FilterCopy<'a> {
    /// The byte stored at buffer `+4`. The returned id is this plus 1.
    pub slot: u8,
    /// Filter type `1`, `2`, or `3`.
    pub kind: u8,
    /// Pattern bytes. The same length is required of the mask.
    pub pattern: &'a [u8],
    /// Mask bytes.
    pub mask: &'a [u8],
    /// Flow-control bytes. Used when `kind` is `3`.
    pub flow: &'a [u8],
    /// Mask flag `0x100` is set, so pattern byte 0 gains bit `0x80`.
    pub extend_first: bool,
}

/// The `AddFilter` record for one `PassThruStartMsgFilter`.
///
/// # Errors
///
/// Returns [`ForwardError::BadFilter`] when `kind` is not 1, 2, or 3, or a
/// copy is longer than 12 bytes. Returns [`ForwardError::ShortCopy`] when the
/// mask, or the type-3 flow control, is a different length from the pattern.
pub fn start_filter(copy: &FilterCopy<'_>) -> Result<FilterRecord, ForwardError> {
    if !matches!(copy.kind, 1..=3) || copy.pattern.len() > 12 {
        return Err(ForwardError::BadFilter);
    }
    if copy.mask.len() != copy.pattern.len()
        || (copy.kind == 3 && copy.flow.len() != copy.pattern.len())
    {
        return Err(ForwardError::ShortCopy);
    }
    let mut pattern = copy.pattern.to_vec();
    if copy.extend_first {
        if let Some(first) = pattern.first_mut() {
            *first |= 0x80;
        }
    }
    let selector = if copy.kind == 3 { 2 } else { 1 };
    let Ok(at4) = u8::try_from(selector) else {
        return Err(ForwardError::BadFilter);
    };
    let Ok(at12) = u8::try_from(copy.pattern.len()) else {
        return Err(ForwardError::BadFilter);
    };
    Ok(FilterRecord {
        at0: copy.slot,
        at4,
        at8: match copy.kind {
            1 => 1,
            2 => 0,
            _ => 2,
        },
        at12,
        selector,
        from_0x0d: pattern,
        from_0x19: copy.mask.to_vec(),
        from_0x25: if copy.kind == 3 {
            copy.flow.to_vec()
        } else {
            Vec::new()
        },
    })
}

/// The `AddFilter` record `PassThruStopMsgFilter` sends. The length byte is 0.
#[must_use]
pub fn stop_filter(slot: u8) -> FilterRecord {
    FilterRecord {
        at0: slot,
        at4: 1,
        at8: 1,
        at12: 0,
        selector: 1,
        from_0x0d: Vec::new(),
        from_0x19: Vec::new(),
        from_0x25: Vec::new(),
    }
}

/// PDU flags `FUN_10005df0` sets from the caller's flag word.
///
/// # Errors
///
/// Returns [`ForwardError::UnknownProtocol`] when the rewrite for `protocol`
/// was not recovered.
pub fn pdu_flags(protocol: u32, tx_flags: u32) -> Result<u32, ForwardError> {
    let mut flags = 0;
    match protocol {
        5 | 6 | 0x8004 | 0x8005 | 0x8007 | 0x8008 | 0x800f | 0x8010 | 0x10000 => {
            if tx_flags & 0x100 != 0 {
                flags |= 0x100;
            }
            if tx_flags & 0x40 != 0 {
                flags |= 0x200;
            }
            if tx_flags & 0x80 != 0 {
                flags |= 0x400;
            }
            if tx_flags & 0x400 != 0 {
                flags |= 0x1000;
            }
        }
        0x800e => {
            if tx_flags & 0x1_0000 != 0 {
                flags |= 0x20;
            }
            if tx_flags & 0x100 != 0 {
                flags |= 0x100;
            }
        }
        1
        | 2
        | 3
        | 4
        | 7..=10
        | 0x8000
        | 0x8001
        | 0x8002
        | 0x8003
        | 0x800a
        | 0x800b
        | 0x10003
        | 0x10004
        | 0x8000_0001
        | 0x8000_0002 => {}
        _ => return Err(ForwardError::UnknownProtocol),
    }
    Ok(flags)
}

/// The extra PDU bit `FUN_10005df0` sets when it logs `CAN_MIXED_FORMAT`.
///
/// `enabled` is channel `+0xa4` being nonzero. The channel id must be one of
/// `6`, `0x8005`, `0x9280`, `0x8007`, `0x9380`, `0x8010`, `0x9500`, `0x10000`,
/// and the message id one of `5`, `0x8004`, `0x9000`, `0x8008`, `0x9300`,
/// `0x800f`, `0x9480`. A matching pair of ids adds nothing.
///
/// # Errors
///
/// Returns [`ForwardError::ProtocolMismatch`] when the ids differ and the
/// mixed path does not apply. The pass-through then returns `0x15`.
pub fn mixed_bit(channel: u32, message: u32, enabled: bool) -> Result<u32, ForwardError> {
    if channel == message {
        return Ok(0);
    }
    let channel_ok = matches!(
        channel,
        6 | 0x8005 | 0x9280 | 0x8007 | 0x9380 | 0x8010 | 0x9500 | 0x10000
    );
    let message_ok = matches!(
        message,
        5 | 0x8004 | 0x9000 | 0x8008 | 0x9300 | 0x800f | 0x9480
    );
    if enabled && channel_ok && message_ok {
        Ok(1)
    } else {
        Err(ForwardError::ProtocolMismatch)
    }
}

/// Dword `READ_VBATT` writes before `VCX_DEV_GetPinVol` replaces it.
pub const BATTERY_SEED: u32 = 14_000;

/// Middle argument of that `VCX_DEV_GetPinVol` call.
pub const BATTERY_QUERY: u32 = 0x10;

/// The dword `READ_VBATT` keeps. `None` is the device call failing, which stores 0.
#[must_use]
pub fn battery_reading(device_dword: Option<u32>) -> u32 {
    device_dword.unwrap_or(0)
}

/// Timeout both init helpers pass to `VCX_PRO_Send`.
pub const INIT_WAIT: u32 = 0x3e8;

/// The `FIVE_BAUD_INIT` payload. Flags are 8, the length is 1, and `byte` is the data.
///
/// The pass-through reads `byte` from the pointer at input `+4`. A null channel,
/// output, or that pointer returns 4 and does not build this payload.
#[must_use]
pub fn five_baud_payload(byte: u8) -> [u8; 7] {
    let mut payload = [0; 7];
    payload[..4].copy_from_slice(&8_u32.to_be_bytes());
    payload[5] = 1;
    payload[6] = byte;
    payload
}

/// The `FAST_INIT` payload. The normal UART copy is sent with PDU flag `0x4` set.
///
/// Accepted protocols are `3`, `4`, `0x8002`, `0x8003`, `0x10003`, and `0x800b`.
/// A protocol outside that set returns 1. A message protocol that differs from
/// the channel returns `0x0A`.
///
/// # Errors
///
/// Returns [`ForwardError::FastStatus`] for those two refusals, and the errors
/// of [`pdu_flags`] and [`copied_data`].
pub fn fast_init(
    channel: u32,
    message: u32,
    tx_flags: u32,
    data: &[u8],
    strip_uart: bool,
) -> Result<Vec<u8>, ForwardError> {
    if channel != message {
        return Err(ForwardError::FastStatus(0x0a));
    }
    if !matches!(channel, 3 | 4 | 0x8002 | 0x8003 | 0x800b | 0x10003) {
        return Err(ForwardError::FastStatus(1));
    }
    let flags = pdu_flags(channel, tx_flags)? | 4;
    let payload = copied_data(channel, data, strip_uart)?;
    send_payload(flags, &payload)
}

/// Bytes `FUN_10005df0` places at PDU offset 6.
///
/// `strip_uart` means channel flag `0x200` is clear, so the UART sum is appended.
/// J1850 appends [`j1850_check`]. Protocol `0x800E` copies from byte 4.
///
/// # Errors
///
/// Returns [`ForwardError::TooLong`] above [`MAX_FORWARD`],
/// [`ForwardError::ShortCopy`] when an `0x800E` copy is shorter than 4 bytes,
/// and [`ForwardError::UnknownProtocol`] when the rewrite was not recovered.
pub fn copied_data(protocol: u32, data: &[u8], strip_uart: bool) -> Result<Vec<u8>, ForwardError> {
    if data.len() > MAX_FORWARD {
        return Err(ForwardError::TooLong);
    }
    let uart = matches!(
        protocol,
        3 | 4 | 0x8002 | 0x8003 | 0x800b | 0x10003 | 0x8000_0001 | 0x8000_0002
    );
    match protocol {
        1 | 2 | 0x8000 | 0x8001 => {
            let mut out = data.to_vec();
            out.push(j1850_check(data));
            Ok(out)
        }
        _ if uart => {
            let mut out = data.to_vec();
            if strip_uart {
                out.push(uart_sum(data));
            }
            Ok(out)
        }
        0x800e => {
            let Some(tail) = data.get(4..) else {
                return Err(ForwardError::ShortCopy);
            };
            Ok(tail.to_vec())
        }
        5
        | 6
        | 7..=10
        | 0x8004
        | 0x8005
        | 0x8007
        | 0x8008
        | 0x800a
        | 0x800f
        | 0x8010
        | 0x10000
        | 0x10004 => Ok(data.to_vec()),
        _ => Err(ForwardError::UnknownProtocol),
    }
}

/// One periodic message, before `VCX_PRO_AddTimer` packs it.
pub struct Periodic<'a> {
    /// Timer slot. The returned id is this plus 1.
    pub slot: u8,
    /// `ulTimeInterval`. The record stores it times `0x3E8`.
    pub interval: u32,
    /// Channel protocol id. Selects the PDU rewrite.
    pub protocol: u32,
    /// Caller flag word. [`pdu_flags`] maps it.
    pub tx_flags: u32,
    /// Caller data. Longer than 12 bytes is refused.
    pub data: &'a [u8],
    /// Present when `ExtraDataIndex` is at least `DataSize`. Its length must match `data`.
    pub extra: Option<&'a [u8]>,
    /// Channel flag `0x200` is clear, so a UART sum is appended.
    pub strip_uart: bool,
}

/// The `AddTimer` record `FUN_10007780` fills.
///
/// The first extra is the byte-swapped PDU flags followed by the copied data.
/// A present `extra` clears bit `0x40` of the `0xC9` mark.
///
/// # Errors
///
/// Returns [`ForwardError::BadFilter`] when `data` is longer than 12 bytes,
/// [`ForwardError::ShortCopy`] when `extra` is a different length, and the
/// errors of [`copied_data`] and [`pdu_flags`].
pub fn periodic_timer(message: &Periodic<'_>) -> Result<TimerRecord, ForwardError> {
    if message.data.len() > 12 {
        return Err(ForwardError::BadFilter);
    }
    let payload = copied_data(message.protocol, message.data, message.strip_uart)?;
    let flags = pdu_flags(message.protocol, message.tx_flags)?;
    let Ok(span) = u8::try_from(payload.len() + 4) else {
        return Err(ForwardError::BadFilter);
    };
    let (mark, tail, tail_len) = if let Some(extra) = message.extra {
        if extra.len() != message.data.len() {
            return Err(ForwardError::ShortCopy);
        }
        let Ok(tail_len) = u8::try_from(message.data.len()) else {
            return Err(ForwardError::BadFilter);
        };
        (0x89, extra.to_vec(), tail_len)
    } else {
        (0xc9, Vec::new(), 0)
    };
    let mut from_8 = flags.to_be_bytes().to_vec();
    from_8.extend_from_slice(&payload);
    Ok(TimerRecord {
        first: [message.slot, mark, span, tail_len],
        dword: message.interval.wrapping_mul(0x3e8).to_le_bytes(),
        from_8,
        from_0x18: tail,
    })
}

/// Id `0x08018A8E` stores from the dword at record offset `0x14`.
///
/// Bits 24..28 become bits 0..4. Bits 29..31 are left out. Bits 16..23 move
/// to bits 8..15, bits 8..15 move to bits 16..23, and bits 0..7 move to bits
/// 24..31.
#[must_use]
pub fn wire_id(value: u32) -> u32 {
    ((value >> 24) & 0x1f) | ((value >> 8) & 0xff00) | ((value << 8) & 0x00ff_0000) | (value << 24)
}

/// Header `0x08018A6A` writes for record kinds 1, 2, and 5.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WiredCan {
    /// Flag word. Kind 2 sets bit 2. Bit 31 of the id sets `0x100`. A non-zero
    /// extra byte sets `0x400`.
    pub flags: u32,
    /// Low 16 bits of the length the device stores.
    pub length: u16,
    /// [`wire_id`] of the record dword.
    pub id: u32,
    /// Byte at record offset `0x19`, present when the extra byte is not 0.
    pub mark: Option<u8>,
    /// Value stored back into the kind byte.
    pub kind_after: u8,
}

/// What `0x08018A6A` keeps of one queued record.
///
/// The kind byte is at offset 0. Kind 2 continues only when `gate` is set,
/// which is bit 2 of the byte at `r6 + 0x0C`. It then sets flag `4`, stores
/// `extra + 4`, and writes kind 3. Kinds 1 and 5 store `extra + count + 4`
/// and write kind 0. Any other kind returns `None`. Kind 5 also calls
/// `0x08011D7A`. That address is the tail of the copy at `0x08011CD8`, and
/// the carry flag is clear, so the call stores nothing.
#[must_use]
pub fn wire_can(
    kind: u8,
    id: u32,
    extra: u8,
    mark: u8,
    count: u32,
    gate: bool,
) -> Option<WiredCan> {
    if kind == 2 && !gate {
        return None;
    }
    let (length, kind_after, mut flags) = match kind {
        2 => (u32::from(extra) + 4, 3, 4),
        1 | 5 => (u32::from(extra) + count + 4, 0, 0),
        _ => return None,
    };
    if id & 0x8000_0000 != 0 {
        flags |= 0x100;
    }
    let kept = if extra == 0 {
        None
    } else {
        flags |= 0x400;
        Some(mark)
    };
    let bytes = length.to_le_bytes();
    Some(WiredCan {
        flags,
        length: u16::from_le_bytes([bytes[0], bytes[1]]),
        id: wire_id(id),
        mark: kept,
        kind_after,
    })
}

/// Object offsets `ProcessQueuedCan` (`0x080188B0`) reads.
///
/// The record starts at [`record`](QueuedMap::record). The header is the
/// second argument: flags at byte 0, the length halfword at byte 4, and the
/// pointer to the shuffled id at byte 8.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueuedMap {
    /// Start of the record. The kind byte is here.
    pub record: usize,
    /// Byte whose bit 2 lets kind 2 continue.
    pub gate: usize,
    /// Dword whose low byte is copied to `record + 1`.
    pub copied_a: usize,
    /// Dword whose low byte is copied to `record + 2`.
    pub copied_b: usize,
    /// Dword copied to `record + 0x10`.
    pub prior: usize,
    /// Count dword. It is `record + 0x0C`.
    pub count: usize,
    /// Id dword. It is `record + 0x14`.
    pub id: usize,
    /// Extra byte. It is `record + 0x18`.
    pub extra: usize,
    /// Byte kept when [`extra`](QueuedMap::extra) is not 0. It is `record + 0x19`.
    pub mark: usize,
    /// Dword kind 5 sets to 0. It is `record + 0x1C`.
    pub held: usize,
    /// Header offset of the flag dword.
    pub header_flags: usize,
    /// Header offset of the length halfword.
    pub header_length: usize,
    /// Header offset of the pointer that receives the shuffled id.
    pub header_bytes: usize,
}

/// Offsets of the queued record and the header `0x080188B0` fills.
#[must_use]
pub fn queued_map() -> QueuedMap {
    let record = 0xC68;
    QueuedMap {
        record,
        gate: 0xC2C,
        copied_a: 0xC50,
        copied_b: 0xC54,
        prior: 0xC3C,
        count: record + 0x0C,
        id: record + 0x14,
        extra: record + 0x18,
        mark: record + 0x19,
        held: record + 0x1C,
        header_flags: 0,
        header_length: 4,
        header_bytes: 8,
    }
}

/// Fields `0x0802DD94` writes into the destination record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Staged {
    /// Byte stored at offset 0. It is 3.
    pub kind: u8,
    /// Halfword stored at offset 2. It is the source byte at offset `0x0C`.
    pub half: u16,
    /// Dword stored at offset `0x18`. It is the source dword at offset 0.
    pub dword: u32,
}

/// What `0x0802DD94` stores after it logs the dword at destination offset `0x10`.
///
/// The source dword is written at offset `0x18`. The source byte at offset
/// `0x0C` is written as a halfword at offset 2. The kind byte is then set to
/// 3. The call that follows enters `0x08011DE8` at `0x08011E22`. The mix
/// matches XXTEA, but the routine's own load is `0xF7FFA80B`, so those source
/// bytes are not decrypted here.
#[must_use]
pub fn stage_record(dword: u32, mark: u8) -> Staged {
    Staged {
        kind: 3,
        half: u16::from(mark),
        dword,
    }
}

/// Slots in the queue object `0x08019466` indexes.
pub const QUEUE_SLOTS: u8 = 10;

/// Bytes from one slot to the next. The first slot is at object offset `0xA5C`.
pub const QUEUE_STRIDE: usize = 0x2C;

/// Object offset of slot 0. It is `0xA50 + 0x0C`.
pub const QUEUE_SLOT_BASE: usize = 0xA5C;

/// Where one record sits, and whether the later update runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueuePlace {
    /// First byte of the record after `byte % 10`.
    pub index: u8,
    /// Offset of that slot from the start of the queue object.
    pub offset: usize,
    /// False when the byte at offset 2 is `0x11` or greater. The index is still
    /// reduced. The occupancy word and the rest of the update are skipped.
    pub kept: bool,
}

/// Slot `0x08019466` picks from the record's first byte and the byte at offset 2.
///
/// The index is that first byte modulo 10, and it is written back. The slot is
/// `0xA5C + index * 0x2C`. A byte at offset 2 of `0x11` or more skips the update.
#[must_use]
pub fn place_queue(first: u8, at_2: u8) -> QueuePlace {
    let index = first % QUEUE_SLOTS;
    QueuePlace {
        index,
        offset: QUEUE_SLOT_BASE + usize::from(index) * QUEUE_STRIDE,
        kept: at_2 < 0x11,
    }
}

/// The `AddTimer` record `FUN_10007a10` sends for one stopped id.
///
/// The stored slot is `id - 1`, wrapping the way a byte subtract does.
/// `SetTimer` is not called.
#[must_use]
pub fn stop_timer(id: u8) -> TimerRecord {
    TimerRecord {
        first: [id.wrapping_sub(1), 0, 0, 0],
        dword: [0; 4],
        from_8: Vec::new(),
        from_0x18: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BATTERY_SEED, ChannelFields, ComRecord, FILTER_CLEAR, FILTER_PASS, FilterCopy,
        FilterRecord, ForwardError, INIT_WAIT, Periodic, TIMER_CLEAR, TIMER_PASS, TimerRecord,
        add_filter, add_timer, battery_reading, baud_record, close_payload, config_records,
        copied_data, delivered_len, fast_init, five_baud_payload, j1850_check, lookup_record,
        mixed_bit, open_code, open_payload, pdu_flags, periodic_timer, place_queue, queued_map,
        reply_protocol, rx_status, send_payload, set_com_param, set_filter_payload,
        set_timer_payload, stage_record, start_filter, start_payload, stop_filter, stop_payload,
        stop_timer, uart_sum, wire_can, wire_id,
    };

    #[test]
    fn a_29_bit_id_keeps_five_bits_of_its_top_byte() {
        assert_eq!(wire_id(0x9abc_def0), 0xf0de_bc1a);
        let wired = wire_can(2, 0x8000_0001, 1, 0x22, 0, true).unwrap();
        assert_eq!(wired.flags, 4 | 0x100 | 0x400);
        assert_eq!(wired.id, 0x0100_0000);
        assert_eq!(wired.length, 5);
        assert_eq!(wired.mark, Some(0x22));
        assert_eq!(wired.kind_after, 3);
        assert_eq!(wire_can(2, 1, 0, 0, 0, false), None);
        let other = wire_can(1, 0x11, 0, 0x99, 6, false).unwrap();
        assert_eq!(other.flags, 0);
        assert_eq!(other.length, 10);
        assert_eq!(other.mark, None);
        assert_eq!(other.kind_after, 0);
        assert_eq!(wire_can(5, 0, 2, 0, 1, false).unwrap().length, 7);
        assert_eq!(wire_can(1, 0, 0, 0, 0x1_0000, false).unwrap().length, 4);
        assert_eq!(wire_can(0, 0, 0, 0, 0, true), None);
    }

    #[test]
    fn a_staged_record_keeps_the_dword_and_the_mark_byte() {
        let staged = stage_record(0x1122_3344, 0x0a);
        assert_eq!(staged.kind, 3);
        assert_eq!(staged.half, 0x0a);
        assert_eq!(staged.dword, 0x1122_3344);
    }

    #[test]
    fn the_queued_record_starts_at_object_0xc68() {
        let map = queued_map();
        assert_eq!(map.record, 0xC68);
        assert_eq!(map.gate, 0xC2C);
        assert_eq!(map.id, 0xC7C);
        assert_eq!(map.count, 0xC74);
        assert_eq!(map.held, 0xC84);
        assert_eq!(map.prior, 0xC3C);
        assert_eq!(map.header_bytes, 8);
    }

    #[test]
    fn a_queue_index_is_the_first_byte_modulo_ten() {
        let placed = place_queue(25, 0x10);
        assert_eq!(placed.index, 5);
        assert_eq!(placed.offset, 0xA5C + 5 * 0x2C);
        assert!(placed.kept);
        let skipped = place_queue(10, 0x11);
        assert_eq!(skipped.index, 0);
        assert_eq!(skipped.offset, 0xA5C);
        assert!(!skipped.kept);
    }

    #[test]
    fn iso15765_opens_as_0x8001() {
        assert_eq!(open_code(6), Ok(0x8001));
        assert_eq!(open_code(0x10000), Ok(0x8001));
        assert_eq!(open_payload(6), Ok([0x00, 0x00, 0x80, 0x01]));
    }

    #[test]
    fn an_unknown_protocol_is_not_given_a_dword() {
        assert_eq!(open_code(0), Err(ForwardError::UnknownProtocol));
    }

    #[test]
    fn the_send_payload_is_the_flags_the_length_and_the_data() {
        let payload = send_payload(0x100, &[0x22, 0xf1, 0x90]).unwrap();
        assert_eq!(
            payload,
            [0x00, 0x00, 0x01, 0x00, 0x00, 0x03, 0x22, 0xf1, 0x90]
        );
    }

    #[test]
    fn a_copy_past_0x1020_is_refused() {
        let data = vec![0_u8; 0x1021];
        assert_eq!(send_payload(0, &data), Err(ForwardError::TooLong));
    }

    #[test]
    fn the_uart_extra_byte_is_the_sum() {
        assert_eq!(uart_sum(&[0x01, 0x02, 0x03]), 0x06);
    }

    #[test]
    fn the_j1850_extra_byte_matches_the_pass_through() {
        assert_eq!(j1850_check(&[0x68, 0x6a, 0xf1, 0x01, 0x00]), 0x17);
    }

    #[test]
    fn set_com_param_stores_the_word_and_the_dword_big_endian() {
        let payload = set_com_param(&[ComRecord {
            key: 0x3412,
            value: 0xF0DE_BC9A,
        }])
        .unwrap();
        assert_eq!(payload, [0x34, 0x12, 0xF0, 0xDE, 0xBC, 0x9A]);
    }

    #[test]
    fn add_filter_starts_with_the_count_and_copies_three_tails() {
        let payload = add_filter(&[FilterRecord {
            at0: 0x11,
            at4: 0x02,
            at8: 0x22,
            at12: 0x02,
            selector: 2,
            from_0x0d: vec![0x31, 0x32],
            from_0x19: vec![0x41, 0x42],
            from_0x25: vec![0x51, 0x52],
        }])
        .unwrap();
        assert_eq!(
            payload,
            [
                0x01, 0x11, 0x02, 0x22, 0x02, 0x31, 0x32, 0x41, 0x42, 0x51, 0x52
            ]
        );
    }

    #[test]
    fn a_negative_filter_selector_copies_no_tail() {
        let payload = add_filter(&[FilterRecord {
            at0: 0x11,
            at4: 0x02,
            at8: 0x22,
            at12: 0x02,
            selector: -1,
            from_0x0d: vec![],
            from_0x19: vec![],
            from_0x25: vec![],
        }])
        .unwrap();
        assert_eq!(payload, [0x01, 0x11, 0x02, 0x22, 0x02]);
    }

    #[test]
    fn add_timer_keeps_the_dword_and_both_extras() {
        let payload = add_timer(&[TimerRecord {
            first: [0x11, 0x22, 0x02, 0x01],
            dword: [0x78, 0x56, 0x34, 0x12],
            from_8: vec![0xAA, 0xBB],
            from_0x18: vec![0xCC],
        }])
        .unwrap();
        assert_eq!(
            payload,
            [
                0x01, 0x11, 0x22, 0x02, 0x01, 0x78, 0x56, 0x34, 0x12, 0xAA, 0xBB, 0xCC
            ]
        );
    }

    #[test]
    fn connect_stores_baud_as_record_3() {
        let wire = set_com_param(&[baud_record(500_000)]).unwrap();
        assert_eq!(wire, [0x00, 0x03, 0x00, 0x07, 0xA1, 0x20]);
    }

    #[test]
    fn a_break_bit_becomes_rx_status_4() {
        assert_eq!(rx_status(6, 0x2), 0x4);
        assert_eq!(rx_status(6, 0x4), 0x2);
        assert_eq!(rx_status(6, 0x1000), 0x1_0000);
        assert_eq!(rx_status(7, 0x8), 0);
    }

    #[test]
    fn iso15765_keeps_its_bytes_and_j1850_pwm_drops_two() {
        assert_eq!(delivered_len(6, 8, false), Some(8));
        assert_eq!(delivered_len(2, 5, false), Some(3));
        assert_eq!(delivered_len(2, 1, false), Some(1));
        assert_eq!(delivered_len(3, 1, true), None);
        assert_eq!(reply_protocol(6, 1), 5);
        assert_eq!(reply_protocol(6, 0), 6);
    }

    #[test]
    fn iso9141_scales_the_connect_default() {
        let mut fields = ChannelFields::new(10_400);
        fields.apply_connect_defaults(3);
        let records = config_records(3, &fields).unwrap();
        assert_eq!(
            records[3],
            ComRecord {
                key: 0x41,
                value: 0x4e20
            }
        );
        assert_eq!(
            records[1],
            ComRecord {
                key: 2,
                value: 0x0700_0000
            }
        );
        assert_eq!(
            records[2],
            ComRecord {
                key: 0x401,
                value: 0x0100_0002
            }
        );
        assert_eq!(
            records.last(),
            Some(&ComRecord {
                key: 0x400,
                value: 0x0800_0101
            })
        );
    }

    #[test]
    fn iso15765_config_ends_with_record_0x23_value_4() {
        let mut fields = ChannelFields::new(500_000);
        fields.apply_connect_defaults(6);
        let records = config_records(6, &fields).unwrap();
        assert_eq!(
            records.last(),
            Some(&ComRecord {
                key: 0x23,
                value: 4
            })
        );
        fields.at_0x154 = 1;
        let with_extra = config_records(6, &fields).unwrap();
        assert_eq!(
            with_extra.last(),
            Some(&ComRecord {
                key: 0x8001,
                value: 2
            })
        );
        fields.at_0xa8 = 0x0e;
        fields.at_0xa9 = 0x06;
        let forced = config_records(0x8005, &fields).unwrap();
        assert_eq!(
            forced[2],
            ComRecord {
                key: 0x100,
                value: 0
            }
        );
        fields.at_0xa8 = 0;
        fields.at_0xa9 = 0;
        let phase = config_records(0x8005, &fields).unwrap();
        assert_eq!(
            phase[2],
            ComRecord {
                key: 0x100,
                value: 1
            }
        );
        fields.at_0x94 = 0xffff;
        let cleared = config_records(6, &fields).unwrap();
        assert_eq!(
            cleared[6],
            ComRecord {
                key: 0x8007,
                value: 0
            }
        );
    }

    #[test]
    fn tp20_opens_but_its_config_phase_is_not_recovered() {
        assert_eq!(open_code(0x800e), Ok(0x8401));
        let fields = ChannelFields::new(500_000);
        assert_eq!(
            config_records(0x800e, &fields),
            Err(ForwardError::Unrecovered)
        );
        assert_eq!(
            config_records(0x8000_0001, &fields),
            Err(ForwardError::KeptBuffer)
        );
    }

    #[test]
    fn a_tx_flag_0x40_becomes_pdu_0x200() {
        let flags = pdu_flags(6, 0x40).unwrap();
        let data = copied_data(6, &[0x22, 0xf1, 0x90], false).unwrap();
        let payload = send_payload(flags, &data).unwrap();
        assert_eq!(
            payload,
            [0x00, 0x00, 0x02, 0x00, 0x00, 0x03, 0x22, 0xf1, 0x90]
        );
        let j1850 = copied_data(1, &[0x68, 0x6a, 0xf1, 0x01, 0x00], false).unwrap();
        assert_eq!(j1850.last(), Some(&0x17));
        assert_eq!(
            copied_data(3, &[0x01, 0x02, 0x03], true).unwrap(),
            [0x01, 0x02, 0x03, 0x06]
        );
        assert_eq!(
            copied_data(0x800e, &[1, 2, 3, 4, 9, 8], false).unwrap(),
            [9, 8]
        );
        assert_eq!(
            copied_data(0x800e, &[1, 2, 3], false),
            Err(ForwardError::ShortCopy)
        );
    }

    #[test]
    fn a_filter_pattern_gains_0x80_when_the_mask_flag_is_set() {
        let record = start_filter(&FilterCopy {
            slot: 3,
            kind: 1,
            pattern: &[0x11, 0x22],
            mask: &[0xff, 0x00],
            flow: &[],
            extend_first: true,
        })
        .unwrap();
        let payload = add_filter(&[record]).unwrap();
        assert_eq!(
            payload,
            [0x01, 0x03, 0x01, 0x01, 0x02, 0x91, 0x22, 0xff, 0x00]
        );
        let stopped = add_filter(&[stop_filter(3)]).unwrap();
        assert_eq!(stopped, [0x01, 0x03, 0x01, 0x01, 0x00]);
        assert_eq!(lookup_record(0).key, 0x8502);
        assert_eq!(lookup_record(0xab).value, 0xab);
    }

    #[test]
    fn connect_sends_the_one_byte_and_the_empty_payloads() {
        assert_eq!(close_payload(), &[] as &[u8]);
        assert_eq!(stop_payload(), &[] as &[u8]);
        assert_eq!(start_payload(), [0]);
        assert_eq!(set_filter_payload(FILTER_PASS).unwrap(), [1]);
        assert_eq!(set_filter_payload(FILTER_CLEAR).unwrap(), [2]);
        assert_eq!(set_timer_payload(TIMER_PASS).unwrap(), [1]);
        assert_eq!(set_timer_payload(TIMER_CLEAR).unwrap(), [2]);
        assert_eq!(set_filter_payload(0), Err(ForwardError::Mode));
        assert_eq!(set_timer_payload(0), Err(ForwardError::Mode));
    }

    #[test]
    fn tp20_with_pin_word_0x060e_stores_phase_zero() {
        let mut fields = ChannelFields::new(500_000);
        fields.at_0xa9 = 0x11;
        fields.apply_connect_defaults(0x800e);
        assert_eq!(fields.at_0xa9, 0);
        fields.at_0xa8 = 0x0e;
        fields.at_0xa9 = 0x06;
        let records = config_records(0x800e, &fields).unwrap();
        assert_eq!(records.len(), 5);
        assert_eq!(
            records[2],
            ComRecord {
                key: 0x100,
                value: 0
            }
        );
        assert_eq!(
            records[3],
            ComRecord {
                key: 0x8401,
                value: 100_000
            }
        );
    }

    #[test]
    fn a_periodic_iso15765_message_carries_the_swapped_flags() {
        let record = periodic_timer(&Periodic {
            slot: 0,
            interval: 1,
            protocol: 6,
            tx_flags: 0x40,
            data: &[0x22, 0xf1, 0x90],
            extra: None,
            strip_uart: false,
        })
        .unwrap();
        let payload = add_timer(&[record]).unwrap();
        assert_eq!(
            payload,
            [
                0x01, 0x00, 0xc9, 0x07, 0x00, 0xe8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x22,
                0xf1, 0x90,
            ]
        );
        let stopped = add_timer(&[stop_timer(1)]).unwrap();
        assert_eq!(
            stopped,
            [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn a_raw_can_message_on_iso15765_sets_pdu_bit_0() {
        assert_eq!(mixed_bit(6, 5, true), Ok(1));
        assert_eq!(mixed_bit(6, 6, true), Ok(0));
        assert_eq!(mixed_bit(0x10000, 0x800f, true), Ok(1));
        assert_eq!(mixed_bit(6, 5, false), Err(ForwardError::ProtocolMismatch));
        assert_eq!(mixed_bit(1, 5, true), Err(ForwardError::ProtocolMismatch));
        assert_eq!(battery_reading(None), 0);
        assert_eq!(BATTERY_SEED, 14_000);
    }

    #[test]
    fn five_baud_is_one_byte_under_flag_8() {
        assert_eq!(
            five_baud_payload(0x33),
            [0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x33]
        );
        assert_eq!(INIT_WAIT, 0x3e8);
    }

    #[test]
    fn fast_init_sets_flag_4_and_appends_the_uart_sum() {
        let payload = fast_init(3, 3, 0, &[0x01, 0x02, 0x03], true).unwrap();
        assert_eq!(
            payload,
            [0x00, 0x00, 0x00, 0x04, 0x00, 0x04, 0x01, 0x02, 0x03, 0x06]
        );
        assert_eq!(
            fast_init(3, 4, 0, &[0], false),
            Err(ForwardError::FastStatus(0x0a))
        );
        assert_eq!(
            fast_init(6, 6, 0, &[0], false),
            Err(ForwardError::FastStatus(1))
        );
    }
}
