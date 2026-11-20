//! Bytes `VCXPT32.dll` copies into a `VCX_PRO_*` payload.
//!
//! `PassThruWriteMsgs` (`0x100039e0`) calls `FUN_10005df0`, which copies the
//! caller's data to PDU offset 6. `VCX_PRO_Send` (`0x7B2F5740`) then stores that
//! dword big-endian, the length big-endian, and those bytes. `FUN_10005a20`
//! is the dword `VCX_PRO_Open` is given. Nothing here opens a port.

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
}

impl std::fmt::Display for ForwardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TooLong => "forwarded data is longer than 0x1020",
            Self::UnknownProtocol => "protocol has no VCX_PRO_Open dword",
            Self::TooMany => "the record count does not fit in one byte",
            Self::ShortCopy => "a tail is shorter than its length byte",
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

#[cfg(test)]
mod tests {
    use super::{
        ComRecord, FilterRecord, ForwardError, TimerRecord, add_filter, add_timer, j1850_check,
        open_code, open_payload, send_payload, set_com_param, uart_sum,
    };

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
}
