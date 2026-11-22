//! The SAE J2534-1 return codes and the protocol ids, as plain constants.
//!
//! These are the SAE J2534-1 API numbers (the spec to cite from `primarysource`),
//! cross-referenced to ISO 22900-2:2009 (D-PDU API) where a clause applies
//! (`docs/j2534.md`). Nothing here touches hardware or the registry; it only
//! names the numbers the [`crate::ffi`] exports return.

/// A J2534 `long` return value. Every `PassThru*` export returns one of these.
pub type Status = i32;

/// The call succeeded. J2534-1 `STATUS_NOERROR`.
pub const STATUS_NOERROR: Status = 0x00;
/// The device or DLL does not support the function. `ERR_NOT_SUPPORTED`.
pub const ERR_NOT_SUPPORTED: Status = 0x01;
/// The channel id is not one this DLL handed out. `ERR_INVALID_CHANNEL_ID`.
pub const ERR_INVALID_CHANNEL_ID: Status = 0x02;
/// The protocol id is not one this DLL opens. `ERR_INVALID_PROTOCOL_ID`.
pub const ERR_INVALID_PROTOCOL_ID: Status = 0x03;
/// A required pointer argument was null. `ERR_NULL_PARAMETER`.
pub const ERR_NULL_PARAMETER: Status = 0x04;
/// A general failure. `ERR_FAILED`.
pub const ERR_FAILED: Status = 0x07;
/// No device is connected. `ERR_DEVICE_NOT_CONNECTED`.
pub const ERR_DEVICE_NOT_CONNECTED: Status = 0x08;
/// A read returned no message before the timeout. `ERR_TIMEOUT`.
pub const ERR_TIMEOUT: Status = 0x09;
/// A message was malformed. `ERR_INVALID_MSG`.
pub const ERR_INVALID_MSG: Status = 0x0A;
/// The device id is not one this DLL handed out. `ERR_INVALID_DEVICE_ID`.
pub const ERR_INVALID_DEVICE_ID: Status = 0x1A;

/// The J2534 protocol ids this DLL understands. The numbers are the J2534-1 API
/// values; the ISO 15765 one is the only channel this host would open for GM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Protocol {
    /// `J1850VPW`, J2534-1 id 1.
    J1850Vpw = 1,
    /// `J1850PWM`, J2534-1 id 2.
    J1850Pwm = 2,
    /// `ISO9141`, J2534-1 id 3.
    Iso9141 = 3,
    /// `ISO14230`, J2534-1 id 4.
    Iso14230 = 4,
    /// `CAN` (raw 11/29-bit), J2534-1 id 5.
    Can = 5,
    /// `ISO15765` (ISO-TP over CAN), J2534-1 id 6. The GM diagnostic channel.
    Iso15765 = 6,
}

impl Protocol {
    /// The J2534 protocol id, which is also the selector [`vcxtoolkit::open_code`]
    /// resolves to a `VCX_PRO_Open` dword.
    #[must_use]
    pub const fn id(self) -> u32 {
        self as u32
    }

    /// Name a J2534 protocol id, or `None` for one this DLL does not open.
    ///
    /// The set is deliberately the ids [`vcxtoolkit::open_code`] resolves and
    /// this host is ready to reason about; `PassThruConnect` answers
    /// [`ERR_INVALID_PROTOCOL_ID`] for anything else rather than guess a channel.
    #[must_use]
    pub const fn from_id(id: u32) -> Option<Self> {
        Some(match id {
            1 => Self::J1850Vpw,
            2 => Self::J1850Pwm,
            3 => Self::Iso9141,
            4 => Self::Iso14230,
            5 => Self::Can,
            6 => Self::Iso15765,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ERR_INVALID_PROTOCOL_ID, Protocol, STATUS_NOERROR};

    #[test]
    fn protocol_ids_match_the_vcx_open_selectors() {
        // Every J2534 protocol id this DLL accepts must resolve to a recovered
        // VCX_PRO_Open dword; if it did not, PassThruConnect would be opening a
        // channel the dongle has no code for.
        for protocol in [
            Protocol::J1850Vpw,
            Protocol::J1850Pwm,
            Protocol::Iso9141,
            Protocol::Iso14230,
            Protocol::Can,
            Protocol::Iso15765,
        ] {
            assert!(
                vcxtoolkit::open_code(protocol.id()).is_ok(),
                "{protocol:?} has no VCX_PRO_Open dword"
            );
        }
        // ISO 15765 is the GM channel and opens as 0x8001 (reads.md).
        assert_eq!(vcxtoolkit::open_code(Protocol::Iso15765.id()), Ok(0x8001));
    }

    #[test]
    fn an_unknown_protocol_is_not_named() {
        assert_eq!(Protocol::from_id(0), None);
        assert_eq!(Protocol::from_id(99), None);
        assert_eq!(Protocol::from_id(6), Some(Protocol::Iso15765));
        // Sanity on the two codes the exports lean on most.
        assert_eq!(STATUS_NOERROR, 0);
        assert_eq!(ERR_INVALID_PROTOCOL_ID, 3);
    }
}
