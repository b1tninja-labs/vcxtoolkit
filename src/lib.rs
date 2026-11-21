//! VCX serial frames for a VXDIAG dongle.
//!
//! This crate talks to a VXDIAG dongle from Rust.
//!
//! It packs and unpacks one frame, names the command headers, and reads the `GetInfo`
//! fields the DLL logs. It reads a firmware image header and compares it with the
//! running version, and it reads the license info already stored on the device.
//! The license key's second half is the challenge reply raised to the secret
//! that produced the nonce.
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
pub use command::{Command, ConfigCopy, config_copy, pro_send_header};
pub use dfu::{
    BOOT_MODE, BOOT_POLLS, Built, DFU_NAME_OFFSET, DFU_SERIAL_OFFSET, DfuError, LEAVE_MODE,
    LEAVE_POLLS, NAME_DIALOG, NAME_DIALOG_CAPTION, NAME_DIALOG_FLAGS, POLL_WAIT, RewriteError,
    dfu_get_info, dfu_serial, erase_chip, flash_length_fits, get_mode, in_boot, left_boot,
    name_blocks, poll_passes, rewrite_page, set_mode, write_flash, write_serial,
};
#[cfg(windows)]
pub use exchange::{ExchangeError, exchange};
pub use forward::{
    BATTERY_QUERY, BATTERY_SEED, ChannelFields, ComRecord, FILTER_CLEAR, FILTER_PASS, FilterCopy,
    FilterRecord, ForwardError, INIT_WAIT, MAX_FORWARD, Periodic, START_MODE, TIMER_CLEAR,
    TIMER_PASS, TimerRecord, add_filter, add_timer, battery_reading, baud_record, close_payload,
    config_records, copied_data, delivered_len, fast_init, five_baud_payload, j1850_check,
    lookup_record, mixed_bit, open_code, open_payload, pdu_flags, periodic_timer, reply_protocol,
    rx_status, send_payload, set_com_param, set_filter_payload, set_timer_payload, start_filter,
    start_payload, stop_filter, stop_payload, stop_timer, uart_sum,
};
pub use frame::{Frame, FrameError, MAX_PAYLOAD, checksum, pack, unpack};
pub use image::{
    ImageError, ImageHeader, PageRecord, file_is_newer, read_header, read_pages, version_is_newer,
};
pub use info::{DeviceInfo, FirmwareDate, InfoError, device_info};
pub use license_info::{
    LICENSE_CIPHER, LicenseDate, LicenseInfo, LicenseInfoError, Product, decrypt_license,
    license_key, license_key_half, license_nonce, parse_license_info,
};
#[cfg(windows)]
pub use license_info::{LicenseReadError, read_license};
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
