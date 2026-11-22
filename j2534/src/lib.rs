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
//! **Status: scaffold.** The exports' signatures, the `PASSTHRU_MSG` layout, the
//! raw-mode CAN-id convention, the J2534→VCX mapping, and the
//! `PassThruSupport.04.04` registry record are all here and tested. The
//! channel-bearing calls return `ERR_NOT_SUPPORTED` (pending): the native VCX
//! vehicle exchange is unproven on hardware (a bench `TesterPresent` returns a
//! dongle status, no vehicle), and this host does not claim a link it cannot
//! prove. Nothing here transmits to a vehicle, and nothing writes the registry;
//! `registry` builds the install plan and holds it, the way `vcxtoolkit::collect`
//! holds a vehicle read.

pub mod engine;
pub mod ffi;
pub mod msg;
pub mod rawmode;
pub mod registry;
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
