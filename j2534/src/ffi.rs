//! The twelve `PassThru*` C-ABI exports — the only place this crate uses
//! `unsafe`.
//!
//! Each export marshals its C arguments, calls one safe [`crate::engine`]
//! function, and returns a J2534 [`Status`]. The crate lints forbid-equivalent
//! `unsafe`, so the whole boundary is confined to this module behind an explicit
//! `#![allow(unsafe_code)]`; the logic lives in the safe modules and is tested
//! there. Calling convention is `extern "system"` (Windows `__stdcall` on 32-bit,
//! the C ABI on 64-bit), the convention J2534-1 specifies.
//!
//! **This is a scaffold.** The channel-bearing calls (`PassThruConnect`,
//! `PassThruReadMsgs`, `PassThruWriteMsgs`, filters, ioctl) validate their
//! arguments and then return [`crate::status::ERR_NOT_SUPPORTED`]: the native VCX
//! vehicle exchange is unproven on hardware (ROADMAP), so the DLL answers honestly
//! rather than claim a link. No export transmits to a vehicle.

#![allow(unsafe_code)]
#![allow(non_snake_case)]

use core::ffi::{c_char, c_void};

use crate::engine;
use crate::status::{ERR_INVALID_CHANNEL_ID, ERR_NULL_PARAMETER, STATUS_NOERROR, Status};

/// A fixed device id this scaffold hands out. A real multi-device host would
/// track a table; the scaffold opens at most the one VCX on the port.
const DEVICE_ID: u32 = 1;

/// The J2534-1 version/error string buffer length (documented as 80 bytes).
const STRING_CAP: usize = 80;

/// Write a `u32` through a caller pointer, after a null check by the caller.
///
/// # Safety
///
/// `ptr` must be non-null and point to a writable `u32` (the caller checks null
/// first). The J2534 app owns the storage.
unsafe fn write_u32(ptr: *mut u32, value: u32) {
    unsafe { *ptr = value };
}

/// Copy `text` as a NUL-terminated C string into a caller buffer of at least
/// `cap` bytes (J2534 version/error buffers are documented as 80 bytes).
///
/// # Safety
///
/// `buf` must be non-null and point to at least `cap` writable bytes.
#[allow(clippy::cast_possible_wrap)] // c_char is i8 or u8 by platform; ASCII fits either
unsafe fn write_cstr(buf: *mut c_char, text: &str, cap: usize) {
    let bytes = text.as_bytes();
    let n = bytes.len().min(cap.saturating_sub(1));
    unsafe {
        for (i, &byte) in bytes[..n].iter().enumerate() {
            *buf.add(i) = byte as c_char;
        }
        *buf.add(n) = 0;
    }
}

/// `PassThruOpen` — open the pass-through device and hand back a device id.
///
/// Opening the DLL needs no vehicle, so this succeeds and reports `DEVICE_ID`.
///
/// # Safety
///
/// `pDeviceID` must be null or point to a writable `u32`. `pName` is ignored.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruOpen(_pName: *const c_void, pDeviceID: *mut u32) -> Status {
    if pDeviceID.is_null() {
        return ERR_NULL_PARAMETER;
    }
    unsafe { write_u32(pDeviceID, DEVICE_ID) };
    STATUS_NOERROR
}

/// `PassThruClose` — close a device opened by [`PassThruOpen`].
///
/// # Safety
///
/// Safe to call; `DeviceID` is a value, not a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruClose(DeviceID: u32) -> Status {
    if DeviceID != DEVICE_ID {
        return crate::status::ERR_INVALID_DEVICE_ID;
    }
    STATUS_NOERROR
}

/// `PassThruConnect` — open a protocol channel.
///
/// Validates the protocol against the recovered `VCX_PRO_Open` table
/// ([`engine::connect_open_code`]), then returns the pending-hardware status: the
/// channel bring-up (`SetComParam`/`SetFilter`/`Start` + flow-control filter) is
/// recovered but unproven on a vehicle, so the scaffold opens no live channel.
///
/// # Safety
///
/// `pChannelID` must be null or point to a writable `u32`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruConnect(
    DeviceID: u32,
    ProtocolID: u32,
    _Flags: u32,
    _BaudRate: u32,
    pChannelID: *mut u32,
) -> Status {
    if pChannelID.is_null() {
        return ERR_NULL_PARAMETER;
    }
    if DeviceID != DEVICE_ID {
        return crate::status::ERR_INVALID_DEVICE_ID;
    }
    match engine::connect_open_code(ProtocolID) {
        Ok(_dword) => engine::pending_hardware(),
        Err(status) => status,
    }
}

