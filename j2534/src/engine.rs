//! The safe glue between a J2534 call and the `vcxtoolkit` codecs.
//!
//! Each function here is pointer-free and unit-tested; the FFI layer
//! ([`crate::ffi`]) does nothing but marshal the C arguments, call one of these,
//! and turn the result into a J2534 status. No function transmits: the Send PDU
//! is *built*, exactly as `vcxtoolkit`'s own `forward`/`collect` build theirs and
//! hold them.

use crate::msg::Msg;
use crate::rawmode;
use crate::status::{ERR_INVALID_MSG, ERR_INVALID_PROTOCOL_ID, Protocol, Status};

/// The `VCX_PRO_Open` dword for a J2534 `PassThruConnect` protocol id.
///
/// The J2534 protocol id is the same selector [`vcxtoolkit::open_code`] resolves,
/// so this is a validate-and-delegate: name the protocol, then ask the recovered
/// table for its dword.
///
/// # Errors
///
/// Returns [`ERR_INVALID_PROTOCOL_ID`] when the id is not one this host opens.
pub fn connect_open_code(protocol_id: u32) -> Result<u32, Status> {
    let protocol = Protocol::from_id(protocol_id).ok_or(ERR_INVALID_PROTOCOL_ID)?;
    vcxtoolkit::open_code(protocol.id()).map_err(|_| ERR_INVALID_PROTOCOL_ID)
}

/// The `VCX_PRO_Send` PDU a `PassThruWriteMsgs` of `msg` would hand the dongle.
///
/// For ISO 15765 the message data is `[CAN id BE u32][application bytes]`
/// (raw mode, ISO 22900-2 §9.2.6.3.6); `vcxtoolkit::copied_data` copies those
/// bytes to PDU offset 6 unchanged and `send_payload` frames them with the flag
/// word and length. This is the whole write path *up to transmission*; it returns
/// the bytes and sends nothing. The application content is the app's — this host
/// synthesizes no vehicle request.
///
/// # Errors
///
/// Returns [`ERR_INVALID_PROTOCOL_ID`] for a protocol with no VCX dword,
/// [`ERR_INVALID_MSG`] when ISO 15765 data lacks its 4-byte CAN id or the copy is
/// refused (e.g. over the `0x1020` cap).
pub fn write_send_pdu(msg: &Msg) -> Result<Vec<u8>, Status> {
    let protocol = Protocol::from_id(msg.protocol).ok_or(ERR_INVALID_PROTOCOL_ID)?;
    if protocol == Protocol::Iso15765 {
        // Confirm the raw-mode id is present (it is the app's routing), but keep
        // the whole data field as the bytes the dongle forwards.
        rawmode::split(&msg.data).map_err(|_| ERR_INVALID_MSG)?;
    }
    // flags 0: an ordinary request (forward::pdu_flags maps tx flags where a real
    // channel needs it; the scaffold keeps the plain request framing).
    let data =
        vcxtoolkit::copied_data(protocol.id(), &msg.data, false).map_err(|_| ERR_INVALID_MSG)?;
    vcxtoolkit::send_payload(0, &data).map_err(|_| ERR_INVALID_MSG)
}

/// Read the application bytes out of a `VCX_PRO_Recv` reply PDU for a J2534
/// `PassThruReadMsgs`.
///
/// This applies the recovered Recv envelope (`vcxtoolkit::reply_application_bytes`,
/// from `VCXPT32!ConvertVCXMsg`): a flag word, a length, then the data. For
/// ISO 15765 every byte is kept, including the leading 4-byte rx CAN id that raw
/// mode puts there — the app expects it in `Data`.
///
/// # Errors
///
/// Returns [`ERR_INVALID_MSG`] when the PDU does not hold a well-formed envelope.
pub fn read_application_bytes(pdu: &[u8]) -> Result<Vec<u8>, Status> {
    vcxtoolkit::collect::reply_application_bytes(pdu)
        .map(<[u8]>::to_vec)
        .map_err(|_| ERR_INVALID_MSG)
}

