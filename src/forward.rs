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
}

impl std::fmt::Display for ForwardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TooLong => "forwarded data is longer than 0x1020",
            Self::UnknownProtocol => "protocol has no VCX_PRO_Open dword",
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
    use super::{ForwardError, j1850_check, open_code, open_payload, send_payload, uart_sum};

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
}
