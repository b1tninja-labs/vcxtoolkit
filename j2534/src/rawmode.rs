//! The raw-mode (`PassThru`) CAN-id convention that a J2534 ISO 15765 message
//! carries, split out as a pure codec.
//!
//! A J2534 pass-through runs the link in *raw mode*: ISO 22900-2:2009
//! §9.2.6.3.6 is normative that "for ISO 15765, SAE J1939, and ISO 11898, the
//! first 4 bytes of the pCopData shall be the CAN ID (11 bit or 29 bit)", and
//! §11.1.4.18.5 repeats that the CAN id is in the first four bytes of the PDU
//! data (confirmed in `primarysource`, `iso:22900-2@2009`). The VCX pass-through
//! `VCXPT32.dll` follows exactly this: `ConvertPassthruMsg` copies the J2534
//! `PASSTHRU_MSG.Data` to the Send PDU's offset 6 **verbatim**, and the reply
//! copy is symmetric (the side-session note "vcx-open-row-recovered-from-vcxpt32").
//!
//! So the bytes a J2534 app hands [`crate::ffi::PassThruWriteMsgs`] are already
//! `[CAN id, big-endian u32][application bytes…]`, and they are exactly what
//! [`vcxtoolkit::copied_data`] / [`vcxtoolkit::send_payload`] wrap. This module
//! only *names* the split; it never invents an id or an application byte, and it
//! transmits nothing. The application content is the diagnostic app's, which is
//! why a pass-through DLL — unlike this crate's held `collect` path — carries no
//! synthesized vehicle request of its own.

/// The four-byte CAN-id prefix of a raw-mode ISO 15765 / CAN PDU.
pub const CAN_ID_LEN: usize = 4;

/// Why a raw-mode PDU could not be split at the CAN id.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawModeError {
    /// The data is shorter than the four-byte CAN id ISO 22900-2 §9.2.6.3.6
    /// requires at the front of an ISO 15765 / CAN raw-mode PDU.
    NoCanId,
}

impl std::fmt::Display for RawModeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("raw-mode ISO 15765 data is shorter than its 4-byte CAN id")
    }
}

impl std::error::Error for RawModeError {}

/// The CAN id and the application bytes of a raw-mode ISO 15765 / CAN PDU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawPdu<'a> {
    /// The arbitration id, from the first four data bytes, big-endian. An 11-bit
    /// id is the low 11 bits; a 29-bit id uses the whole word (the `CAN_29BIT_ID`
    /// tx flag, not inspected here, says which the app meant).
    pub can_id: u32,
    /// The application bytes after the id: the UDS/GMLAN service PDU the app
    /// built. These are the app's bytes, kept opaque.
    pub application: &'a [u8],
}

/// Split a raw-mode ISO 15765 / CAN `data` field at its CAN id.
///
/// This is the read side of the convention: given the `PASSTHRU_MSG.Data` an app
/// wrote (and that the dongle copies to the Send PDU's offset 6), name the id and
/// the application bytes without changing either.
///
/// # Errors
///
/// Returns [`RawModeError::NoCanId`] when `data` is shorter than [`CAN_ID_LEN`].
pub fn split(data: &[u8]) -> Result<RawPdu<'_>, RawModeError> {
    let id = data.get(..CAN_ID_LEN).ok_or(RawModeError::NoCanId)?;
    Ok(RawPdu {
        can_id: u32::from_be_bytes([id[0], id[1], id[2], id[3]]),
        application: &data[CAN_ID_LEN..],
    })
}

/// Build a raw-mode `data` field from a CAN id and application bytes.
///
/// The write side of the same convention: prepend the four big-endian id bytes
/// to the app's PDU, which is what the dongle forwards past offset 6. The caller
/// supplies both halves; this does not form a request.
#[must_use]
pub fn join(can_id: u32, application: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(CAN_ID_LEN + application.len());
    out.extend_from_slice(&can_id.to_be_bytes());
    out.extend_from_slice(application);
    out
}

#[cfg(test)]
mod tests {
    use super::{RawModeError, RawPdu, join, split};

    #[test]
    fn a_raw_iso15765_pdu_splits_into_the_id_and_the_application_bytes() {
        // The recovered example: a `$1A C1` read to 0x7E0 rides the wire as the
        // id 00 00 07 E0 then the UDS bytes 1A C1 (vcx-open-row note).
        let data = [0x00, 0x00, 0x07, 0xE0, 0x1A, 0xC1];
        assert_eq!(
            split(&data),
            Ok(RawPdu {
                can_id: 0x0000_07E0,
                application: &[0x1A, 0xC1],
            })
        );
        // And it round-trips through join.
        assert_eq!(join(0x0000_07E0, &[0x1A, 0xC1]), data);
    }

    #[test]
    fn a_pdu_without_a_full_can_id_is_refused() {
        // Three bytes cannot carry the 4-byte id ISO 22900-2 §9.2.6.3.6 requires.
        assert_eq!(split(&[0x00, 0x00, 0x07]), Err(RawModeError::NoCanId));
        // Exactly four bytes is an id with an empty application PDU, not an error.
        assert_eq!(
            split(&[0x00, 0x00, 0x07, 0xE8]),
            Ok(RawPdu {
                can_id: 0x0000_07E8,
                application: &[],
            })
        );
    }

    #[test]
    fn a_29_bit_id_keeps_its_whole_word() {
        let data = join(0x18DA_F110, &[0x22, 0xF1, 0x90]);
        assert_eq!(split(&data).unwrap().can_id, 0x18DA_F110);
    }
}
