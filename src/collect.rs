//! The held, gated vehicle read that would collect a module identity.
//!
//! Collecting a VIT1 clone means opening an ISO 15765 channel and reading the
//! identity and part identifiers a controller answers, then writing them as a
//! [`crate::modread`] profile for `spskit-vit1`. The channel open is recovered
//! ([`crate::open_code`] gives ISO 15765 its `0x8001` dword), and the frame
//! envelope both ways is recovered ([`frame_identity_request`],
//! [`read_identity_reply`]), but the brake-module identity read is the single
//! `[open]` row in `ROADMAP.md`: the caller's vehicle-facing request *content* is
//! **not** in the VXDIAG binaries (`docs/reads.md`, "The open row"). So this module
//! builds the plan and refuses to transmit.
//!
//! The refusal is two independent gates, and this crate sets neither to the
//! passing value:
//!
//! - [`READ_LAYOUT_RECOVERED`] is `false`. The envelope both ways is recovered
//!   (the `VCX_PRO_Send` framing and the `VCX_PRO_Recv` reply read layout), but
//!   the request content is not; it turns `true` only when a real request is
//!   recovered, not guessed.
//! - The owner's opt-in is a runtime flag the operator sets, never a constant
//!   here. It is the owner's decision to unhold a read of their own vehicle.
//!
//! Both must hold before [`vehicle_read_gate`] returns `Ok`. Until then every
//! path through [`collect`][self] stops before a byte reaches the car.

use crate::ForwardError;
use crate::modread::{IdentifierRead, ModuleRead, ModuleReadError};

/// The ISO 15765 (CAN) protocol selector the pass-through opens as `0x8001`.
pub const ISO15765_PROTOCOL: u32 = 6;

/// Whether the identity read's wire layout has been recovered in full.
///
/// `false` is the honest state. The 2026-11-22 static pass (`docs/reads.md`, "The
/// open row") recovered the *envelope* both ways — the `VCX_PRO_Send` framing
/// (`0x7B2F5740`) and the reply read layout from `VCXPT32!ConvertVCXMsg`
/// (`FUN_10006070`), which [`reply_application_bytes`] and [`read_identity_reply`]
/// apply. What is still missing is the vehicle-facing request *content*: service
/// `$1A`, the identifiers, and the module CAN id are not in `VCX.dll`,
/// `ManagerCenter.exe`, or `VCXPT32.dll` — they are GM application-layer. Until a
/// real request is recovered (not synthesized), this stays `false`. Flipping it
/// without that is exactly the invented frame the `[open]` row refuses.
pub const READ_LAYOUT_RECOVERED: bool = false;

/// The GMLAN `$1A` identity and part slice a VIT1 clone reads from a controller.
///
/// `C1`..`CA` are the ten part-number identifiers, `CB` and `CC` the hardware
/// identifier `spskit`'s saved-read rules consume. The exact set a given
/// controller answers is its request method; this is the common default the
/// operator can override. These are the identifiers a read *would* request. Their
/// on-wire request bytes are the unrecovered gap, so nothing here sends them.
pub const DEFAULT_IDENTITY_LIDS: [u8; 12] = [
    0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC,
];

/// Why a vehicle-facing send is refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateRefusal {
    /// The identity read's request content is not recovered ([`READ_LAYOUT_RECOVERED`]).
    LayoutUnrecovered,
    /// The layout is recovered but the owner has not opted in to a live read.
    OwnerHold,
}

impl GateRefusal {
    /// The documented next step this refusal points to.
    #[must_use]
    pub fn next_step(self) -> &'static str {
        match self {
            Self::LayoutUnrecovered => {
                "the VCX_PRO_Recv reply envelope and the VCX_PRO_Send framing are recovered, but \
                 the Send PDU request content (service $1A, the identifiers, the module CAN id) is \
                 not in the VXDIAG binaries (the [open] row in ROADMAP.md / docs/reads.md); recover \
                 a real request, then set the recovered-layout flag; this crate does not invent it"
            }
            Self::OwnerHold => {
                "the layout is recovered, so the owner must opt in to a live read of their \
                 own vehicle; pass the owner opt-in flag to proceed"
            }
        }
    }
}

