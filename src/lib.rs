//! VCX serial frames for a VXDIAG dongle.
//!
//! This crate packs and unpacks one frame. It does not open a serial port, program a vehicle,
//! or talk to a license server.

mod frame;

pub use frame::{Frame, FrameError, MAX_PAYLOAD, checksum, pack, unpack};

/// The name of this crate, as Cargo knows it.
#[must_use]
pub const fn name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    #[test]
    fn name_is_the_package_name() {
        assert_eq!(super::name(), "vcxcomm");
    }
}
