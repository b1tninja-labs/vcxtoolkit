//! VCX serial frames for a VXDIAG dongle.
//!
//! This crate talks to a VXDIAG dongle from Rust.
//!
//! It packs and unpacks one frame, names the command headers, and reads the `GetInfo`
//! fields the DLL logs. It reads a firmware image header and compares it with the
//! running version, and it reads the license info already stored on the device.
//! On Windows it opens the USB serial port and exchanges one frame.
//! It does not write firmware, call a license server, or program a vehicle.

mod command;
mod exchange;
mod frame;
mod image;
mod info;
mod license_info;
mod serial;

pub use command::{Command, pro_send_header};
#[cfg(windows)]
pub use exchange::{ExchangeError, exchange};
pub use frame::{Frame, FrameError, MAX_PAYLOAD, checksum, pack, unpack};
pub use image::{ImageError, ImageHeader, file_is_newer, read_header};
pub use info::{DeviceInfo, FirmwareDate, InfoError, device_info};
pub use license_info::{LicenseDate, LicenseInfo, LicenseInfoError, Product, parse_license_info};
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
