//! Command headers `VCXCommSendFrame` is called with.
//!
//! No specification names these. Each third byte below is the immediate the named
//! export writes into header byte 2. Byte 0 is `0x80` and byte 1 is `0x00`. Byte 3
//! is `0x00` for the device commands and the caller's channel for the `VCX_PRO_*`
//! commands. `VCX_PRO_GetComParam` and `VCX_PRO_Clear` log a name and do not write
//! a header. `GetLicense` is the empty frame ordinal 21 sends last; that byte is
//! not an immediate in the clear exports. The eight-byte frame before it is
//! header `80 00 A0 00`. Third bytes `0x44`, `0x46`, `0x81`, `0x82`, `0x84`,
//! `0x87`, `0x91`, `0xA0`, `0xF4`, and `0xF6` have no named sender in the DLL.
//! `0x91` is the `VCX_ERR_INVALID_CMD` return. `VCX_DEV_Config` sends `0x90` and
//! `0x92` through `0x99`. Its switch sends selector `0x91` nowhere. The app
//! image lists some of those bytes in [`APP_COMMANDS`].

/// One command header, named the way the DLL logs the sender.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Command {
    /// `VCX_PRO_Open` writes `0x40`.
    ProOpen = 0x40,
    /// `VCX_PRO_Close` writes `0x41`.
    ProClose = 0x41,
    /// `VCX_PRO_Start` writes `0x42`.
    ProStart = 0x42,
    /// `VCX_PRO_Stop` writes `0x43`.
    ProStop = 0x43,
    /// `VCX_PRO_SetComParam` writes `0x45`.
    ProSetComParam = 0x45,
    /// `VCX_PRO_SetFilter` writes `0x47`.
    ProSetFilter = 0x47,
    /// `VCX_PRO_AddFilter` writes `0x48`.
    ProAddFilter = 0x48,
    /// `VCX_PRO_SetTimer` writes `0x49`.
    ProSetTimer = 0x49,
    /// `VCX_PRO_AddTimer` writes `0x4A`.
    ProAddTimer = 0x4A,
    /// `VCX_DEV_Echo` writes `0x80`.
    Echo = 0x80,
    /// `VCX_DEV_SetPinVol` writes `0x85`.
    SetPinVol = 0x85,
    /// `VCX_DEV_GetPinVol` writes `0x86`.
    GetPinVol = 0x86,
    /// `VCX_DEV_GetTime` writes `0x88`.
    GetTime = 0x88,
    /// `VCX_DEV_SetLed` writes `0x89`.
    SetLed = 0x89,
    /// `VCX_DEV_SetBeep` writes `0x8A`.
    SetBeep = 0x8A,
    /// `VCX_DEV_GetTest` writes `0x8B`.
    GetTest = 0x8B,
    /// Ordinal 21's last frame. Header `80 00 83 00`, empty payload.
    GetLicense = 0x83,
    /// `VCX_DEV_GetInfo` writes `0x8C`.
    GetInfo = 0x8C,
    /// `VCX_DEV_Reset` and `VCX_DEV_ResetEx` both write `0x8D`.
    Reset = 0x8D,
    /// `_VCX_DEV_SetExInfo@8` writes `0x8E`.
    SetExInfo = 0x8E,
    /// `VCX_DEV_SynExInfo` writes `0x8F`.
    SynExInfo = 0x8F,
    /// `VCX_CONFIG_SET_FRAME` writes `0x90`.
    SetFrame = 0x90,
    /// `VCX_CONFIG_SET_COM` writes `0x92`.
    SetCom = 0x92,
    /// `VCX_CONFIG_GET_COM` writes `0x93`.
    GetCom = 0x93,
    /// `VCX_CONFIG_SET_BT` writes `0x94`.
    SetBt = 0x94,
    /// `VCX_CONFIG_GET_BT` writes `0x95`.
    GetBt = 0x95,
    /// `VCX_CONFIG_SET_LAN` writes `0x96`.
    SetLan = 0x96,
    /// `VCX_CONFIG_GET_LAN` writes `0x97`.
    GetLan = 0x97,
    /// `VCX_CONFIG_SET_WLAN` writes `0x98`.
    SetWlan = 0x98,
    /// `VCX_CONFIG_GET_WLAN` writes `0x99`.
    GetWlan = 0x99,
    /// `VCX_DFU_SetMode` writes `0xF0`.
    DfuSetMode = 0xF0,
    /// `VCX_DFU_GetMode` writes `0xF1`.
    DfuGetMode = 0xF1,
    /// `VCX_DFU_GetInfo` writes `0xF2`.
    DfuGetInfo = 0xF2,
    /// `VCX_DFU_ErChip` writes `0xF3`.
    DfuErChip = 0xF3,
    /// `VCX_DFU_WrFlash` writes `0xF5`.
    DfuWrFlash = 0xF5,
    /// `VCX_DFU_WrSerial` writes `0xF7`.
    DfuWrSerial = 0xF7,
}

