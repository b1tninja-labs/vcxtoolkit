//! The profile `VCX_GetCommIf` copies back.
//!
//! The opener at `0x7B2FA680` keeps a link type and `0x114` profile bytes.
//! `VCX_GetCommIf` writes that type as a little-endian dword and then those
//! bytes. Types 0, 1, and 2 read a port, a baud, and a delay. Types 3 through
//! 8 read a mark dword and `0x40` address bytes. This module does not open a
//! link and does not send a frame.

/// Bytes of the profile, after the type dword.
pub const COMM_PROFILE: usize = 0x114;

/// Bytes `VCX_GetCommIf` writes: the type dword plus [`COMM_PROFILE`].
pub const COMM_COPY: usize = 4 + COMM_PROFILE;

/// Port dword for types 0, 1, and 2. Name types store their mark here.
pub const PROFILE_PORT: usize = 0;

/// Baud dword for types 0, 1, and 2.
pub const PROFILE_BAUD: usize = 4;

/// First of the `0x40` address bytes for types 3 through 8.
pub const PROFILE_NAME: usize = 8;

/// Address bytes copied for types 3 through 8.
pub const PROFILE_NAME_LEN: usize = 0x40;

/// Delay dword for types 0, 1, and 2.
pub const PROFILE_DELAY: usize = 0x10C;

/// Baud type 0 uses when the profile baud is 0. This is 921600.
pub const TYPE0_BAUD: u32 = 0x000E_1000;

/// Baud types 1 and 2 use when the profile baud is 0. This is 115200.
pub const TYPE12_BAUD: u32 = 0x0001_C200;

/// Delay types 0 and 1 use when the profile delay is 0.
pub const TYPE01_DELAY: u32 = 0xC8;

/// Delay type 2 uses when the profile delay is 0.
pub const TYPE2_DELAY: u32 = 0x5DC;

/// Byte size the serial working block stores for a new type 0, 1, or 2.
pub const COMM_BYTE_SIZE: u16 = 8;

/// Parity the serial working block stores for a new type 0, 1, or 2.
pub const COMM_PARITY: u8 = 0;

/// Mark stored with `192.168.8.1`, `192.168.171.1`, and `192.168.9.1`.
pub const ADDRESS_MARK: u32 = 0x1AA4;

/// Mark stored with `192.168.4.1`.
pub const ALT_MARK: u32 = 0x17;

/// One address a probe copies into the profile, and the mark beside it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddressWrite {
    /// Bytes copied to profile offset [`PROFILE_NAME`].
    pub address: &'static [u8],
    /// Dword stored at profile offset [`PROFILE_PORT`].
    pub mark: u32,
}

/// Stores from `0x7B2F44D0`, in the order of its checks.
///
/// A positive check makes the opener keep link type 4. The third check copies
/// `192.168.8.1` again.
pub const TYPE_4_WRITES: [AddressWrite; 4] = [
    AddressWrite {
        address: b"192.168.8.1",
        mark: ADDRESS_MARK,
    },
    AddressWrite {
        address: b"192.168.171.1",
        mark: ADDRESS_MARK,
    },
    AddressWrite {
        address: b"192.168.8.1",
        mark: ADDRESS_MARK,
    },
    AddressWrite {
        address: b"192.168.4.1",
        mark: ALT_MARK,
    },
];

/// The store from `0x7B2F4690`. A positive check makes the opener keep link type 8.
pub const TYPE_8_WRITE: AddressWrite = AddressWrite {
    address: b"192.168.9.1",
    mark: ADDRESS_MARK,
};

/// Strings named by the type 1 helper.
pub const TYPE1_NAMES: [&str; 2] = ["Serial", "SerialSTDiscover"];

/// Strings named by the type 2 helper.
pub const TYPE2_NAMES: [&str; 2] = ["Bt", "SerialBTDiscover"];

/// Why a comm buffer was not read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommError {
    /// The buffer is shorter than [`COMM_COPY`].
    Short,
}

impl std::fmt::Display for CommError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("comm buffer is shorter than the copy")
    }
}

impl std::error::Error for CommError {}

/// Type dword and profile `VCX_GetCommIf` writes.
///
/// # Errors
///
/// Returns [`CommError::Short`] when `buffer` is shorter than [`COMM_COPY`].
#[must_use = "a comm profile that is dropped was not read"]
pub fn comm_interface(buffer: &[u8]) -> Result<(u32, &[u8]), CommError> {
    if buffer.len() < COMM_COPY {
        return Err(CommError::Short);
    }
    let kind = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
    Ok((kind, &buffer[4..COMM_COPY]))
}

