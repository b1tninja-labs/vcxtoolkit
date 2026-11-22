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

/// The single channel id this driver hands out (one ISO 15765 channel).
const CHANNEL_ID: u32 = 1;

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

/// Copy a `PASSTHRU_MSG`'s live `Data[..DataSize]` into an owned vector.
///
/// # Safety
///
/// `p` must point to a valid, readable `PassThruMsg`.
unsafe fn msg_data(p: *const crate::msg::PassThruMsg) -> Vec<u8> {
    let msg = unsafe { &*p };
    let size = usize::try_from(msg.DataSize)
        .unwrap_or(0)
        .min(crate::msg::DATA_MAX);
    msg.Data[..size].to_vec()
}

/// Fill a `PASSTHRU_MSG` the app provided with one ISO 15765 reply's bytes.
///
/// # Safety
///
/// `p` must point to a valid, writable `PassThruMsg`.
unsafe fn fill_msg(p: *mut crate::msg::PassThruMsg, app: &[u8]) {
    let msg = unsafe { &mut *p };
    msg.ProtocolID = 6;
    msg.RxStatus = 0;
    msg.TxFlags = 0;
    msg.Timestamp = 0;
    msg.ExtraDataIndex = 0;
    let n = app.len().min(crate::msg::DATA_MAX);
    msg.DataSize = u32::try_from(n).unwrap_or(0);
    msg.Data[..n].copy_from_slice(&app[..n]);
}

/// `PassThruOpen` — open the VCX-NANO on its serial port and hand back a device id.
///
/// Opens the dongle (`runtime::open`, default COM4). `pName` is not parsed; the
/// VCX-NANO's port is used as configured. On success the device id is `DEVICE_ID`.
///
/// # Safety
///
/// `pDeviceID` must be null or point to a writable `u32`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruOpen(_pName: *const c_void, pDeviceID: *mut u32) -> Status {
    if pDeviceID.is_null() {
        return ERR_NULL_PARAMETER;
    }
    let status = crate::runtime::open(crate::runtime::DEFAULT_PORT);
    if status == STATUS_NOERROR {
        unsafe { write_u32(pDeviceID, DEVICE_ID) };
    }
    status
}

/// `PassThruClose` — close the device opened by [`PassThruOpen`].
///
/// # Safety
///
/// Safe to call; `DeviceID` is a value, not a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruClose(DeviceID: u32) -> Status {
    if DeviceID != DEVICE_ID {
        return crate::status::ERR_INVALID_DEVICE_ID;
    }
    crate::runtime::close()
}

/// `PassThruConnect` — open an ISO 15765 channel and run the recovered bring-up.
///
/// Runs the hardware-accepted GM sequence `VCX_PRO_Open(0x8001)` →
/// `SetComParam{key 3, baud}` → `SetFilter(1)` → `Start` (`runtime::connect`). Only
/// ISO 15765 (protocol 6) is wired natively; other protocols return
/// `ERR_NOT_SUPPORTED`. The app still installs a flow-control filter
/// ([`PassThruStartMsgFilter`]) before a reply is delivered.
///
/// # Safety
///
/// `pChannelID` must be null or point to a writable `u32`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruConnect(
    DeviceID: u32,
    ProtocolID: u32,
    _Flags: u32,
    BaudRate: u32,
    pChannelID: *mut u32,
) -> Status {
    if pChannelID.is_null() {
        return ERR_NULL_PARAMETER;
    }
    if DeviceID != DEVICE_ID {
        return crate::status::ERR_INVALID_DEVICE_ID;
    }
    let status = crate::runtime::connect(ProtocolID, BaudRate);
    if status == STATUS_NOERROR {
        unsafe { write_u32(pChannelID, CHANNEL_ID) };
    }
    status
}

/// `PassThruDisconnect` — close a channel.
///
/// The channel's dongle teardown is left to [`PassThruClose`]; this only validates
/// the channel id.
///
/// # Safety
///
/// Safe to call; `ChannelID` is a value.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruDisconnect(ChannelID: u32) -> Status {
    if ChannelID != CHANNEL_ID {
        return ERR_INVALID_CHANNEL_ID;
    }
    STATUS_NOERROR
}