impl Command {
    /// The third header byte.
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Header bytes `80 00 <code> fourth`.
    #[must_use]
    pub const fn header(self, fourth: u8) -> [u8; 4] {
        [0x80, 0x00, self.code(), fourth]
    }

    /// The command whose third byte is `code`, if one of the named senders writes it.
    #[must_use]
    pub const fn from_byte(code: u8) -> Option<Self> {
        Some(match code {
            0x40 => Self::ProOpen,
            0x41 => Self::ProClose,
            0x42 => Self::ProStart,
            0x43 => Self::ProStop,
            0x45 => Self::ProSetComParam,
            0x47 => Self::ProSetFilter,
            0x48 => Self::ProAddFilter,
            0x49 => Self::ProSetTimer,
            0x4A => Self::ProAddTimer,
            0x80 => Self::Echo,
            0x85 => Self::SetPinVol,
            0x86 => Self::GetPinVol,
            0x88 => Self::GetTime,
            0x89 => Self::SetLed,
            0x8A => Self::SetBeep,
            0x8B => Self::GetTest,
            0x83 => Self::GetLicense,
            0x8C => Self::GetInfo,
            0x8D => Self::Reset,
            0x8E => Self::SetExInfo,
            0x8F => Self::SynExInfo,
            0x90 => Self::SetFrame,
            0x92 => Self::SetCom,
            0x93 => Self::GetCom,
            0x94 => Self::SetBt,
            0x95 => Self::GetBt,
            0x96 => Self::SetLan,
            0x97 => Self::GetLan,
            0x98 => Self::SetWlan,
            0x99 => Self::GetWlan,
            0xF0 => Self::DfuSetMode,
            0xF1 => Self::DfuGetMode,
            0xF2 => Self::DfuGetInfo,
            0xF3 => Self::DfuErChip,
            0xF5 => Self::DfuWrFlash,
            0xF7 => Self::DfuWrSerial,
            _ => return None,
        })
    }

    /// [`from_byte`](Self::from_byte) when byte 0 is `0x80` and byte 1 is `0x00`.
    ///
    /// The fourth byte is the channel and is not part of the name.
    #[must_use]
    pub const fn from_header(header: [u8; 4]) -> Option<Self> {
        if header[0] != 0x80 || header[1] != 0x00 {
            return None;
        }
        Self::from_byte(header[2])
    }
}

/// Header `VCX_PRO_Send` writes.
///
/// Byte 1 and byte 3 are the caller's arguments. Byte 2 is `0x00`, which is not
/// one of the named third-byte commands.
#[must_use]
pub const fn pro_send_header(second: u8, fourth: u8) -> [u8; 4] {
    [0x80, second, 0x00, fourth]
}

/// How many bytes `VCX_DEV_Config` copies for one selector.
///
/// `send` is the payload copied from the caller buffer. `recv` is how many
/// reply-payload bytes are copied back into that buffer, starting at the
/// first payload byte. The received length must be at least `recv + 1`.
/// The extra byte is not copied. Selector `0x91` is not a copy: the switch
/// returns `0x91` and does not send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigCopy {
    /// Payload bytes taken from the caller buffer. Zero sends an empty payload.
    pub send: usize,
    /// Reply payload bytes written back. The reply must be at least one byte longer.
    pub recv: usize,
}

