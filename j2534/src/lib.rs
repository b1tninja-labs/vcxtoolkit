//! An SAE J2534-1 pass-through DLL that hosts the native VCX-NANO.
//!
//! This crate is the `cdylib` a standard J2534 diagnostic application (Techline
//! Connect, GDS2) loads to drive the VCX-NANO through the ordinary `PassThru*`
//! API — without the proprietary VXDIAG / VX-Manager software. It owns only the
//! thin C-ABI boundary ([`ffi`]); every real decision is delegated to the safe,
//! `unsafe`-free [`vcxtoolkit`] codecs (`open_code`, `copied_data`,
//! `send_payload`, `reply_application_bytes`, …).
//!
//! The split is deliberate. `vcxtoolkit`'s library forbids `unsafe`, and a J2534
//! DLL cannot — its `extern "system" PassThru*` exports must cross the C ABI. So
//! the unsafe FFI lives here, in one module, behind an explicit
//! `#[allow(unsafe_code)]`, with the crate lint set to `deny`; the library stays
//! `forbid`. See `docs/j2534.md` for the full design.
//!
//! **Status: native driver.** On Windows the exports open the dongle and drive
//! the recovered, hardware-accepted sequence through the reusable
//! [`vcxtoolkit::Channel`] primitives (one copy in the library, shared with any
//! other consumer): `PassThruConnect` runs `Channel::connect_with`
//! (`VCX_PRO_Open(0x8001)` → `SetComParam` → `SetFilter` → `Start`),
//! `PassThruStartMsgFilter` installs the flow-control filter
//! (`add_flow_control_filter_with`), `PassThruWriteMsgs` sends through
//! `VCX_PRO_Send` (`send_raw_with`), and `PassThruReadMsgs` reads the pushed reply
//! frame (`read_msg_with`) and parses the recovered Recv envelope. The bring-up is
//! unit-tested over the serial seam in the library; the live COM4 bring-up is an
//! `#[ignore]`d owner-run test that sends no vehicle request.
//!
//! The read model is confirmed: a static read of `VCXPT32.dll` shows
//! `PassThruReadMsgs` issues no transmit — the ECU reply is pushed and an
//! arrival-driven callback enqueues it, so the driver reads a queued frame and
//! sends no `VCX_PRO_Recv` request. This host synthesizes no vehicle content (the
//! app owns `Data`), never writes the registry (`registry` builds the plan and
//! holds it), and never flips `vcxtoolkit::collect::READ_LAYOUT_RECOVERED`.

pub mod engine;
pub mod ffi;
pub mod msg;
pub mod rawmode;
pub mod registry;
pub(crate) mod runtime;
pub mod status;

/// The name of this crate, as Cargo knows it.
#[must_use]
pub fn name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    #[test]
    fn name_is_the_package_name() {
        assert_eq!(super::name(), "vcxtoolkit-j2534");
    }
}