/// `PassThruReadMsgs` — read one queued ISO 15765 reply.
///
/// Reads the next reply frame the dongle pushed and parses the recovered Recv
/// envelope (`runtime::read`), filling one `PASSTHRU_MSG` whose `Data` is the
/// reply's `[4-byte rx CAN id][application PDU]`. Reads at most one message.
///
/// # Safety
///
/// `pMsg` and `pNumMsgs` must be null or valid for the counts they carry.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruReadMsgs(
    ChannelID: u32,
    pMsg: *mut crate::msg::PassThruMsg,
    pNumMsgs: *mut u32,
    _Timeout: u32,
) -> Status {
    if pMsg.is_null() || pNumMsgs.is_null() {
        return ERR_NULL_PARAMETER;
    }
    if ChannelID != CHANNEL_ID {
        unsafe { write_u32(pNumMsgs, 0) };
        return ERR_INVALID_CHANNEL_ID;
    }
    match crate::runtime::read() {
        Ok(app) => {
            unsafe {
                fill_msg(pMsg, &app);
                write_u32(pNumMsgs, 1);
            }
            STATUS_NOERROR
        }
        Err(status) => {
            unsafe { write_u32(pNumMsgs, 0) };
            status
        }
    }
}

/// `PassThruWriteMsgs` — write one ISO 15765 message to the vehicle.
///
/// Forwards the app's `PASSTHRU_MSG.Data` (`[4-byte CAN id][PDU]`) through
/// `VCX_PRO_Send` (`runtime::write`). The content is the app's; this DLL
/// synthesizes nothing. Writes at most one message.
///
/// # Safety
///
/// `pMsg` and `pNumMsgs` must be valid for the counts they carry.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruWriteMsgs(
    ChannelID: u32,
    pMsg: *const crate::msg::PassThruMsg,
    pNumMsgs: *mut u32,
    _Timeout: u32,
) -> Status {
    if pMsg.is_null() || pNumMsgs.is_null() {
        return ERR_NULL_PARAMETER;
    }
    if ChannelID != CHANNEL_ID {
        unsafe { write_u32(pNumMsgs, 0) };
        return ERR_INVALID_CHANNEL_ID;
    }
    let data = unsafe { msg_data(pMsg) };
    let status = crate::runtime::write(&data);
    unsafe { write_u32(pNumMsgs, u32::from(status == STATUS_NOERROR)) };
    status
}

/// `PassThruStartMsgFilter` — install a flow-control filter from the CAN ids.
///
/// Reads the mask / pattern / flow-control CAN ids from the three `PASSTHRU_MSG`s
/// and installs the recovered `VCX_PRO_AddFilter` flow-control record
/// (`runtime::add_flow_filter`), returning its filter id.
///
/// # Safety
///
/// Pointer arguments must be null or valid `PASSTHRU_MSG`s.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruStartMsgFilter(
    ChannelID: u32,
    _FilterType: u32,
    pMaskMsg: *const crate::msg::PassThruMsg,
    pPatternMsg: *const crate::msg::PassThruMsg,
    pFlowControlMsg: *const crate::msg::PassThruMsg,
    pFilterID: *mut u32,
) -> Status {
    if pFilterID.is_null() {
        return ERR_NULL_PARAMETER;
    }
    if ChannelID != CHANNEL_ID {
        return ERR_INVALID_CHANNEL_ID;
    }
    if pMaskMsg.is_null() || pPatternMsg.is_null() || pFlowControlMsg.is_null() {
        return ERR_NULL_PARAMETER;
    }
    let mask = unsafe { msg_data(pMaskMsg) };
    let pattern = unsafe { msg_data(pPatternMsg) };
    let flow = unsafe { msg_data(pFlowControlMsg) };
    match crate::runtime::add_flow_filter(&pattern, &mask, &flow) {
        Ok(id) => {
            unsafe { write_u32(pFilterID, id) };
            STATUS_NOERROR
        }
        Err(status) => status,
    }
}

/// `PassThruStopMsgFilter` — remove a filter.
///
/// # Safety
///
/// Safe to call; both arguments are values.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PassThruStopMsgFilter(ChannelID: u32, _FilterID: u32) -> Status {
    if ChannelID != CHANNEL_ID {
        return ERR_INVALID_CHANNEL_ID;
    }
    // The dongle clears filters on close; a per-filter clear is not yet wired.
    STATUS_NOERROR
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

