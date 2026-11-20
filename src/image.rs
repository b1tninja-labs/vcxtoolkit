//! The firmware image header `VCXHexLoad` logs.
//!
//! Inverting `file_is_newer` made the 1.9.4.2 test fail, and the comparison was restored.
//!
//! `Head.Format` is at offset 0, `Head.Name` at offset 4, `Head.Version` at offset `0x0C`,
//! `Head.Key` at offset `0x10`, and `Head.PageNum` at offset `0x20`. This module does not
//! write an image.

const FORMAT: usize = 0;
const NAME: usize = 4;
const VERSION: usize = 0x0C;
const KEY: usize = 0x10;
const PAGE_NUM: usize = 0x20;
const HEADER_LEN: usize = PAGE_NUM + 4;

const FORMAT_MAGIC: [u8; 4] = [0xA5; 4];

/// Why a firmware image header was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageError {
    /// The slice ends before `Head.PageNum`.
    Short,
    /// `Head.Format` is not `A5 A5 A5 A5`.
    Format,
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Short => "firmware image does not reach Head.PageNum",
            Self::Format => "Head.Format is not A5 A5 A5 A5",
        })
    }
}

impl std::error::Error for ImageError {}

/// The five fields `VCXHexLoad` logs from a firmware image.
///
/// The logged fields occupy the first `0x24` bytes, through `Head.PageNum`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageHeader {
    /// `Head.Format` at offset 0. An image is `A5 A5 A5 A5`.
    pub format: [u8; 4],
    /// `Head.Name` at offset 4, eight bytes.
    pub name: [u8; 8],
    /// `Head.Version` in log order, the same order as `DeviceInfo.fw_version`.
    ///
    /// The file stores a little-endian dword at offset `0x0C`. These are that
    /// dword's bytes from the high address down. `00 01 09 01` is the dword
    /// `0x01090100`, which is `[1, 9, 1, 0]` (1.9.1.0).
    pub version: [u8; 4],
    /// `Head.Key` at offset `0x10`.
    pub key: [u8; 16],
    /// `Head.PageNum` at offset `0x20`, a little-endian `u32`.
    pub page_num: u32,
}

impl ImageHeader {
    /// `Head.Name` as UTF-8, ending at the first NUL.
    ///
    /// [`None`] when those bytes are not UTF-8. Nothing is substituted for an
    /// invalid byte.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        let end = self
            .name
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(self.name.len());
        std::str::from_utf8(&self.name[..end]).ok()
    }
}

/// Read [`ImageHeader`] from the front of a firmware image.
///
/// # Errors
///
/// Returns [`ImageError::Short`] when `bytes` ends before `Head.PageNum`, and
/// [`ImageError::Format`] when the four bytes at offset 0 are not `A5 A5 A5 A5`.
#[must_use = "a firmware header that is parsed and dropped was not read"]
pub fn read_header(bytes: &[u8]) -> Result<ImageHeader, ImageError> {
    if bytes.len() < HEADER_LEN {
        return Err(ImageError::Short);
    }
    let mut format = [0_u8; 4];
    format.copy_from_slice(&bytes[FORMAT..FORMAT + 4]);
    if format != FORMAT_MAGIC {
        return Err(ImageError::Format);
    }
    let mut name = [0_u8; 8];
    name.copy_from_slice(&bytes[NAME..NAME + 8]);
    let mut stored = [0_u8; 4];
    stored.copy_from_slice(&bytes[VERSION..VERSION + 4]);
    let mut key = [0_u8; 16];
    key.copy_from_slice(&bytes[KEY..KEY + 16]);
    let mut page = [0_u8; 4];
    page.copy_from_slice(&bytes[PAGE_NUM..PAGE_NUM + 4]);
    // The dword is little-endian on disk. Log order is the high byte first.
    let version = u32::from_le_bytes(stored).to_be_bytes();
    Ok(ImageHeader {
        format,
        name,
        version,
        key,
        page_num: u32::from_le_bytes(page),
    })
}

/// `true` when `file` is a greater version dword than `device`.
///
/// Both arrays are in log order, the same order as `DeviceInfo.fw_version`.
/// The compare is the unsigned dword that logs `Device FwVersion is New!`.
/// 1.9.1.0 is not newer than 1.9.4.2.
#[must_use]
pub fn version_is_newer(file: [u8; 4], device: [u8; 4]) -> bool {
    u32::from_be_bytes(file) > u32::from_be_bytes(device)
}

/// `true` when the file version dword is greater than `device`.
///
/// `device` is four version bytes in log order, the same order as
/// [`ImageHeader::version`].
#[must_use]
pub fn file_is_newer(file: &ImageHeader, device: [u8; 4]) -> bool {
    version_is_newer(file.version, device)
}

#[cfg(test)]
mod tests {
    use super::{ImageError, file_is_newer, read_header};

