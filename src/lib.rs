//! VCX serial frames for a VXDIAG dongle.
//!
//! This crate packs and unpacks one frame, names the command headers, and reads the
//! `GetInfo` fields the DLL logs. On Windows it also opens the USB serial port.
//! It does not program a vehicle or talk to a license server.

mod command;
mod frame;
mod info;
mod serial;

pub use command::{Command, pro_send_header};
pub use frame::{Frame, FrameError, MAX_PAYLOAD, checksum, pack, unpack};
pub use info::{DeviceInfo, FirmwareDate, InfoError, device_info};
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