/// The copy lengths for a `VCX_DEV_Config` selector.
///
/// The selector is header byte 2. A command this function does not return is
/// not one of those nine sends.
#[must_use]
pub const fn config_copy(command: Command) -> Option<ConfigCopy> {
    let (send, recv) = match command {
        Command::SetFrame => (2, 0),
        Command::SetCom => (8, 0),
        Command::GetCom => (0, 8),
        Command::SetBt => (0x28, 0),
        Command::GetBt => (0, 0x28),
        Command::SetLan => (0x0C, 0),
        Command::GetLan => (0, 0x0C),
        Command::SetWlan => (0x2C, 0),
        Command::GetWlan => (0, 0x2C),
        _ => return None,
    };
    Some(ConfigCopy { send, recv })
}

/// Payload `VCX_DEV_SetPinVol` sends: the pin byte, then the dword big-endian.
#[must_use]
pub const fn set_pin_vol(pin: u8, value: u32) -> [u8; 5] {
    let bytes = value.to_be_bytes();
    [pin, bytes[0], bytes[1], bytes[2], bytes[3]]
}

/// Payload `VCX_DEV_GetPinVol` sends. The log name of that export is `VCX_DEV_GetDlcVol`.
#[must_use]
pub const fn get_pin_vol(pin: u8) -> [u8; 1] {
    [pin]
}

/// Dword `VCX_DEV_GetPinVol` stores when the reply payload is at least 5 bytes.
///
/// `value` is payload bytes 1 through 4, big-endian. `index` is payload byte 5
/// when the payload is at least 6 bytes, and 0 otherwise. The function stores
/// `value` in `PinVolBackups[index]`. A shorter reply does not parse; the
/// function returns the backup it already holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinReading {
    /// First payload byte, which the function returns.
    pub status: u8,
    /// The dword written to the caller.
    pub value: u32,
    /// The `PinVolBackups` slot.
    pub index: u8,
}

/// The parsed `GetPinVol` reply.
///
/// Returns `None` when `payload` is shorter than 5 bytes.
#[must_use]
pub const fn pin_reading(payload: &[u8]) -> Option<PinReading> {
    if payload.len() < 5 {
        return None;
    }
    let value = u32::from_be_bytes([payload[1], payload[2], payload[3], payload[4]]);
    let index = if payload.len() >= 6 { payload[5] } else { 0 };
    Some(PinReading {
        status: payload[0],
        value,
        index,
    })
}

/// What the reply wait at `0x7B2F4C90` does with one queued command.
///
/// The wait is given the command byte of the request it just sent. The
/// timeout it adds is the dword at `device+0x20`. A miss returns 0, and the
/// caller returns `0x83`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitReply {
    /// The queued command is the one the request sent. The wait returns 1.
    Ready,
    /// Command `0x86` arrived while waiting for a different command, and the
    /// payload is at least 5 bytes. The dword and the index match
    /// [`pin_reading`]. The wait stores that backup and keeps waiting.
    Pin {
        /// Payload bytes 1 through 4, big-endian.
        value: u32,
        /// Payload byte 5, or 0 when the payload is shorter than 6 bytes.
        index: u8,
    },
    /// Any other queued command, including a `0x86` payload shorter than 5
    /// bytes. The wait keeps waiting.
    Again,
}

/// Classify one queued reply while a request is waiting.
///
/// A command equal to `expected` finishes the wait. That comparison runs
/// before the `0x86` backup, so a wait for `0x86` itself does not take the
/// backup path.
#[must_use]
pub fn wait_reply(expected: u8, command: u8, payload: &[u8]) -> WaitReply {
    if command == expected {
        return WaitReply::Ready;
    }
    if command == Command::GetPinVol.code() {
        if let Some(reading) = pin_reading(payload) {
            return WaitReply::Pin {
                value: reading.value,
                index: reading.index,
            };
        }
    }
    WaitReply::Again
}

/// Dword `VCX_DEV_GetTime` stores. The request payload is empty.
///
/// The reply must be at least 5 bytes. `status` is the first payload byte,
/// which the function returns. `value` is the next four bytes, big-endian.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceTime {
    /// First payload byte.
    pub status: u8,
    /// The dword written to the caller.
    pub value: u32,
}

