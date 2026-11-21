//! Command headers `VCXCommSendFrame` is called with.
//!
//! No specification names these. Each third byte below is the immediate the named
//! export writes into header byte 2. Byte 0 is `0x80` and byte 1 is `0x00`. Byte 3
//! is `0x00` for the device commands and the caller's channel for the `VCX_PRO_*`
//! commands. `VCX_PRO_GetComParam` and `VCX_PRO_Clear` log a name and do not write
//! a header. `GetLicense` is the empty frame ordinal 21 sends last; that byte is
//! not an immediate in the clear exports. The eight-byte frame before it is
//! header `80 00 A0 00`. Third bytes `0x44`, `0x46`, `0x81`, `0x82`, `0x84`,
//! `0x87`, `0x91`, `0xA0`, `0xF4`, and `0xF6` have no named sender. `0x91` is
//! the `VCX_ERR_INVALID_CMD` return. `VCX_DEV_Config` sends `0x90` and
//! `0x92` through `0x99`. Its switch sends selector `0x91` nowhere.

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

#[cfg(test)]
mod tests {
    use super::{Command, ConfigCopy, config_copy, pro_send_header};

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