impl std::fmt::Display for GateRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LayoutUnrecovered => {
                f.write_str("vehicle read held: the $1A request content is not recovered")
            }
            Self::OwnerHold => {
                f.write_str("vehicle read held: the owner has not opted in to a live read")
            }
        }
    }
}

impl std::error::Error for GateRefusal {}

/// Decide whether a vehicle-facing send may proceed.
///
/// Both gates must pass. The layout gate is checked first, because an owner
/// opt-in means nothing while the bytes that would be sent are unknown.
///
/// # Errors
///
/// Returns [`GateRefusal::LayoutUnrecovered`] when `layout_recovered` is `false`,
/// then [`GateRefusal::OwnerHold`] when `owner_opt_in` is `false`.
pub fn vehicle_read_gate(layout_recovered: bool, owner_opt_in: bool) -> Result<(), GateRefusal> {
    if !layout_recovered {
        return Err(GateRefusal::LayoutUnrecovered);
    }
    if !owner_opt_in {
        return Err(GateRefusal::OwnerHold);
    }
    Ok(())
}

/// The recovered scaffold of a collection, with the vehicle send held.
///
/// The channel-open dword is recovered, so it is a concrete value here. The read
/// of each identifier is not: `request_bytes_recovered` is `false`, and this plan
/// carries the identifiers it *would* request without any on-wire request bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectPlan {
    /// The controller name the saved read will be filed under.
    pub ecu: String,
    /// The ISO 15765 open dword, from [`crate::open_code`].
    pub open_code: u32,
    /// The `$1A` identifiers the read would request.
    pub lids: Vec<u8>,
    /// Always `false`: the request/response byte layout is the `[open]` row.
    pub request_bytes_recovered: bool,
}

impl CollectPlan {
    /// Build the plan for one controller and a list of `$1A` identifiers.
    ///
    /// # Errors
    ///
    /// Returns [`ForwardError::UnknownProtocol`] only if the ISO 15765 selector
    /// ever stops resolving, which would be a regression in [`crate::open_code`].
    pub fn iso15765(ecu: impl Into<String>, lids: &[u8]) -> Result<Self, ForwardError> {
        Ok(Self {
            ecu: ecu.into(),
            open_code: crate::open_code(ISO15765_PROTOCOL)?,
            lids: lids.to_vec(),
            request_bytes_recovered: READ_LAYOUT_RECOVERED,
        })
    }

    /// The empty saved-read skeleton this plan would fill once a read runs.
    ///
    /// Every identifier is present with no data: the block is well-formed for
    /// `spskit`, and the absent bytes make plain that nothing was read. This is a
    /// skeleton, not a vehicle read.
    ///
    /// # Errors
    ///
    /// Returns [`ModuleReadError::BadEcuName`] when the ECU name is unusable.
    pub fn skeleton(&self) -> Result<ModuleRead, ModuleReadError> {
        // Request/response headers are not known from the DLL, so the skeleton
        // uses zeroes the operator overwrites; the identifiers are the request set.
        let mut block = ModuleRead::new(self.ecu.clone(), 0, 0, false)?;
        for lid in &self.lids {
            block.push(IdentifierRead::lid(*lid, Vec::new()));
        }
        Ok(block)
    }
}

/// The positive-response service id of a `$1A` read: `0x1A | 0x40`.
///
/// ISO 14229-1:2013 §7.3.2 sets bit 6 of the service id on a positive response
/// ([`crate::pci::response_bit`]). `spskit`'s saved read stores the bytes *after*
/// this id and the identifier ([`crate::modread`]).
pub const IDENTITY_POSITIVE_SID: u8 = 0x5A;

