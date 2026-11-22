//! The held, gated vehicle read that would collect a module identity.
//!
//! Collecting a VIT1 clone means opening an ISO 15765 channel and reading the
//! identity and part identifiers a controller answers, then writing them as a
//! [`crate::modread`] profile for `spskit-vit1`. The channel open is recovered
//! ([`crate::open_code`] gives ISO 15765 its `0x8001` dword), but the brake-module
//! identity read is the single `[open]` row in `ROADMAP.md`: the caller's request
//! bytes and the `VCX_PRO_Recv` reply body are **not** recovered (`docs/reads.md`,
//! "The open row"). So this module builds the plan and refuses to transmit.
//!
//! The refusal is two independent gates, and this crate sets neither to the
//! passing value:
//!
//! - [`READ_LAYOUT_RECOVERED`] is `false`. It turns `true` only when the
//!   `VCX_PRO_Recv` read byte layout is genuinely recovered, not guessed.
//! - The owner's opt-in is a runtime flag the operator sets, never a constant
//!   here. It is the owner's decision to unhold a read of their own vehicle.
//!
//! Both must hold before [`vehicle_read_gate`] returns `Ok`. Until then every
//! path through [`collect`][self] stops before a byte reaches the car.

use crate::ForwardError;
use crate::modread::{IdentifierRead, ModuleRead, ModuleReadError};

/// The ISO 15765 (CAN) protocol selector the pass-through opens as `0x8001`.
pub const ISO15765_PROTOCOL: u32 = 6;

/// Whether the `VCX_PRO_Recv` read byte layout has been recovered.
///
/// `false` is the honest state: `docs/reads.md` records that the caller's request
/// bytes at a Send PDU's offset 6 and the bytes `VCX_PRO_Recv` writes at its
/// offset 6 were not recovered from `VCX.dll` or `ManagerCenter.exe`. This flips
/// to `true` only after that layout is recovered for real. Setting it `true`
/// without the recovery is exactly the invented frame the `[open]` row refuses.
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
    /// The `VCX_PRO_Recv` read byte layout is not recovered ([`READ_LAYOUT_RECOVERED`]).
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
                "recover the VCX_PRO_Recv read bytes and the Send PDU request bytes \
                 (the [open] row in ROADMAP.md / docs/reads.md), then set the recovered-layout \
                 flag; this crate does not invent them"
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
                f.write_str("vehicle read held: the VCX_PRO_Recv read byte layout is not recovered")
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

#[cfg(test)]
mod tests {
    use super::{
        CollectPlan, DEFAULT_IDENTITY_LIDS, GateRefusal, READ_LAYOUT_RECOVERED, vehicle_read_gate,
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
}