/// The DLL version string `PassThruReadVersion` reports for its own API field.
///
/// The firmware and device version are not known without an open device, so a
/// scaffold reports its own crate version and the `04.04` API generation and
/// leaves the firmware string to the hardware path.
#[must_use]
pub fn dll_version() -> &'static str {
    concat!("vcxtoolkit-j2534 ", env!("CARGO_PKG_VERSION"))
}

/// The status a call returns once it has validated its arguments but the proven
/// hardware exchange does not yet exist.
///
/// The native VCX vehicle exchange is unproven on hardware (a bench
/// `TesterPresent` returns the dongle status `0x9B`, no vehicle — see the project
/// ROADMAP), so the channel-bearing calls answer honestly rather than claim a
/// link. [`STATUS_NOERROR`](crate::status::STATUS_NOERROR) is never returned by
/// this helper.
#[must_use]
pub const fn pending_hardware() -> Status {
    crate::status::ERR_NOT_SUPPORTED
}

#[cfg(test)]
mod tests {
    use super::{
        connect_open_code, dll_version, pending_hardware, read_application_bytes, write_send_pdu,
    };
    use crate::msg::Msg;
    use crate::status::{ERR_INVALID_MSG, ERR_INVALID_PROTOCOL_ID, ERR_NOT_SUPPORTED};

    #[test]
    fn connect_resolves_iso15765_to_the_recovered_open_dword() {
        assert_eq!(connect_open_code(6), Ok(0x8001));
        assert_eq!(connect_open_code(5), Ok(0x8101));
        assert_eq!(connect_open_code(42), Err(ERR_INVALID_PROTOCOL_ID));
    }

    #[test]
    fn a_write_builds_the_recovered_send_pdu_and_does_not_transmit() {
        // Raw-mode ISO 15765 data: id 00 00 07 E0 then 1A C1. copied_data keeps it
        // verbatim; send_payload prefixes flags(0) BE and length(6) BE.
        let msg = Msg::from_fields(6, 0, 0, 6, &[0x00, 0x00, 0x07, 0xE0, 0x1A, 0xC1]).unwrap();
        let pdu = write_send_pdu(&msg).unwrap();
        assert_eq!(
            pdu,
            vec![
                0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x07, 0xE0, 0x1A, 0xC1
            ]
        );
        // The whole PDU is pinned above: built in full, and never transmitted here.
    }

    #[test]
    fn an_iso15765_write_without_a_can_id_is_an_invalid_message() {
        // Two bytes cannot carry the 4-byte raw-mode id.
        let msg = Msg::from_fields(6, 0, 0, 2, &[0x1A, 0xC1]).unwrap();
        assert_eq!(write_send_pdu(&msg), Err(ERR_INVALID_MSG));
    }

    #[test]
    fn a_reply_pdu_yields_its_application_bytes_including_the_rx_id() {
        // Recovered Recv envelope: flags u32 LE, len u16 LE, then data. ISO 15765
        // keeps every byte, so the leading rx id 00 00 07 E8 stays in Data.
        let data = [0x00, 0x00, 0x07, 0xE8, 0x5A, 0xC1, 0x11];
        let mut pdu = vec![0, 0, 0, 0];
        pdu.extend_from_slice(&u16::try_from(data.len()).unwrap().to_le_bytes());
        pdu.extend_from_slice(&data);
        assert_eq!(read_application_bytes(&pdu), Ok(data.to_vec()));
        // A stub PDU shorter than the envelope is an invalid message.
        assert_eq!(read_application_bytes(&[0x00, 0x00]), Err(ERR_INVALID_MSG));
    }

    #[test]
    fn the_pending_status_is_not_supported_and_the_version_names_the_crate() {
        assert_eq!(pending_hardware(), ERR_NOT_SUPPORTED);
        assert!(dll_version().starts_with("vcxtoolkit-j2534 "));
    }
}