/// Little-endian dword at `offset` in a profile.
#[must_use]
pub fn profile_dword(profile: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let bytes = profile.get(offset..end)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// The `0x40` address bytes, when the profile contains them.
#[must_use]
pub fn profile_name(profile: &[u8]) -> Option<&[u8]> {
    profile.get(PROFILE_NAME..PROFILE_NAME + PROFILE_NAME_LEN)
}

/// Baud the type 0, 1, or 2 path uses.
///
/// A zero baud becomes [`TYPE0_BAUD`] or [`TYPE12_BAUD`]. Any other link type
/// has no baud on this path.
#[must_use]
pub const fn baud_in_use(kind: u32, baud: u32) -> Option<u32> {
    let default = match kind {
        0 => TYPE0_BAUD,
        1 | 2 => TYPE12_BAUD,
        _ => return None,
    };
    Some(if baud == 0 { default } else { baud })
}

/// Delay the type 0, 1, or 2 path uses.
///
/// A zero delay becomes [`TYPE01_DELAY`] or [`TYPE2_DELAY`]. Any other link
/// type has no delay on this path.
#[must_use]
pub const fn delay_in_use(kind: u32, delay: u32) -> Option<u32> {
    let default = match kind {
        0 | 1 => TYPE01_DELAY,
        2 => TYPE2_DELAY,
        _ => return None,
    };
    Some(if delay == 0 { default } else { delay })
}

/// Byte size and parity stored for a new type 0, 1, or 2.
#[must_use]
pub const fn serial_line(kind: u32) -> Option<(u16, u8)> {
    if kind <= 2 {
        Some((COMM_BYTE_SIZE, COMM_PARITY))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ADDRESS_MARK, ALT_MARK, COMM_COPY, COMM_PROFILE, PROFILE_BAUD, PROFILE_DELAY, PROFILE_NAME,
        PROFILE_PORT, TYPE_4_WRITES, TYPE_8_WRITE, TYPE0_BAUD, TYPE2_DELAY, TYPE12_BAUD,
        baud_in_use, comm_interface, delay_in_use, profile_dword, profile_name, serial_line,
    };

    #[test]
    fn the_copy_is_a_type_and_the_profile() {
        let mut buffer = vec![0_u8; COMM_COPY + 4];
        buffer[0] = 0x04;
        buffer[4] = 0x04;
        buffer[5] = 0x03;
        buffer[6] = 0x02;
        buffer[7] = 0x01;
        buffer[8] = 0x00;
        buffer[9] = 0xC2;
        buffer[10] = 0x01;
        let (kind, profile) = comm_interface(&buffer).unwrap();
        assert_eq!(kind, 4);
        assert_eq!(profile.len(), COMM_PROFILE);
        assert_eq!(profile_dword(profile, PROFILE_PORT), Some(0x0102_0304));
        assert_eq!(profile_dword(profile, PROFILE_BAUD), Some(TYPE12_BAUD));
        assert_eq!(
            comm_interface(&buffer[..COMM_COPY - 1]),
            Err(super::CommError::Short)
        );
    }

    #[test]
    fn serial_defaults_fill_a_zero_baud_and_delay() {
        assert_eq!(baud_in_use(0, 0), Some(TYPE0_BAUD));
        assert_eq!(baud_in_use(1, 0), Some(TYPE12_BAUD));
        assert_eq!(baud_in_use(2, 0), Some(TYPE12_BAUD));
        assert_eq!(baud_in_use(0, 9600), Some(9600));
        assert_eq!(baud_in_use(4, 0), None);
        assert_eq!(delay_in_use(0, 0), Some(0xC8));
        assert_eq!(delay_in_use(1, 0), Some(0xC8));
        assert_eq!(delay_in_use(2, 0), Some(TYPE2_DELAY));
        assert_eq!(delay_in_use(8, 0), None);
        assert_eq!(serial_line(2), Some((8, 0)));
        assert_eq!(serial_line(4), None);
        assert_eq!(
            profile_dword(&vec![0; COMM_PROFILE], PROFILE_DELAY),
            Some(0)
        );
    }

    #[test]
    fn address_probes_keep_the_dll_order() {
        assert_eq!(TYPE_4_WRITES[0].address, b"192.168.8.1");
        assert_eq!(TYPE_4_WRITES[1].address, b"192.168.171.1");
        assert_eq!(TYPE_4_WRITES[2].address, b"192.168.8.1");
        assert_eq!(TYPE_4_WRITES[3].address, b"192.168.4.1");
        assert_eq!(TYPE_4_WRITES[0].mark, ADDRESS_MARK);
        assert_eq!(TYPE_4_WRITES[3].mark, ALT_MARK);
        assert_eq!(TYPE_8_WRITE.address, b"192.168.9.1");
        assert_eq!(TYPE_8_WRITE.mark, ADDRESS_MARK);
        let mut profile = vec![0_u8; COMM_PROFILE];
        profile[PROFILE_NAME..PROFILE_NAME + TYPE_8_WRITE.address.len()]
            .copy_from_slice(TYPE_8_WRITE.address);
        assert_eq!(
            &profile_name(&profile).unwrap()[..TYPE_8_WRITE.address.len()],
            TYPE_8_WRITE.address
        );
    }
}