/// The parsed `GetTime` reply.
///
/// Returns `None` when `payload` is shorter than 5 bytes.
#[must_use]
pub const fn device_time(payload: &[u8]) -> Option<DeviceTime> {
    if payload.len() < 5 {
        return None;
    }
    Some(DeviceTime {
        status: payload[0],
        value: u32::from_be_bytes([payload[1], payload[2], payload[3], payload[4]]),
    })
}

/// Bytes `VCX_DEV_SetLed` copies from the caller pointer, in that order.
#[must_use]
pub const fn set_led(bytes: [u8; 4]) -> [u8; 4] {
    bytes
}

/// Bytes `VCX_DEV_SetBeep` copies from the caller pointer, in that order.
#[must_use]
pub const fn set_beep(bytes: [u8; 2]) -> [u8; 2] {
    bytes
}

/// Reply length `VCX_DEV_GetTest` requires.
///
/// The first payload byte is the return. The next [`TEST_COPY`] bytes are
/// copied to the caller.
pub const TEST_REPLY: usize = 0x107;

/// Bytes `VCX_DEV_GetTest` copies, starting at payload byte 1.
pub const TEST_COPY: usize = 0x106;

/// The status byte and the copied body of a `GetTest` reply.
///
/// Returns `None` when `payload` is shorter than [`TEST_REPLY`].
#[must_use]
pub const fn test_record(payload: &[u8]) -> Option<(u8, &[u8])> {
    const _: () = assert!(TEST_REPLY == TEST_COPY + 1);
    if payload.len() < TEST_REPLY {
        return None;
    }
    Some((payload[0], payload.split_at(1).1.split_at(TEST_COPY).0))
}

/// Payload `VCX_DEV_Reset` sends. `VCX_DEV_ResetEx` sends [`reset_ex`] instead.
#[must_use]
pub const fn reset_payload() -> [u8; 0] {
    []
}

/// The one byte `VCX_DEV_ResetEx` sends. The header byte is still `0x8D`.
#[must_use]
pub const fn reset_ex(byte: u8) -> [u8; 1] {
    [byte]
}

/// First dword of a buffer `VCX_DEV_SynExInfo` sends as extended info.
///
/// The load is a little-endian dword. The four bytes are `16 16 16 16`.
pub const EXINFO_MARK: u32 = 0x1616_1616;

/// Bytes copied when that dword is present, and bytes filled on a get.
pub const EXINFO_LEN: usize = 0x80;

/// Reply length that carries an extended-info body: one status byte plus [`EXINFO_LEN`].
pub const EXINFO_REPLY: usize = 0x81;

/// Why an extended-info buffer was not built or applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExInfoError {
    /// A marked buffer, or a get buffer, is shorter than [`EXINFO_LEN`].
    /// An empty reply is also this error: the DLL returns `0x83` and does not copy.
    Short,
}

impl std::fmt::Display for ExInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("extended-info buffer is shorter than the copy")
    }
}

impl std::error::Error for ExInfoError {}

/// Payload `VCX_DEV_SynExInfo` sends.
///
/// A buffer whose first dword is [`EXINFO_MARK`] is logged `Set ExInfo` and
/// the payload is its first [`EXINFO_LEN`] bytes. Any other buffer is logged
/// `Get ExInfo` and the payload is four zero bytes. The buffer itself is not
/// copied on that path.
///
/// # Errors
///
/// Returns [`ExInfoError::Short`] when the buffer starts with the mark and is
/// shorter than [`EXINFO_LEN`].
#[must_use = "an extended-info payload that is dropped was not sent"]
pub fn syn_ex_info(buffer: &[u8]) -> Result<Vec<u8>, ExInfoError> {
    if buffer.len() >= 4
        && u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) == EXINFO_MARK
    {
        if buffer.len() < EXINFO_LEN {
            return Err(ExInfoError::Short);
        }
        return Ok(buffer[..EXINFO_LEN].to_vec());
    }
    Ok(vec![0, 0, 0, 0])
}