/// Why a reply PDU could not be read as a `$1A` identity response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplyError {
    /// The PDU is shorter than the recovered envelope (six bytes), or the data
    /// bytes it claims are not all present.
    Truncated,
    /// The controller answered `0x7F` (ISO 14229-1:2013 §7.3.3).
    Negative {
        /// The echoed service id. `0x1A` for an identity read.
        service: u8,
        /// The negative-response code byte.
        code: u8,
    },
    /// The first application byte is neither [`IDENTITY_POSITIVE_SID`] nor
    /// the negative-response id.
    NotIdentity {
        /// The first application byte.
        first: u8,
    },
    /// The response id matched but the echoed identifier is not the one asked.
    WrongIdentifier {
        /// The identifier the controller echoed.
        got: u8,
    },
}

impl std::fmt::Display for ReplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => f.write_str("reply PDU is shorter than the recovered envelope"),
            Self::Negative { service, code } => {
                write!(
                    f,
                    "controller answered 0x7F to service {service:#04x}, code {code:#04x}"
                )
            }
            Self::NotIdentity { first } => {
                write!(
                    f,
                    "first application byte {first:#04x} is not a $1A response"
                )
            }
            Self::WrongIdentifier { got } => {
                write!(
                    f,
                    "the controller echoed identifier {got:#04x}, not the one asked"
                )
            }
        }
    }
}

impl std::error::Error for ReplyError {}

/// Frame the recovered `VCX_PRO_Send` PDU around an already-formed request.
///
/// This is the recovered Send *framing* only (`VCX_PRO_Send` `0x7B2F5740`;
/// `VCXPT32!ConvertPassthruMsg` `FUN_10005df0` copies the ISO 15765 bytes
/// unchanged to PDU offset 6). `uds` is the caller's vehicle-facing request —
/// its bytes are **not** recovered from the VXDIAG binaries, which carry no `$1A`
/// request builder (`docs/reads.md`, "The open row"). This crate does not
/// synthesize `1A <identifier>`; it only wraps bytes a caller supplies, and
/// [`collect`][self] still refuses to transmit the result.
///
/// # Errors
///
/// Returns [`ForwardError::TooLong`] when `uds` exceeds [`crate::MAX_FORWARD`].
pub fn frame_identity_request(uds: &[u8]) -> Result<Vec<u8>, ForwardError> {
    // ISO 15765 copies the caller's bytes to offset 6 unchanged (no check byte).
    let data = crate::forward::copied_data(ISO15765_PROTOCOL, uds, false)?;
    // Flags 0: an ordinary request carries no tx-flag rewrites (forward::pdu_flags).
    crate::forward::send_payload(0, &data)
}

/// The ISO 15765 application bytes a reply PDU delivers, per the recovered envelope.
///
/// `VCX_PRO_Recv` fills the PDU and `VCXPT32!ConvertVCXMsg` (`FUN_10006070`,
/// `0x10006070`) reads it: a little-endian `u32` flags word at offset 0, a
/// little-endian `u16` length at offset 4, and that many data bytes at offset 6.
/// ISO 15765 keeps every data byte ([`crate::delivered_len`] for protocol
/// [`ISO15765_PROTOCOL`] returns the whole length). The returned slice is the
/// controller's application data; this does not skip a CAN address prefix, the
/// same open question `crate::pci` documents.
///
/// # Errors
///
/// Returns [`ReplyError::Truncated`] when the PDU is under six bytes or claims
/// more data than it carries.
pub fn reply_application_bytes(pdu: &[u8]) -> Result<&[u8], ReplyError> {
    let header = pdu.get(..6).ok_or(ReplyError::Truncated)?;
    let len = u16::from_le_bytes([header[4], header[5]]);
    let data = pdu
        .get(6..6 + usize::from(len))
        .ok_or(ReplyError::Truncated)?;
    // ISO 15765 drops no trailing byte; delivered_len proves it against the DLL.
    let Some(kept) = crate::forward::delivered_len(ISO15765_PROTOCOL, len, false) else {
        return Err(ReplyError::Truncated);
    };
    data.get(..usize::from(kept)).ok_or(ReplyError::Truncated)
}

