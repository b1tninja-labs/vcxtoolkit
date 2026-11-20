//! The `DevInfo` fields `VCX_DEV_GetInfo` logs.
//!
//! The reply payload has to be at least `0x3D` bytes before the DLL copies it.
//! The copy skips payload byte 0 and keeps the rest. The logged fields sit in
//! that copy. `ActiveFlag` is at `+0x3C` and `DoNetFlag` is at `+0x3D`, so a
//! reply only contains both when the payload is at least `0x3F` bytes. A shorter
//! payload leaves those two reads on whatever the caller buffer already held.

use crate::Command;
use crate::frame::Frame;

const SERIAL: usize = 0x10;
const HW_NAME: usize = 0x1C;
const VERSION_LAST: usize = 0x34;
const DAY: usize = 0x39;
const MONTH: usize = 0x3A;
const YEAR: usize = 0x3B;
const ACTIVE: usize = 0x3C;
const DO_NET: usize = 0x3D;

/// Payload length that still contains `DoNetFlag` after the skipped first byte.
const MIN_PAYLOAD: usize = DO_NET + 2;

/// Why a `GetInfo` reply was not turned into the logged fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InfoError {
    /// The header is not `80 00 8C 00`.
    NotGetInfo,
    /// The payload does not reach `DoNetFlag`.
    Short,
    /// `DevInfo.HwName` is not a UTF-8 C string inside the copy.
    HwName,
    /// The five serial bytes are not UTF-8, so the `%s` log is not text.
    Serial,
}

impl std::fmt::Display for InfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotGetInfo => "frame is not a GetInfo reply",
            Self::Short => "GetInfo payload does not reach DoNetFlag",
            Self::HwName => "DevInfo.HwName is not a UTF-8 C string",
            Self::Serial => "DevInfo.Serial is not UTF-8",
        })
    }
}

impl std::error::Error for InfoError {}

/// `DevInfo.FwDate`, logged as `%04d-%02d-%02d`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FirmwareDate {
    /// The byte at `+0x3B` plus 1970.
    pub year: u16,
    /// The byte at `+0x3A`.
    pub month: u8,
    /// The byte at `+0x39`.
    pub day: u8,
}

impl std::fmt::Display for FirmwareDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// The six fields `VCX_DEV_GetInfo` logs from the caller's buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    /// `DevInfo.Serial`. Empty when the check byte at serial `+7` does not match.
    pub serial: String,
    /// `DevInfo.HwName`.
    pub hw_name: String,
    /// `DevInfo.FwVersion` in log order: `+0x37`, `+0x36`, `+0x35`, `+0x34`.
    pub fw_version: [u8; 4],
    /// `DevInfo.FwDate`.
    pub fw_date: FirmwareDate,
    /// `DevInfo.ActiveFlag`, the byte at `+0x3C`.
    pub active_flag: u8,
    /// `DevInfo.DoNetFlag`, the byte at `+0x3D`.
    pub do_net_flag: u8,
}

/// Read the logged `GetInfo` fields from one unpacked reply.
///
/// # Errors
///
/// Returns [`InfoError`] when the frame is not a `GetInfo` reply, when the
/// payload stops before `DoNetFlag`, or when the serial or the hardware name
/// is not UTF-8 text.
#[must_use = "a device info that is parsed and dropped was not read"]
pub fn device_info(frame: &Frame) -> Result<DeviceInfo, InfoError> {
    if frame.header != Command::GetInfo.header(0) {
        return Err(InfoError::NotGetInfo);
    }
    if frame.payload.len() < MIN_PAYLOAD {
        return Err(InfoError::Short);
    }
    let copied = &frame.payload[1..];
    let mut serial_bytes = [0_u8; 8];
    serial_bytes.copy_from_slice(&copied[SERIAL..SERIAL + 8]);
    let name = &copied[HW_NAME..];
    let end = name
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(InfoError::HwName)?;
    let hw_name = std::str::from_utf8(&name[..end])
        .map_err(|_| InfoError::HwName)?
        .to_owned();
    Ok(DeviceInfo {
        serial: logged_serial(serial_bytes)?,
        hw_name,
        fw_version: [
            copied[VERSION_LAST + 3],
            copied[VERSION_LAST + 2],
            copied[VERSION_LAST + 1],
            copied[VERSION_LAST],
        ],
        fw_date: FirmwareDate {
            year: u16::from(copied[YEAR]) + 1970,
            month: copied[MONTH],
            day: copied[DAY],
        },
        active_flag: copied[ACTIVE],
        do_net_flag: copied[DO_NET],
    })
}