/// Apply a `SynExInfo` reply to `buffer`.
///
/// A buffer that already starts with [`EXINFO_MARK`] is left as it is. Any
/// other buffer of [`EXINFO_LEN`] bytes is filled with zero. When the reply is
/// exactly [`EXINFO_REPLY`] bytes, the `0x80` bytes after the status byte
/// replace that fill. The status byte is the return.
///
/// # Errors
///
/// Returns [`ExInfoError::Short`] when `payload` is empty, or when a buffer
/// that does not start with the mark is shorter than [`EXINFO_LEN`].
#[must_use = "an extended-info status that is dropped was not applied"]
pub fn apply_ex_info(buffer: &mut [u8], payload: &[u8]) -> Result<u8, ExInfoError> {
    if payload.is_empty() {
        return Err(ExInfoError::Short);
    }
    let status = payload[0];
    if buffer.len() >= 4
        && u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) == EXINFO_MARK
    {
        return Ok(status);
    }
    if buffer.len() < EXINFO_LEN {
        return Err(ExInfoError::Short);
    }
    buffer[..EXINFO_LEN].fill(0);
    if payload.len() == EXINFO_REPLY {
        buffer[..EXINFO_LEN].copy_from_slice(&payload[1..EXINFO_REPLY]);
    }
    Ok(status)
}

/// Command bytes in the decrypted app image, in table order.
///
/// The table is at file offset `0x286C6` of `vcxa-1.9.4.2-plain.bin`. Each
/// record is the command byte, three zero bytes, and a word. `0xF2`, `0xF3`,
/// and `0xF5` are not in this table.
pub const APP_COMMANDS: [u8; 45] = [
    0x40, 0x41, 0x42, 0x43, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B, 0x4C, 0x80, 0x81, 0x82, 0x83,
    0x84, 0x85, 0x86, 0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x8D, 0x8E, 0x8F, 0x90, 0x92, 0x93, 0x94, 0x95,
    0x96, 0x97, 0x98, 0x99, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xF0, 0xF1, 0xF7, 0xFF,
];

#[cfg(test)]
mod tests {
    use super::{
        APP_COMMANDS, Command, ConfigCopy, DeviceTime, EXINFO_LEN, EXINFO_MARK, EXINFO_REPLY,
        ExInfoError, PinReading, TEST_COPY, TEST_REPLY, WaitReply, apply_ex_info, config_copy,
        device_time, get_pin_vol, pin_reading, pro_send_header, reset_ex, reset_payload, set_beep,
        set_led, set_pin_vol, syn_ex_info, test_record, wait_reply,
    };

    #[test]
    fn the_app_image_lists_get_info_and_not_dfu_get_info() {
        assert!(APP_COMMANDS.contains(&0x8C));
        assert!(APP_COMMANDS.contains(&0xF0));
        assert!(APP_COMMANDS.contains(&0xF1));
        assert!(APP_COMMANDS.contains(&0xF7));
        assert!(!APP_COMMANDS.contains(&0xF2));
        assert!(!APP_COMMANDS.contains(&0xF3));
        assert!(!APP_COMMANDS.contains(&0xF5));
        assert!(!APP_COMMANDS.contains(&0x44));
    }

    fn every() -> [Command; 36] {
        [
            Command::ProOpen,
            Command::ProClose,
            Command::ProStart,
            Command::ProStop,
            Command::ProSetComParam,
            Command::ProSetFilter,
            Command::ProAddFilter,
            Command::ProSetTimer,
            Command::ProAddTimer,
            Command::Echo,
            Command::SetPinVol,
            Command::GetPinVol,
            Command::GetTime,
            Command::SetLed,
            Command::SetBeep,
            Command::GetTest,
            Command::GetLicense,
            Command::GetInfo,
            Command::Reset,
            Command::SetExInfo,
            Command::SynExInfo,
            Command::SetFrame,
            Command::SetCom,
            Command::GetCom,
            Command::SetBt,
            Command::GetBt,
            Command::SetLan,
            Command::GetLan,
            Command::SetWlan,
            Command::GetWlan,
            Command::DfuSetMode,
            Command::DfuGetMode,
            Command::DfuGetInfo,
            Command::DfuErChip,
            Command::DfuWrFlash,
            Command::DfuWrSerial,
        ]
    }