    /// First 64 bytes of `vcx.bin`.
    const VCX: [u8; 64] = [
        0xA5, 0xA5, 0xA5, 0xA5, 0x56, 0x43, 0x58, 0x2D, 0x50, 0x6C, 0x75, 0x73, 0x00, 0x01, 0x09,
        0x01, 0x57, 0x57, 0x57, 0x41, 0x4C, 0x4C, 0x53, 0x43, 0x41, 0x4E, 0x4E, 0x45, 0x52, 0x43,
        0x4F, 0x4D, 0xB5, 0x00, 0x00, 0x00, 0x06, 0x04, 0x00, 0x00, 0xF7, 0x22, 0x01, 0x00, 0xA2,
        0xE5, 0x5D, 0xDD, 0x9C, 0xD4, 0x1B, 0xDA, 0x23, 0xE3, 0x01, 0x3A, 0xFF, 0xFE, 0x6A, 0xC5,
        0x48, 0xA0, 0xE7, 0xA6,
    ];

    /// First 64 bytes of `vcxa.bin`.
    const VCXA: [u8; 64] = [
        0xA5, 0xA5, 0xA5, 0xA5, 0x56, 0x43, 0x58, 0x2D, 0x41, 0x74, 0x34, 0x30, 0x00, 0x01, 0x09,
        0x01, 0x57, 0x57, 0x57, 0x41, 0x4C, 0x4C, 0x53, 0x43, 0x41, 0x4E, 0x4E, 0x45, 0x52, 0x43,
        0x4F, 0x4D, 0xBA, 0x00, 0x00, 0x00, 0x06, 0x04, 0x00, 0x00, 0x7F, 0x60, 0x01, 0x00, 0x91,
        0x41, 0x19, 0xE6, 0x3F, 0x24, 0x05, 0x74, 0x6C, 0x86, 0xAB, 0xB4, 0xEB, 0xE6, 0xEA, 0xA3,
        0x26, 0x3E, 0xEC, 0x16,
    ];

    fn header(bytes: &[u8]) -> super::ImageHeader {
        read_header(bytes).unwrap()
    }

    #[test]
    fn vcx_bin_is_named_vcx_plus_at_1_9_1_0() {
        let file = header(&VCX);
        assert_eq!(file.format, [0xA5; 4]);
        assert_eq!(file.name(), Some("VCX-Plus"));
        assert_eq!(file.version, [1, 9, 1, 0]);
        assert_eq!(file.key, *b"WWWALLSCANNERCOM");
        assert_eq!(file.page_num, 181);
    }

    #[test]
    fn vcxa_bin_is_named_vcx_at40_at_1_9_1_0() {
        let file = header(&VCXA);
        assert_eq!(file.format, [0xA5; 4]);
        assert_eq!(file.name(), Some("VCX-At40"));
        assert_eq!(file.version, [1, 9, 1, 0]);
        assert_eq!(file.key, *b"WWWALLSCANNERCOM");
        assert_eq!(file.page_num, 186);
    }

    #[test]
    fn a_1_9_1_0_file_is_not_newer_than_1_9_4_2() {
        assert!(!file_is_newer(&header(&VCX), [1, 9, 4, 2]));
    }

    #[test]
    fn a_1_9_1_0_file_is_newer_than_1_9_0_0() {
        assert!(file_is_newer(&header(&VCX), [1, 9, 0, 0]));
    }

    #[test]
    fn the_compare_is_the_unsigned_version_dword() {
        let file = header(&VCX);
        assert!(file_is_newer(&file, [0, 9, 9, 9]));
        assert!(!file_is_newer(&file, [2, 0, 0, 0]));
        assert!(!file_is_newer(&file, [1, 9, 1, 0]));
    }

    #[test]
    fn a_short_slice_is_refused() {
        assert_eq!(read_header(&VCX[..0x23]), Err(ImageError::Short));
        assert_eq!(header(&VCX[..0x24]).page_num, 181);
    }

    #[test]
    fn a_bad_format_is_refused() {
        let mut bytes = VCX;
        bytes[3] = 0;
        assert_eq!(read_header(&bytes), Err(ImageError::Format));
    }

    #[test]
    fn the_name_stops_at_the_first_nul() {
        let mut bytes = VCX;
        bytes[7] = 0;
        bytes[8] = 0xFF;
        assert_eq!(header(&bytes).name(), Some("VCX"));
    }

    #[test]
    fn a_non_utf8_name_is_not_replaced() {
        let mut bytes = VCX;
        bytes[4] = 0xFF;
        assert_eq!(header(&bytes).name(), None);
    }

    #[test]
    fn a_different_name_is_still_read() {
        let mut bytes = VCX;
        bytes[4..12].copy_from_slice(b"VCX-NANO");
        assert_eq!(header(&bytes).name(), Some("VCX-NANO"));
    }
}