/// `PassThruDisconnect` — close a channel.
///
/// No channel is handed out by the scaffold, so any id is unknown.
///
/// # Safety
///
/// Safe to call; `ChannelID` is a value.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruDisconnect(_ChannelID: u32) -> Status {
    ERR_INVALID_CHANNEL_ID
}

/// `PassThruReadMsgs` — read queued messages.
///
/// # Safety
///
/// `pMsg` and `pNumMsgs` must be null or valid for the counts they carry.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruReadMsgs(
    _ChannelID: u32,
    pMsg: *mut crate::msg::PassThruMsg,
    pNumMsgs: *mut u32,
    _Timeout: u32,
) -> Status {
    if pMsg.is_null() || pNumMsgs.is_null() {
        return ERR_NULL_PARAMETER;
    }
    // No live channel, so nothing is queued: report zero messages read, then the
    // pending-hardware status (the Recv parse is engine::read_application_bytes).
    unsafe { write_u32(pNumMsgs, 0) };
    engine::pending_hardware()
}

/// `PassThruWriteMsgs` — write messages to the vehicle.
///
/// # Safety
///
/// `pMsg` and `pNumMsgs` must be valid for the counts they carry.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruWriteMsgs(
    _ChannelID: u32,
    pMsg: *const crate::msg::PassThruMsg,
    pNumMsgs: *mut u32,
    _Timeout: u32,
) -> Status {
    if pMsg.is_null() || pNumMsgs.is_null() {
        return ERR_NULL_PARAMETER;
    }
    // The send PDU is built by engine::write_send_pdu from the app's bytes and
    // held: the scaffold forwards nothing to a vehicle.
    unsafe { write_u32(pNumMsgs, 0) };
    engine::pending_hardware()
}

/// `PassThruStartMsgFilter` — install a message filter.
///
/// # Safety
///
/// Pointer arguments must be null or valid `PASSTHRU_MSG`s.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruStartMsgFilter(
    _ChannelID: u32,
    _FilterType: u32,
    _pMaskMsg: *const crate::msg::PassThruMsg,
    _pPatternMsg: *const crate::msg::PassThruMsg,
    _pFlowControlMsg: *const crate::msg::PassThruMsg,
    pFilterID: *mut u32,
) -> Status {
    if pFilterID.is_null() {
        return ERR_NULL_PARAMETER;
    }
    ERR_INVALID_CHANNEL_ID
}

/// `PassThruStopMsgFilter` — remove a filter.
///
/// # Safety
///
/// Safe to call; both arguments are values.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruStopMsgFilter(_ChannelID: u32, _FilterID: u32) -> Status {
    ERR_INVALID_CHANNEL_ID
}

/// `PassThruIoctl` — a protocol/control operation.
///
/// # Safety
///
/// `pInput` / `pOutput` must match the ioctl id's expectations when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruIoctl(
    _ChannelID: u32,
    _IoctlID: u32,
    _pInput: *mut c_void,
    _pOutput: *mut c_void,
) -> Status {
    engine::pending_hardware()
}

/// `PassThruSetProgrammingVoltage` — set a pin's programming voltage.
///
/// # Safety
///
/// Safe to call; all arguments are values.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruSetProgrammingVoltage(
    DeviceID: u32,
    _PinNumber: u32,
    _Voltage: u32,
) -> Status {
    if DeviceID != DEVICE_ID {
        return crate::status::ERR_INVALID_DEVICE_ID;
    }
    engine::pending_hardware()
}

/// `PassThruReadVersion` — report the firmware, DLL, and API version strings.
///
/// The DLL and API strings are known ([`engine::dll_version`], `04.04`); the
/// firmware string needs an open device, so it is reported as pending.
///
/// # Safety
///
/// Each non-null buffer must hold at least 80 bytes, per J2534-1.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruReadVersion(
    DeviceID: u32,
    pFirmwareVersion: *mut c_char,
    pDllVersion: *mut c_char,
    pApiVersion: *mut c_char,
) -> Status {
    if pFirmwareVersion.is_null() || pDllVersion.is_null() || pApiVersion.is_null() {
        return ERR_NULL_PARAMETER;
    }
    if DeviceID != DEVICE_ID {
        return crate::status::ERR_INVALID_DEVICE_ID;
    }
    unsafe {
        write_cstr(pFirmwareVersion, "unknown (device not opened)", STRING_CAP);
        write_cstr(pDllVersion, engine::dll_version(), STRING_CAP);
        write_cstr(pApiVersion, crate::registry::API_VERSION, STRING_CAP);
    }
    STATUS_NOERROR
}

