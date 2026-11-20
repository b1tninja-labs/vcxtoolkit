//! VCX serial frames for a VXDIAG dongle.
//!
//! This crate talks to a VXDIAG dongle from Rust.
//!
//! It packs and unpacks one frame, names the command headers, and reads the `GetInfo`
//! fields the DLL logs. It reads a firmware image header and compares it with the
//! running version, and it reads the license info already stored on the device.
//! It retrieves a license from the license service and the firmware image the
//! catalog lists for a device. An image is an update only when it is newer than
//! the running version. `read_pages` walks the packed page records in that image.
//! The manager's update sequence is drawn in `docs/protocol.md`.
//! `plan_firmware` names those steps, and `hold` refuses the ones that would
//! be sent. The pass-through reads are planned in `docs/reads.md`.
//! `open_code` and `send_payload` are the bytes the pass-through copies.
//! The caller's data is not invented here.
//! On Windows it opens the USB serial port and exchanges one frame.
//! It does not write the license or the firmware to the device, and it does not
//! program a vehicle.

mod catalog;
mod command;
mod dfu;
mod exchange;
mod forward;
mod frame;
mod hash;
mod image;
mod info;
mod license_info;
mod remote;
mod serial;
mod update;

pub use catalog::{FirmwareItem, firmware_for, newer_firmware, parse_catalog};
pub use command::{Command, pro_send_header};
pub use dfu::{
    BOOT_MODE, Built, DFU_NAME_OFFSET, DfuError, LEAVE_MODE, LEAVE_POLLS, RewriteError,
    dfu_get_info, erase_chip, flash_length_fits, get_mode, in_boot, left_boot, name_blocks,
    rewrite_page, set_mode, write_flash, write_serial,
};
#[cfg(windows)]
pub use exchange::{ExchangeError, exchange};
pub use forward::{
    ComRecord, FilterRecord, ForwardError, MAX_FORWARD, TimerRecord, add_filter, add_timer,
    j1850_check, open_code, open_payload, send_payload, set_com_param, uart_sum,
};
pub use frame::{Frame, FrameError, MAX_PAYLOAD, checksum, pack, unpack};
pub use image::{
    ImageError, ImageHeader, PageRecord, file_is_newer, read_header, read_pages, version_is_newer,
};
pub use info::{DeviceInfo, FirmwareDate, InfoError, device_info};
pub use license_info::{LicenseDate, LicenseInfo, LicenseInfoError, Product, parse_license_info};
pub use remote::{
    FIRMWARE_CATALOG, HttpFailure, LICENSE_SERVER, RemoteError, auth_url, decode_license,
    license_url,
};
#[cfg(windows)]
pub use remote::{fetch_firmware, fetch_license, fetch_newer_firmware};
pub use serial::USB_BAUD;
#[cfg(windows)]
pub use serial::{Call, OpenError, Port, State, open_usb};
pub use update::{Kept, Refused, UpdatePlan, UpdateStep, hold, plan_firmware};

/// The name of this crate, as Cargo knows it.
#[must_use]
pub const fn name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    #[test]
    fn name_is_the_package_name() {
        assert_eq!(super::name(), "vcxtoolkit");
    }
}