    #[test]
    fn every_named_header_round_trips() {
        for command in every() {
            match command {
                Command::ProOpen => assert_eq!(command.code(), 0x40),
                Command::ProClose => assert_eq!(command.code(), 0x41),
                Command::ProStart => assert_eq!(command.code(), 0x42),
                Command::ProStop => assert_eq!(command.code(), 0x43),
                Command::ProSetComParam => assert_eq!(command.code(), 0x45),
                Command::ProSetFilter => assert_eq!(command.code(), 0x47),
                Command::ProAddFilter => assert_eq!(command.code(), 0x48),
                Command::ProSetTimer => assert_eq!(command.code(), 0x49),
                Command::ProAddTimer => assert_eq!(command.code(), 0x4A),
                Command::Echo => assert_eq!(command.code(), 0x80),
                Command::SetPinVol => assert_eq!(command.code(), 0x85),
                Command::GetPinVol => assert_eq!(command.code(), 0x86),
                Command::GetTime => assert_eq!(command.code(), 0x88),
                Command::SetLed => assert_eq!(command.code(), 0x89),
                Command::SetBeep => assert_eq!(command.code(), 0x8A),
                Command::GetTest => assert_eq!(command.code(), 0x8B),
                Command::GetLicense => assert_eq!(command.code(), 0x83),
                Command::GetInfo => assert_eq!(command.code(), 0x8C),
                Command::Reset => assert_eq!(command.code(), 0x8D),
                Command::SetExInfo => assert_eq!(command.code(), 0x8E),
                Command::SynExInfo => assert_eq!(command.code(), 0x8F),
                Command::SetFrame => assert_eq!(command.code(), 0x90),
                Command::SetCom => assert_eq!(command.code(), 0x92),
                Command::GetCom => assert_eq!(command.code(), 0x93),
                Command::SetBt => assert_eq!(command.code(), 0x94),
                Command::GetBt => assert_eq!(command.code(), 0x95),
                Command::SetLan => assert_eq!(command.code(), 0x96),
                Command::GetLan => assert_eq!(command.code(), 0x97),
                Command::SetWlan => assert_eq!(command.code(), 0x98),
                Command::GetWlan => assert_eq!(command.code(), 0x99),
                Command::DfuSetMode => assert_eq!(command.code(), 0xF0),
                Command::DfuGetMode => assert_eq!(command.code(), 0xF1),
                Command::DfuGetInfo => assert_eq!(command.code(), 0xF2),
                Command::DfuErChip => assert_eq!(command.code(), 0xF3),
                Command::DfuWrFlash => assert_eq!(command.code(), 0xF5),
                Command::DfuWrSerial => assert_eq!(command.code(), 0xF7),
            }
            assert_eq!(Command::from_byte(command.code()), Some(command));
            let header = command.header(0);
            assert_eq!(header, [0x80, 0x00, command.code(), 0x00]);
            assert_eq!(Command::from_header(header), Some(command));
        }
    }

    #[test]
    fn the_channel_byte_is_not_the_name() {
        assert_eq!(Command::ProOpen.header(0x11), [0x80, 0x00, 0x40, 0x11]);
        assert_eq!(
            Command::from_header([0x80, 0x00, 0x40, 0x11]),
            Some(Command::ProOpen)
        );
        assert_eq!(Command::from_header([0x81, 0x00, 0x40, 0x00]), None);
        assert_eq!(Command::from_header([0x80, 0x01, 0x40, 0x00]), None);
    }

