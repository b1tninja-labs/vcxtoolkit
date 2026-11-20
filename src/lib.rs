//! VCX serial frames for a VXDIAG dongle.
//!
//! This crate packs and unpacks one frame. On Windows it also opens the USB serial port.
//! It does not program a vehicle or talk to a license server.

mod frame;
mod serial;

pub use frame::{Frame, FrameError, MAX_PAYLOAD, checksum, pack, unpack};
pub use serial::USB_BAUD;
#[cfg(windows)]
pub use serial::{Call, OpenError, Port, State, open_usb};

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
