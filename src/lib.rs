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
//! `comm_interface` is the type and profile `VCX_GetCommIf` copies.
//! The caller's data is not invented here.
//! `pci_type` names the first byte when that data is already an ISO 15765 frame.
//! `flow_control` names the three bytes that follow when that byte is flow control.
//! `first_length` and `sequence_number` name the other two PCI fields.
//! `cli` is the command line: a `doctor` pre-flight that reads `GetInfo`, and a
//! held `collect`. `modread` serializes the saved module read `spskit-vit1` reads,
//! and `collect` gates the vehicle read that would fill it. That read stays held.
//! On Windows it opens the USB serial port and exchanges one frame.
//! `exchange` drives that frame over the `SerialPort` seam, and `device` composes
//! the seam into the channel-open plus Send/Recv exchange a vehicle read would use,
//! driven by a fake serial in tests and held behind the `collect` gate on hardware.
//! It does not write the license or the firmware to the device, and it does not
//! program a vehicle.

mod catalog;
pub mod cli;
pub mod collect;
mod comm;
mod command;
mod device;
mod dfu;
mod exchange;
mod forward;
mod frame;
mod hash;
mod image;
mod info;
mod license_info;
pub mod modread;
mod pci;
mod remote;
mod serial;
mod unlock;
mod update;

pub use catalog::{FirmwareItem, firmware_for, newer_firmware, parse_catalog};
pub use comm::{
    ADDRESS_MARK, ALT_MARK, AddressWrite, COMM_BYTE_SIZE, COMM_COPY, COMM_PARITY, COMM_PROFILE,
    CommError, PROFILE_BAUD, PROFILE_DELAY, PROFILE_NAME, PROFILE_NAME_LEN, PROFILE_PORT,
    TYPE_4_WRITES, TYPE_8_WRITE, TYPE0_BAUD, TYPE01_DELAY, TYPE1_NAMES, TYPE2_DELAY, TYPE2_NAMES,
    TYPE12_BAUD, baud_in_use, comm_interface, delay_in_use, profile_dword, profile_name,
    serial_line,
};
pub use command::{
    APP_COMMANDS, Command, ConfigCopy, DeviceTime, EXINFO_LEN, EXINFO_MARK, EXINFO_REPLY,
    ExInfoError, PinReading, TEST_COPY, TEST_REPLY, WaitReply, apply_ex_info, config_copy,
    device_time, get_pin_vol, pin_reading, pro_send_header, reset_ex, reset_payload, set_beep,
    set_led, set_pin_vol, syn_ex_info, test_record, wait_reply,
};
pub use device::{Channel, DeviceError, INVALID_COMMAND};
pub use dfu::{
    BOOT_MODE, BOOT_POLLS, Built, DFU_NAME_OFFSET, DFU_SERIAL_OFFSET, DfuError, LEAVE_MODE,
    LEAVE_POLLS, NAME_DIALOG, NAME_DIALOG_CAPTION, NAME_DIALOG_FLAGS, POLL_WAIT, RewriteError,
    dfu_get_info, dfu_serial, erase_chip, flash_length_fits, get_mode, in_boot, left_boot,
    name_blocks, poll_passes, rewrite_page, set_mode, write_flash, write_serial,
};
pub use exchange::{
    Deadline, ExchangeError, Patience, SerialPort, exchange, exchange_with, read_message,
};
pub use forward::{
    BATTERY_QUERY, BATTERY_SEED, ChannelFields, ClassPick, ComRecord, FILTER_CLEAR, FILTER_PASS,
    FilterCopy, FilterRecord, ForwardError, INIT_WAIT, MAX_FORWARD, NibblePath, Periodic,
    QueuePlace, QueuedMap, QueuedScale, START_MODE, Staged, TIMER_CLEAR, TIMER_PASS, TimerRecord,
    WiredCan, add_filter, add_timer, battery_reading, baud_record, class_pick, close_payload,
    config_records, copied_data, delivered_len, fast_init, five_baud_payload, flow_control_filter,
    j1850_check, lookup_record, mixed_bit, nibble_path, open_code, open_payload, pdu_flags,
    periodic_timer, place_queue, queued_map, queued_scale, reply_protocol, rx_status, send_payload,
    set_com_param, set_filter_payload, set_timer_payload, stage_record, start_filter,
    start_payload, stop_filter, stop_payload, stop_timer, uart_sum, wire_can, wire_id,
};
pub use frame::{
    DEVICE_CHECKSUM, Frame, FrameError, MAX_PAYLOAD, MAX_SEND, PLUS_BODY_MAX, PLUS_END,
    PLUS_ESCAPE, QueuedBody, SendCut, checksum, device_frame, device_will_send, marker_wrap, pack,
    plus_body, queued_body, recv_length, send_cut, unpack,
};
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
pub use pci::{
    FirstLength, FlowControl, FlowStatus, NEGATIVE_RESPONSE, PciType, Separation, ServicePci,
    first_length, flow_control, pci_type, response_bit, sequence_number, service_pci,
    single_service,
};
pub use remote::{
    FIRMWARE_CATALOG, HttpFailure, LICENSE_SERVER, RemoteError, auth_url, decode_license,
    license_url,
};
#[cfg(windows)]
pub use remote::{fetch_firmware, fetch_license, fetch_newer_firmware};
pub use serial::USB_BAUD;
#[cfg(windows)]
pub use serial::{Call, OpenError, Port, State, open_usb};
pub use unlock::{
    BASE_R, EXPONENT, FINAL_5, FINAL_R, Unlock, unlock_device, unlock_key, unlock_key_r,
};
pub use update::{
    FLASH_SEQUENCE_RECOVERED, FlashRefusal, Kept, Refused, UpdatePlan, UpdateStep, flash_gate,
    hold, plan_firmware,
};

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