    #[test]
    fn config_copies_the_selector_lengths() {
        assert_eq!(
            config_copy(Command::SetFrame),
            Some(ConfigCopy { send: 2, recv: 0 })
        );
        assert_eq!(
            config_copy(Command::SetCom),
            Some(ConfigCopy { send: 8, recv: 0 })
        );
        assert_eq!(
            config_copy(Command::GetCom),
            Some(ConfigCopy { send: 0, recv: 8 })
        );
        assert_eq!(
            config_copy(Command::SetBt),
            Some(ConfigCopy {
                send: 0x28,
                recv: 0
            })
        );
        assert_eq!(
            config_copy(Command::GetBt),
            Some(ConfigCopy {
                send: 0,
                recv: 0x28
            })
        );
        assert_eq!(
            config_copy(Command::SetLan),
            Some(ConfigCopy {
                send: 0x0C,
                recv: 0
            })
        );
        assert_eq!(
            config_copy(Command::GetLan),
            Some(ConfigCopy {
                send: 0,
                recv: 0x0C
            })
        );
        assert_eq!(
            config_copy(Command::SetWlan),
            Some(ConfigCopy {
                send: 0x2C,
                recv: 0
            })
        );
        assert_eq!(
            config_copy(Command::GetWlan),
            Some(ConfigCopy {
                send: 0,
                recv: 0x2C
            })
        );
        assert_eq!(config_copy(Command::GetInfo), None);
        assert_eq!(config_copy(Command::Echo), None);
    }