/// Read a `$1A` identity response for `identifier` out of a reply PDU.
///
/// Applies [`reply_application_bytes`] (the recovered Recv envelope) and then
/// names the `$1A` response: a positive response is [`IDENTITY_POSITIVE_SID`],
/// the echoed identifier, then the data `spskit` stores; a negative response is
/// `0x7F`, the service id, and the code (ISO 14229-1:2013 §7.3.1, §7.3.3).
///
/// # Errors
///
/// Returns [`ReplyError`] when the PDU is truncated, the controller answered
/// negatively, the response is not a `$1A` response, or the echoed identifier is
/// not `identifier`.
pub fn read_identity_reply(pdu: &[u8], identifier: u8) -> Result<Vec<u8>, ReplyError> {
    let app = reply_application_bytes(pdu)?;
    let first = *app.first().ok_or(ReplyError::Truncated)?;
    if first == crate::pci::NEGATIVE_RESPONSE {
        return Err(ReplyError::Negative {
            service: *app.get(1).ok_or(ReplyError::Truncated)?,
            code: *app.get(2).ok_or(ReplyError::Truncated)?,
        });
    }
    if first != IDENTITY_POSITIVE_SID {
        return Err(ReplyError::NotIdentity { first });
    }
    let got = *app.get(1).ok_or(ReplyError::Truncated)?;
    if got != identifier {
        return Err(ReplyError::WrongIdentifier { got });
    }
    Ok(app.get(2..).unwrap_or_default().to_vec())
}

#[cfg(test)]
mod tests {
    use super::{
        CollectPlan, DEFAULT_IDENTITY_LIDS, GateRefusal, IDENTITY_POSITIVE_SID,
        READ_LAYOUT_RECOVERED, ReplyError, frame_identity_request, read_identity_reply,
        reply_application_bytes, vehicle_read_gate,
    };

    #[test]
    fn the_gate_refuses_until_both_flags_pass() {
        assert_eq!(
            vehicle_read_gate(false, false),
            Err(GateRefusal::LayoutUnrecovered),
        );
        assert_eq!(
            vehicle_read_gate(false, true),
            Err(GateRefusal::LayoutUnrecovered),
        );
        // Even with the owner's opt-in, an unrecovered layout stays held.
        assert_eq!(vehicle_read_gate(true, false), Err(GateRefusal::OwnerHold));
        assert_eq!(vehicle_read_gate(true, true), Ok(()));
    }

    #[test]
    fn the_crate_defaults_keep_the_gate_closed() {
        // With the shipped constant and no opt-in, the gate is closed.
        assert_eq!(
            vehicle_read_gate(READ_LAYOUT_RECOVERED, true),
            Err(GateRefusal::LayoutUnrecovered),
        );
    }

    #[test]
    fn each_refusal_names_a_next_step() {
        assert!(
            GateRefusal::LayoutUnrecovered
                .next_step()
                .contains("VCX_PRO_Recv")
        );
        assert!(GateRefusal::OwnerHold.next_step().contains("opt in"));
    }

    #[test]
    fn the_plan_has_the_recovered_open_code_and_no_request_bytes() {
        let plan = CollectPlan::iso15765("BECM", &DEFAULT_IDENTITY_LIDS).unwrap();
        assert_eq!(plan.open_code, 0x8001, "ISO 15765 opens as 0x8001");
        assert!(
            !plan.request_bytes_recovered,
            "the request bytes stay unrecovered"
        );
        assert_eq!(plan.lids, DEFAULT_IDENTITY_LIDS);
    }

    #[test]
    fn the_skeleton_is_one_empty_line_per_identifier() {
        let plan = CollectPlan::iso15765("BECM", &[0xC1, 0xCC]).unwrap();
        let block = plan.skeleton().unwrap();
        assert_eq!(block.reads.len(), 2);
        assert!(block.reads.iter().all(|read| read.data.is_empty()));
        assert_eq!(block.reads[0].line().unwrap(), "lid C1 ");
        assert_eq!(block.reads[1].line().unwrap(), "lid CC ");
    }