/// The description [`PassThruGetLastError`] reports. A scaffold explains the
/// pending-hardware stance rather than track per-call state.
// Kept under the 80-byte J2534 buffer so it is reported whole, not truncated.
pub const LAST_ERROR_TEXT: &str =
    "vcxtoolkit-j2534: native VCX exchange unproven; channel calls pending";

/// `PassThruGetLastError` — the text for the most recent failure.
///
/// # Safety
///
/// `pErrorDescription` must hold at least 80 bytes when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruGetLastError(pErrorDescription: *mut c_char) -> Status {
    if pErrorDescription.is_null() {
        return ERR_NULL_PARAMETER;
    }
    unsafe { write_cstr(pErrorDescription, LAST_ERROR_TEXT, STRING_CAP) };
    STATUS_NOERROR
}

#[cfg(test)]
mod tests {
    use super::{
        DEVICE_ID, PassThruClose, PassThruConnect, PassThruGetLastError, PassThruOpen,
        PassThruReadVersion, PassThruWriteMsgs,
    };
    use crate::status::{
        ERR_INVALID_PROTOCOL_ID, ERR_NOT_SUPPORTED, ERR_NULL_PARAMETER, STATUS_NOERROR,
    };
    use core::ffi::c_char;
    use core::ptr;

    #[test]
    fn open_hands_back_the_device_id_and_rejects_a_null() {
        let mut id = 0_u32;
        assert_eq!(
            unsafe { PassThruOpen(ptr::null(), &raw mut id) },
            STATUS_NOERROR
        );
        assert_eq!(id, DEVICE_ID);
        assert_eq!(
            unsafe { PassThruOpen(ptr::null(), ptr::null_mut()) },
            ERR_NULL_PARAMETER
        );
        assert_eq!(unsafe { PassThruClose(DEVICE_ID) }, STATUS_NOERROR);
    }

    #[test]
    fn connect_validates_the_protocol_then_reports_pending() {
        let mut ch = 0_u32;
        // A good protocol (ISO 15765 = 6) passes validation but the hardware path
        // is unproven: pending, and no channel id written.
        assert_eq!(
            unsafe { PassThruConnect(DEVICE_ID, 6, 0, 500_000, &raw mut ch) },
            ERR_NOT_SUPPORTED
        );
        // A bad protocol is rejected outright.
        assert_eq!(
            unsafe { PassThruConnect(DEVICE_ID, 99, 0, 0, &raw mut ch) },
            ERR_INVALID_PROTOCOL_ID
        );
        // A null channel pointer is caught first.
        assert_eq!(
            unsafe { PassThruConnect(DEVICE_ID, 6, 0, 0, ptr::null_mut()) },
            ERR_NULL_PARAMETER
        );
    }

    #[test]
    fn a_write_reports_zero_sent_and_pending() {
        let mut n = 7_u32;
        let msg = crate::msg::PassThruMsg {
            ProtocolID: 6,
            RxStatus: 0,
            TxFlags: 0,
            Timestamp: 0,
            DataSize: 6,
            ExtraDataIndex: 0,
            Data: [0; crate::msg::DATA_MAX],
        };
        assert_eq!(
            unsafe { PassThruWriteMsgs(1, &raw const msg, &raw mut n, 100) },
            ERR_NOT_SUPPORTED
        );
        assert_eq!(n, 0, "nothing was transmitted");
    }

    #[test]
    fn read_version_fills_the_dll_and_api_strings() {
        let mut fw: [c_char; 80] = [0; 80];
        let mut dll: [c_char; 80] = [0; 80];
        let mut api: [c_char; 80] = [0; 80];
        assert_eq!(
            unsafe {
                PassThruReadVersion(
                    DEVICE_ID,
                    fw.as_mut_ptr(),
                    dll.as_mut_ptr(),
                    api.as_mut_ptr(),
                )
            },
            STATUS_NOERROR
        );
        let api_str = cstr(&api);
        assert_eq!(api_str, "04.04");
        assert!(cstr(&dll).starts_with("vcxtoolkit-j2534 "));
    }

    #[test]
    fn get_last_error_writes_the_pending_text() {
        let mut buf: [c_char; 80] = [0; 80];
        assert_eq!(
            unsafe { PassThruGetLastError(buf.as_mut_ptr()) },
            STATUS_NOERROR
        );
        assert!(cstr(&buf).contains("pending"));
        assert_eq!(
            unsafe { PassThruGetLastError(ptr::null_mut()) },
            ERR_NULL_PARAMETER
        );
    }

    fn cstr(buf: &[c_char]) -> String {
        let bytes: Vec<u8> = buf
            .iter()
            .take_while(|&&byte| byte != 0)
            .map(|&byte| byte.to_ne_bytes()[0])
            .collect();
        String::from_utf8(bytes).unwrap()
    }
}