    #[test]
    fn pin_voltage_and_time_are_big_endian_dwords() {
        assert_eq!(
            set_pin_vol(0x10, 0x1122_3344),
            [0x10, 0x11, 0x22, 0x33, 0x44]
        );
        assert_eq!(get_pin_vol(0x10), [0x10]);
        assert_eq!(
            pin_reading(&[0x00, 0x11, 0x22, 0x33, 0x44]),
            Some(PinReading {
                status: 0,
                value: 0x1122_3344,
                index: 0,
            })
        );
        assert_eq!(
            pin_reading(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x07]),
            Some(PinReading {
                status: 0,
                value: 0x1122_3344,
                index: 7,
            })
        );
        assert_eq!(pin_reading(&[0x00, 0x11, 0x22, 0x33]), None);
        assert_eq!(
            device_time(&[0x01, 0x00, 0x00, 0x01, 0x02]),
            Some(DeviceTime {
                status: 1,
                value: 0x0102,
            })
        );
        assert_eq!(device_time(&[0x01, 0x00, 0x00, 0x01]), None);
    }

    #[test]
    fn an_unexpected_pin_reply_is_stored_and_the_wait_continues() {
        let payload = [0x00, 0x11, 0x22, 0x33, 0x44, 0x07];
        assert_eq!(wait_reply(0x8C, 0x8C, &[]), WaitReply::Ready);
        assert_eq!(wait_reply(0x86, 0x86, &payload), WaitReply::Ready);
        assert_eq!(
            wait_reply(0x8C, 0x86, &payload),
            WaitReply::Pin {
                value: 0x1122_3344,
                index: 0x07,
            }
        );
        assert_eq!(
            wait_reply(0x8C, 0x86, &[0x00, 0x11, 0x22, 0x33]),
            WaitReply::Again
        );
        assert_eq!(wait_reply(0x8C, 0x80, &[0x01]), WaitReply::Again);
    }

    #[test]
    fn led_beep_reset_and_test_keep_the_caller_bytes() {
        assert_eq!(set_led([1, 2, 3, 4]), [1, 2, 3, 4]);
        assert_eq!(set_beep([5, 6]), [5, 6]);
        assert_eq!(reset_payload(), []);
        assert_eq!(reset_ex(0xA5), [0xA5]);
        let mut payload = vec![0x7E_u8; TEST_REPLY];
        payload[0] = 0x02;
        payload[1] = 0x03;
        let (status, body) = test_record(&payload).unwrap();
        assert_eq!(status, 0x02);
        assert_eq!(body.len(), TEST_COPY);
        assert_eq!(body[0], 0x03);
        assert_eq!(test_record(&payload[..TEST_REPLY - 1]), None);
    }

    #[test]
    fn a_marked_buffer_is_sent_and_an_unmarked_one_asks_for_it() {
        let mut marked = vec![0_u8; EXINFO_LEN];
        marked[..4].copy_from_slice(&EXINFO_MARK.to_le_bytes());
        marked[4] = 0xAB;
        assert_eq!(syn_ex_info(&marked).unwrap(), marked);
        assert_eq!(syn_ex_info(&[0x16, 0x16, 0x16]).unwrap(), [0, 0, 0, 0]);
        assert_eq!(syn_ex_info(&[1, 2, 3, 4, 5]).unwrap(), [0, 0, 0, 0]);
        assert_eq!(
            syn_ex_info(&marked[..EXINFO_LEN - 1]),
            Err(ExInfoError::Short)
        );

        let mut kept = marked.clone();
        assert_eq!(apply_ex_info(&mut kept, &[0x01, 0xFF]).unwrap(), 0x01);
        assert_eq!(kept, marked);

        let mut got = vec![0xEE_u8; EXINFO_LEN];
        let mut reply = vec![0_u8; EXINFO_REPLY];
        reply[0] = 0x01;
        reply[1] = 0xAB;
        assert_eq!(apply_ex_info(&mut got, &reply).unwrap(), 0x01);
        assert_eq!(got[0], 0xAB);
        assert_eq!(&got[1..], &reply[2..]);

        let mut zeroed = vec![0xEE_u8; EXINFO_LEN];
        assert_eq!(apply_ex_info(&mut zeroed, &[0x04]).unwrap(), 0x04);
        assert_eq!(zeroed, vec![0; EXINFO_LEN]);
        assert_eq!(apply_ex_info(&mut zeroed, &[]), Err(ExInfoError::Short));
        assert_eq!(zeroed, vec![0; EXINFO_LEN]);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "needs the CH343 dongle on COM4"]
    fn com4_reads_the_device_time_and_pin_voltage() {
        let port = crate::open_usb(4, 0).expect("COM4");
        let timeout = std::time::Duration::from_secs(3);
        let time = crate::exchange(&port, Command::GetTime.header(0), &[], timeout).expect("time");
        let clock = device_time(&time.payload).expect("time payload");
        let pin = crate::exchange(
            &port,
            Command::GetPinVol.header(0),
            &get_pin_vol(0x10),
            timeout,
        )
        .expect("pin");
        let reading = pin_reading(&pin.payload).expect("pin payload");
        assert_eq!(time.header, [0x80, 0x00, 0x88, 0x00]);
        assert_eq!(pin.header, [0x80, 0x00, 0x86, 0x00]);
        assert_eq!(clock.status, 0);
        assert_eq!(reading.status, 0);
        assert_eq!(reading.index, 0x10);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "needs the CH343 dongle on COM4"]
    fn com4_reads_extended_info() {
        let port = crate::open_usb(4, 0).expect("COM4");
        let query = syn_ex_info(&[0]).unwrap();
        let reply = crate::exchange(
            &port,
            Command::SynExInfo.header(0),
            &query,
            std::time::Duration::from_secs(3),
        )
        .expect("exinfo");
        let mut buffer = vec![0xEE_u8; EXINFO_LEN];
        let status = apply_ex_info(&mut buffer, &reply.payload).expect("apply");
        assert_eq!(reply.header, [0x80, 0x00, 0x8F, 0x00]);
        assert_eq!(reply.payload.len(), EXINFO_REPLY);
        assert_eq!(status, 0);
        assert_eq!(buffer, vec![0; EXINFO_LEN]);
    }

    #[test]
    fn bytes_with_no_sender_are_unnamed() {
        for code in [
            0x00, 0x44, 0x46, 0x81, 0x82, 0x84, 0x87, 0x91, 0xA0, 0xF4, 0xF6,
        ] {
            assert_eq!(Command::from_byte(code), None, "{code:#x}");
        }
    }

    #[test]
    fn get_info_and_echo_match_the_frames_already_packed() {
        assert_eq!(Command::GetInfo.header(0), [0x80, 0x00, 0x8C, 0x00]);
        assert_eq!(Command::from_byte(0x8C), Some(Command::GetInfo));
        assert_eq!(Command::Echo.header(0), [0x80, 0x00, 0x80, 0x00]);
    }

    #[test]
    fn pro_send_keeps_the_two_caller_bytes() {
        assert_eq!(pro_send_header(0x12, 0x34), [0x80, 0x12, 0x00, 0x34]);
        assert_eq!(Command::from_header(pro_send_header(0x12, 0x34)), None);
    }
}