    // A reply PDU in the recovered envelope: flags u32 LE, len u16 LE, then `data`.
    // `VCXPT32!ConvertVCXMsg` (FUN_10006070) reads exactly these offsets, and ISO
    // 15765 keeps every data byte.
    fn reply_pdu(flags: u32, data: &[u8]) -> Vec<u8> {
        let mut pdu = Vec::with_capacity(6 + data.len());
        pdu.extend_from_slice(&flags.to_le_bytes());
        pdu.extend_from_slice(&u16::try_from(data.len()).unwrap().to_le_bytes());
        pdu.extend_from_slice(data);
        pdu
    }

    #[test]
    fn the_send_framing_wraps_caller_bytes_in_the_recovered_pdu() {
        // The UDS bytes here are an EXAMPLE FIXTURE, not recovered from the binary:
        // a classical single frame `02 1A C1` (length 2, service $1A, identifier C1).
        // frame_identity_request only applies the recovered VCX_PRO_Send framing.
        let pdu = frame_identity_request(&[0x02, 0x1A, 0xC1]).unwrap();
        assert_eq!(
            pdu,
            // flags 0 big-endian, length 3 big-endian, then the three bytes at offset 6.
            vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x02, 0x1A, 0xC1],
        );
    }

    #[test]
    fn the_recovered_envelope_keeps_every_iso15765_data_byte() {
        // Flags with the TX-indication bit set must not change the kept length.
        let pdu = reply_pdu(0x0000_0008, &[0x5A, 0xC1, 0x11, 0x22, 0x33]);
        assert_eq!(
            reply_application_bytes(&pdu).unwrap(),
            &[0x5A, 0xC1, 0x11, 0x22, 0x33],
        );
        // A PDU whose claimed length runs past its bytes is truncated.
        assert_eq!(
            reply_application_bytes(&[0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x5A]),
            Err(ReplyError::Truncated),
        );
        // Under six bytes there is no envelope at all.
        assert_eq!(
            reply_application_bytes(&[0x00, 0x00]),
            Err(ReplyError::Truncated)
        );
    }

    #[test]
    fn a_positive_identity_reply_yields_the_bytes_after_the_identifier() {
        // Table of (identifier, returned data) a controller would answer.
        for (did, data) in [
            (0xC1_u8, vec![0x31, 0x32, 0x33, 0x34]),
            (0xCC_u8, vec![0xAB]),
            (0xCB_u8, Vec::new()),
        ] {
            let mut app = vec![IDENTITY_POSITIVE_SID, did];
            app.extend_from_slice(&data);
            let pdu = reply_pdu(0, &app);
            assert_eq!(read_identity_reply(&pdu, did), Ok(data));
        }
    }

    #[test]
    fn a_negative_reply_names_the_service_and_code() {
        // 0x7F, service $1A, code 0x31 (requestOutOfRange).
        let pdu = reply_pdu(0, &[0x7F, 0x1A, 0x31]);
        assert_eq!(
            read_identity_reply(&pdu, 0xC1),
            Err(ReplyError::Negative {
                service: 0x1A,
                code: 0x31,
            }),
        );
    }

    #[test]
    fn a_wrong_echo_or_wrong_service_is_refused() {
        // Right service, wrong echoed identifier.
        let pdu = reply_pdu(0, &[IDENTITY_POSITIVE_SID, 0xC2, 0x00]);
        assert_eq!(
            read_identity_reply(&pdu, 0xC1),
            Err(ReplyError::WrongIdentifier { got: 0xC2 }),
        );
        // A first byte that is neither 0x5A nor 0x7F is not a $1A response. Break
        // the positive-response id by one bit and the parse must refuse it.
        let pdu = reply_pdu(0, &[0x5B, 0xC1, 0x00]);
        assert_eq!(
            read_identity_reply(&pdu, 0xC1),
            Err(ReplyError::NotIdentity { first: 0x5B }),
        );
    }
}