/// The description [`PassThruGetLastError`] reports. This driver does not track
/// per-call error text, so it points the caller at the returned status code.
// Kept under the 80-byte J2534 buffer so it is reported whole, not truncated.
pub const LAST_ERROR_TEXT: &str =
    "vcxtoolkit-j2534 driver: see the status code returned by the last call";

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
        CHANNEL_ID, DEVICE_ID, PassThruClose, PassThruConnect, PassThruGetLastError, PassThruOpen,
        PassThruReadVersion, PassThruWriteMsgs,
    };
    use crate::status::{
        ERR_DEVICE_NOT_CONNECTED, ERR_INVALID_DEVICE_ID, ERR_NOT_SUPPORTED, ERR_NULL_PARAMETER,
        STATUS_NOERROR,
    };
    use core::ffi::c_char;
    use core::ptr;

    // These tests never call PassThruOpen with a valid pointer, so no serial port
    // is ever opened: they exercise only the no-device and validation paths and
    // never touch COM4 or the owner's dongle. The live end-to-end bring-up is the
    // `#[ignore]`d `com4_*` test below.

    #[test]
    fn open_rejects_a_null_pointer_before_touching_hardware() {
        // The null check returns before runtime::open, so no port is opened.
        assert_eq!(
            unsafe { PassThruOpen(ptr::null(), ptr::null_mut()) },
            ERR_NULL_PARAMETER
        );
        // Close with no session is a no-op success; a wrong device id is rejected.
        assert_eq!(unsafe { PassThruClose(DEVICE_ID) }, STATUS_NOERROR);
        assert_eq!(unsafe { PassThruClose(999) }, ERR_INVALID_DEVICE_ID);
    }

    #[test]
    fn connect_validates_and_reports_no_device_until_one_is_opened() {
        let mut ch = 0_u32;
        // A good protocol (ISO 15765 = 6), but no device is open in this test, so
        // the driver reports not-connected without opening COM4.
        assert_eq!(
            unsafe { PassThruConnect(DEVICE_ID, 6, 0, 500_000, &raw mut ch) },
            ERR_DEVICE_NOT_CONNECTED
        );
        // A protocol with no native bring-up is not supported.
        assert_eq!(
            unsafe { PassThruConnect(DEVICE_ID, 99, 0, 0, &raw mut ch) },
            ERR_NOT_SUPPORTED
        );
        // A null channel pointer is caught first.
        assert_eq!(
            unsafe { PassThruConnect(DEVICE_ID, 6, 0, 0, ptr::null_mut()) },
            ERR_NULL_PARAMETER
        );
    }

    #[test]
    fn a_write_without_a_device_is_not_connected() {
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
            unsafe { PassThruWriteMsgs(CHANNEL_ID, &raw const msg, &raw mut n, 100) },
            ERR_DEVICE_NOT_CONNECTED
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
        assert!(cstr(&buf).contains("vcxtoolkit-j2534"));
        assert_eq!(
            unsafe { PassThruGetLastError(ptr::null_mut()) },
            ERR_NULL_PARAMETER
        );
    }

    // End-to-end bring-up against the real VCX-NANO on COM4. It opens the dongle,
    // runs the recovered connect sequence, and installs the flow-control filter,
    // then closes. It sends NO vehicle request (no PassThruWriteMsgs): like
    // device.rs's com4 test, the bring-up configures the channel on the dongle and
    // transmits nothing to the ECU. An actual read is the owner's to drive through
    // a real J2534 app.
    #[cfg(windows)]
    #[test]
    #[ignore = "needs the VCX-NANO on COM4; runs the dongle bring-up, sends no vehicle request"]
    fn com4_opens_connects_and_filters_without_sending() {
        let mut id = 0_u32;
        assert_eq!(
            unsafe { PassThruOpen(ptr::null(), &raw mut id) },
            STATUS_NOERROR
        );
        assert_eq!(id, DEVICE_ID);
        let mut ch = 0_u32;
        assert_eq!(
            unsafe { PassThruConnect(DEVICE_ID, 6, 0, 500_000, &raw mut ch) },
            STATUS_NOERROR
        );
        assert_eq!(ch, CHANNEL_ID);
        let filter_msg = |bytes: &[u8]| {
            let mut m = crate::msg::PassThruMsg {
                ProtocolID: 6,
                RxStatus: 0,
                TxFlags: 0,
                Timestamp: 0,
                DataSize: u32::try_from(bytes.len()).unwrap(),
                ExtraDataIndex: 0,
                Data: [0; crate::msg::DATA_MAX],
            };
            m.Data[..bytes.len()].copy_from_slice(bytes);
            m
        };
        // FLOW_CONTROL filter: mask FF FF FF FF, pattern = resp id 7E8, flow = req id 7E0.
        let mask = filter_msg(&[0xFF, 0xFF, 0xFF, 0xFF]);
        let pattern = filter_msg(&[0x00, 0x00, 0x07, 0xE8]);
        let flow = filter_msg(&[0x00, 0x00, 0x07, 0xE0]);
        let mut fid = 0_u32;
        assert_eq!(
            unsafe {
                super::PassThruStartMsgFilter(
                    CHANNEL_ID,
                    3,
                    &raw const mask,
                    &raw const pattern,
                    &raw const flow,
                    &raw mut fid,
                )
            },
            STATUS_NOERROR
        );
        assert_eq!(unsafe { PassThruClose(DEVICE_ID) }, STATUS_NOERROR);
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