/// The `%s` at `DevInfo.Serial`.
///
/// The eight bytes at `+0x10` pass when byte 7 equals the xor of the other
/// seven with `0x82`. The log is then the first five bytes followed by `%05d`
/// of the big-endian word at bytes 5 and 6. A failed check logs an empty string.
fn logged_serial(bytes: [u8; 8]) -> Result<String, InfoError> {
    let check = bytes[0] ^ bytes[1] ^ bytes[2] ^ bytes[3] ^ bytes[4] ^ bytes[5] ^ bytes[6] ^ 0x82;
    if check != bytes[7] {
        return Ok(String::new());
    }
    let number = u16::from(bytes[5]) << 8 | u16::from(bytes[6]);
    let digits = format!("{number:05}");
    let mut raw = [0_u8; 10];
    raw[..5].copy_from_slice(&bytes[..5]);
    raw[5..].copy_from_slice(digits.as_bytes());
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
    std::str::from_utf8(&raw[..end])
        .map(ToOwned::to_owned)
        .map_err(|_| InfoError::Serial)
}

#[cfg(test)]
mod tests {
    use super::{InfoError, device_info, logged_serial};
    use crate::{Command, unpack};

    fn live_reply() -> crate::Frame {
        let wire = [
            0xBB, 0x80, 0x00, 0x8C, 0x00, 0x00, 0x6D, 0x75, 0x98, 0x00, 0x00, 0x40, 0xC1, 0x1D,
            0x11, 0x95, 0xC4, 0x07, 0x45, 0x03, 0x00, 0x00, 0x4E, 0x43, 0x41, 0x47, 0x4D, 0x67,
            0x8B, 0x28, 0x12, 0x00, 0x00, 0x00, 0x56, 0x43, 0x58, 0x2D, 0x4E, 0x41, 0x4E, 0x4F,
            0x00, 0x00, 0x00, 0x11, 0x00, 0x01, 0x01, 0x6B, 0x56, 0x43, 0x58, 0x41, 0x2E, 0x42,
            0x49, 0x4E, 0x02, 0x04, 0x09, 0x01, 0x00, 0x1F, 0x03, 0x35, 0x01, 0x00, 0x00, 0x00,
            0x58, 0xBB,
        ];
        unpack(&wire).unwrap()
    }

    #[test]
    fn the_live_reply_logs_the_nano_block() {
        let info = device_info(&live_reply()).unwrap();
        assert_eq!(info.serial, "NCAGM26507");
        assert_eq!(info.hw_name, "VCX-NANO");
        assert_eq!(info.fw_version, [1, 9, 4, 2]);
        assert_eq!(info.fw_date.to_string(), "2023-03-31");
        assert_eq!(info.active_flag, 0x01);
        assert_eq!(info.do_net_flag, 0x00);
    }

    #[test]
    fn a_bad_serial_check_logs_an_empty_string() {
        let mut frame = live_reply();
        let at = 1 + 0x10 + 7;
        frame.payload[at] ^= 0xFF;
        let info = device_info(&frame).unwrap();
        assert_eq!(info.serial, "");
        assert_eq!(info.hw_name, "VCX-NANO");
        assert_eq!(info.fw_version, [1, 9, 4, 2]);
    }

    #[test]
    fn a_zero_inside_the_five_serial_bytes_ends_the_log() {
        let mut bytes = [b'A', b'B', 0, b'C', b'D', 0x00, 0x01, 0];
        bytes[7] =
            bytes[0] ^ bytes[1] ^ bytes[2] ^ bytes[3] ^ bytes[4] ^ bytes[5] ^ bytes[6] ^ 0x82;
        assert_eq!(logged_serial(bytes).unwrap(), "AB");
    }

    #[test]
    fn a_payload_that_stops_before_do_net_is_refused() {
        let mut short = live_reply();
        short.payload.truncate(0x3E);
        assert_eq!(device_info(&short), Err(InfoError::Short));
        let mut exact = live_reply();
        exact.payload.truncate(0x3F);
        assert_eq!(device_info(&exact).unwrap().do_net_flag, 0x00);
    }

    #[test]
    fn a_non_utf8_serial_is_refused() {
        let mut bytes = [0xFF, b'B', b'C', b'D', b'E', 0x00, 0x01, 0];
        bytes[7] =
            bytes[0] ^ bytes[1] ^ bytes[2] ^ bytes[3] ^ bytes[4] ^ bytes[5] ^ bytes[6] ^ 0x82;
        assert_eq!(logged_serial(bytes), Err(InfoError::Serial));
    }

    #[test]
    fn an_echo_frame_is_not_device_info() {
        let frame = unpack(&crate::pack(Command::Echo.header(0), b"TEST").unwrap()).unwrap();
        assert_eq!(device_info(&frame), Err(InfoError::NotGetInfo));
    }

    #[test]
    fn a_name_with_no_terminator_is_refused() {
        let mut frame = live_reply();
        for byte in &mut frame.payload[1 + 0x1C..] {
            if *byte == 0 {
                *byte = b'X';
            }
        }
        assert_eq!(device_info(&frame), Err(InfoError::HwName));
    }
}
