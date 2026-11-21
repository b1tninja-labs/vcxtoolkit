//! DFU payloads `VCX_DFU_*` writes, and the page rewrite that runs before `WrFlash`.
//!
//! The command bytes are ordinals 33 through 38 of `vcx-reconstructed.dll`.
//! `rewrite_page` is `FUN_7b2fab90`. An emulation of that function on two
//! little-endian words and a zero key produced `c6 06 71 df 64 4e 44 19`.
//! Nothing here opens a port.

use crate::Command;
use crate::frame::MAX_PAYLOAD;

/// `GetMode` byte that means the dongle is already in the boot mode.
pub const BOOT_MODE: u8 = 0xA5;

/// `SetMode` byte that leaves the boot mode.
pub const LEAVE_MODE: u8 = 0;

/// Initial value of the register that polls `GetMode` until the byte is [`LEAVE_MODE`].
///
/// That path is the one taken when the serial pointer slot is 0. `ESI` starts
/// at 10. The body runs again when `ESI` is already 0, so the maximum number
/// of passes is [`poll_passes`] of this value. The prologue also stores 10 in
/// the slot the other path tests, so the instructions that have a pointer take
/// that other path.
pub const LEAVE_POLLS: u8 = 10;

/// Initial value of the boot-mode poll counter.
///
/// After `SetMode 0xA5`, `AutoUpdate` stores 10 at `[esp+0x20]` while three
/// log arguments are still pushed. `add esp, 0xC` makes that same slot
/// `[esp+0x14]`. The loop reads it there. [`poll_passes`] is how many times
/// the body runs when the reply byte is never `0xA5`.
pub const BOOT_POLLS: u8 = 10;

/// Milliseconds passed to `Sleep` before each polled `GetMode`.
///
/// The call is `[0x7B2FE0B4]`, and that slot is `Sleep`. The immediate is `0x1F4`.
pub const POLL_WAIT: u32 = 0x1F4;

/// Text of the `MessageBoxA` the name refusal shows.
pub const NAME_DIALOG: &str = "Device Firmware Wrong!";

/// Caption of that dialog.
pub const NAME_DIALOG_CAPTION: &str = "Error";

/// `MessageBoxA` type argument on that refusal. The slot is `[0x7B2FE13C]`.
pub const NAME_DIALOG_FLAGS: u32 = 0x1010;

/// Passes of a poll whose counter starts at `initial` and stops after the pass
/// where the value before the decrement was 0.
#[must_use]
pub const fn poll_passes(initial: u8) -> u8 {
    initial.saturating_add(1)
}

/// Offset of the 8-byte name inside the buffer `VCX_DFU_GetInfo` fills.
pub const DFU_NAME_OFFSET: usize = 0x20;

/// Round constant `FUN_7b2fab90` subtracts. Modulo `2^32` this adds `0x9E3779B9`.
const ROUND: u32 = 0x61C8_8647;

/// One DFU command whose payload the DLL writes in full.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Built {
    /// Header byte 2.
    pub command: Command,
    /// Bytes after the 4-byte header. Channel byte 3 is 0.
    pub payload: Vec<u8>,
}

impl Built {
    /// Header `80 00 <command> 00`.
    #[must_use]
    pub const fn header(&self) -> [u8; 4] {
        self.command.header(0)
    }
}

/// Why a DFU payload was not built.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DfuError {
    /// The payload is longer than [`MAX_PAYLOAD`].
    Length,
}

impl std::fmt::Display for DfuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Length => "DFU payload is longer than 0x1026",
        })
    }
}

impl std::error::Error for DfuError {}

/// Why `FUN_7b2fab90` was not applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RewriteError {
    /// The word count is 0. The DLL indexes `n - 1`.
    Empty,
    /// `data` does not cover the words the length asks for.
    Short,
}

impl std::fmt::Display for RewriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "page rewrite has no words",
            Self::Short => "page is shorter than the word count",
        })
    }
}

impl std::error::Error for RewriteError {}

fn built(command: Command, payload: Vec<u8>) -> Built {
    Built { command, payload }
}

/// `VCX_DFU_SetMode`: header byte `0xF0` and one payload byte.
#[must_use]
pub fn set_mode(mode: u8) -> Built {
    built(Command::DfuSetMode, vec![mode])
}

/// `VCX_DFU_GetMode`: header byte `0xF1` and an empty payload.
#[must_use]
pub fn get_mode() -> Built {
    built(Command::DfuGetMode, Vec::new())
}

/// `VCX_DFU_GetInfo`: header byte `0xF2` and an empty payload.
///
/// The reply length must be at least `0x21`. The 8-byte name sits at
/// [`DFU_NAME_OFFSET`] of the buffer the DLL fills, which skips the reply's
/// first payload byte.
#[must_use]
pub fn dfu_get_info() -> Built {
    built(Command::DfuGetInfo, Vec::new())
}

/// `VCX_DFU_ErChip`: header byte `0xF3` and an empty payload.
#[must_use]
pub fn erase_chip() -> Built {
    built(Command::DfuErChip, Vec::new())
}

/// `VCX_DFU_WrFlash`: header byte `0xF5` and `page` as the payload.
///
/// Ordinal 37 copies the buffer it is given. It does not insert a page index.
/// `AutoUpdate`'s call site pushes the image's first dword as the length, which
/// is `0xA5A5A5A5` for every image [`read_header`](crate::read_header) accepts.
/// That length is above [`MAX_PAYLOAD`], so this function does not build it.
///
/// # Errors
///
/// Returns [`DfuError::Length`] when `page` is longer than [`MAX_PAYLOAD`].
#[must_use = "a flash payload that is built and dropped was not checked"]
pub fn write_flash(page: &[u8]) -> Result<Built, DfuError> {
    if page.len() > MAX_PAYLOAD {
        return Err(DfuError::Length);
    }
    Ok(built(Command::DfuWrFlash, page.to_vec()))
}

/// `VCX_DFU_WrSerial`: header byte `0xF7` and these 8 bytes.
///
/// The DLL returns `0x94` and does not send when the length is not 8.
/// `AutoUpdate`'s pointer is not a recovered serial, so a plan does not call this.
#[must_use]
pub fn write_serial(bytes: &[u8; 8]) -> Built {
    built(Command::DfuWrSerial, bytes.to_vec())
}

/// `true` when `VCXCommSendFrame` would accept `length` as a payload size.
#[must_use]
pub fn flash_length_fits(length: u32) -> bool {
    usize::try_from(length).is_ok_and(|len| len <= MAX_PAYLOAD)
}

/// `true` when a `GetMode` reply byte is the boot mode.
#[must_use]
pub const fn in_boot(mode: u8) -> bool {
    mode == BOOT_MODE
}

/// `true` when a `GetMode` reply byte is the mode `SetMode 0` leaves.
#[must_use]
pub const fn left_boot(mode: u8) -> bool {
    mode == LEAVE_MODE
}

/// `true` when the names differ and the device name begins `VCX`.
///
/// `AutoUpdate` compares 8 bytes. On a difference it checks the word `VC` and the
/// next byte `X`, then calls `SetMode` 0 and returns `0xFF`.
#[must_use]
pub fn name_blocks(device: &[u8; 8], image: &[u8; 8]) -> bool {
    device != image && device.starts_with(b"VCX")
}

/// Rewrite `data` the way `FUN_7b2fab90` rewrites a page payload.
///
/// `byte_len` is the record's first dword. The word count is `byte_len >> 2`.
/// A leftover 1, 2, or 3 bytes stay as they are. `key` is 16 bytes, four
/// little-endian words. The call site passes the DFU `GetInfo` buffer, not
/// `Head.Key`.
///
/// # Errors
///
/// Returns [`RewriteError::Empty`] when the word count is 0, and
/// [`RewriteError::Short`] when `data` does not cover those words.
#[must_use = "a rewrite whose error is dropped was not applied"]
pub fn rewrite_page(data: &mut [u8], byte_len: u32, key: &[u8; 16]) -> Result<(), RewriteError> {
    let words = usize::try_from(byte_len >> 2).map_err(|_| RewriteError::Short)?;
    if words == 0 {
        return Err(RewriteError::Empty);
    }
    let Some(used) = words.checked_mul(4) else {
        return Err(RewriteError::Short);
    };
    if data.len() < used {
        return Err(RewriteError::Short);
    }
    let mut value = Vec::with_capacity(words);
    for chunk in data[..used].chunks_exact(4) {
        let mut word = [0_u8; 4];
        word.copy_from_slice(chunk);
        value.push(u32::from_le_bytes(word));
    }
    let mut key_words = [0_u32; 4];
    for (index, chunk) in key.chunks_exact(4).enumerate() {
        let mut word = [0_u8; 4];
        word.copy_from_slice(chunk);
        key_words[index] = u32::from_le_bytes(word);
    }
    apply(&mut value, &key_words);
    for (index, word) in value.iter().enumerate() {
        let start = index * 4;
        data[start..start + 4].copy_from_slice(&word.to_le_bytes());
    }
    Ok(())
}

fn apply(value: &mut [u32], key: &[u32; 4]) {
    let count = value.len();
    let mut rounds = 6 + 52 / count;
    let mut sum = 0_u32;
    let mut mixed = value[count - 1];
    while rounds != 0 {
        sum = sum.wrapping_sub(ROUND);
        rounds -= 1;
        let round_index = ((sum >> 2) & 3) as usize;
        let mut index = 0;
        if count != 1 {
            while index < count - 1 {
                let next = value[index + 1];
                value[index] = value[index].wrapping_add(mix(
                    mixed,
                    next,
                    sum,
                    key[(index & 3) ^ round_index],
                ));
                mixed = value[index];
                index += 1;
            }
        }
        let first = value[0];
        let end = count - 1;
        value[end] =
            value[end].wrapping_add(mix(mixed, first, sum, key[(index & 3) ^ round_index]));
        mixed = value[end];
    }
}

fn mix(z: u32, y: u32, sum: u32, key: u32) -> u32 {
    let left = (z >> 5 ^ y << 2).wrapping_add(y >> 3 ^ z << 4);
    let right = (sum ^ y).wrapping_add(key ^ z);
    left ^ right
}

#[cfg(test)]
mod tests {
    use super::{
        BOOT_MODE, BOOT_POLLS, DFU_NAME_OFFSET, LEAVE_MODE, LEAVE_POLLS, NAME_DIALOG,
        NAME_DIALOG_CAPTION, NAME_DIALOG_FLAGS, POLL_WAIT, RewriteError, dfu_get_info, erase_chip,
        flash_length_fits, get_mode, in_boot, left_boot, name_blocks, poll_passes, rewrite_page,
        set_mode, write_flash, write_serial,
    };
    use crate::{MAX_PAYLOAD, pack};

    #[test]
    fn set_mode_a5_packs_the_header_the_dll_writes() {
        let built = set_mode(BOOT_MODE);
        assert_eq!(built.header(), [0x80, 0x00, 0xF0, 0x00]);
        assert_eq!(built.payload, [0xA5]);
        assert_eq!(
            pack(built.header(), &built.payload).unwrap(),
            [0xBB, 0x80, 0x00, 0xF0, 0x00, 0xA5, 0x15, 0xBB]
        );
    }

    #[test]
    fn leave_mode_is_a_zero_byte() {
        let built = set_mode(LEAVE_MODE);
        assert_eq!(built.payload, [0]);
        assert_eq!(built.command.code(), 0xF0);
    }

    #[test]
    fn get_mode_and_dfu_info_and_erase_are_empty() {
        assert_eq!(get_mode().header(), [0x80, 0x00, 0xF1, 0x00]);
        assert_eq!(get_mode().payload, Vec::<u8>::new());
        assert_eq!(dfu_get_info().header(), [0x80, 0x00, 0xF2, 0x00]);
        assert_eq!(dfu_get_info().payload, Vec::<u8>::new());
        let erase = erase_chip();
        assert_eq!(erase.header(), [0x80, 0x00, 0xF3, 0x00]);
        assert_eq!(
            pack(erase.header(), &erase.payload).unwrap(),
            [0xBB, 0x80, 0x00, 0xF3, 0x00, 0x73, 0xBB]
        );
    }

    #[test]
    fn write_flash_copies_the_page_and_refuses_a_long_one() {
        let built = write_flash(&[9, 8, 7]).unwrap();
        assert_eq!(built.header(), [0x80, 0x00, 0xF5, 0x00]);
        assert_eq!(built.payload, [9, 8, 7]);
        let long = vec![0_u8; MAX_PAYLOAD + 1];
        assert_eq!(write_flash(&long), Err(super::DfuError::Length));
    }

    #[test]
    fn write_serial_is_the_eight_bytes() {
        let bytes = [1, 2, 3, 4, 5, 6, 7, 8];
        let built = write_serial(&bytes);
        assert_eq!(built.header(), [0x80, 0x00, 0xF7, 0x00]);
        assert_eq!(built.payload, bytes);
    }

    #[test]
    fn the_image_dword_does_not_fit_and_a_short_length_does() {
        assert!(!flash_length_fits(0xA5A5_A5A5));
        assert!(flash_length_fits(8));
        assert!(!flash_length_fits(u32::try_from(MAX_PAYLOAD).unwrap() + 1));
    }

    #[test]
    fn boot_and_leave_are_the_bytes_the_compare_uses() {
        assert!(in_boot(0xA5));
        assert!(!in_boot(0));
        assert!(left_boot(0));
        assert!(!left_boot(0xA5));
        assert_eq!(LEAVE_POLLS, 10);
        assert_eq!(BOOT_POLLS, 10);
        assert_eq!(poll_passes(BOOT_POLLS), 11);
        assert_eq!(poll_passes(LEAVE_POLLS), 11);
        assert_eq!(poll_passes(0), 1);
        assert_eq!(POLL_WAIT, 0x1F4);
        assert_eq!(NAME_DIALOG, "Device Firmware Wrong!");
        assert_eq!(NAME_DIALOG_CAPTION, "Error");
        assert_eq!(NAME_DIALOG_FLAGS, 0x1010);
        assert_eq!(DFU_NAME_OFFSET, 0x20);
    }

    #[test]
    fn a_different_vcx_name_blocks_and_an_equal_name_does_not() {
        let nano = *b"VCX-NANO";
        let at40 = *b"VCX-At40";
        assert!(name_blocks(&nano, &at40));
        assert!(!name_blocks(&nano, &nano));
        let other = *b"ABS\0\0\0\0\0";
        assert!(!name_blocks(&other, &nano));
    }

    #[test]
    fn two_words_match_the_emulated_rewrite() {
        let mut data = [1, 0, 0, 0, 2, 0, 0, 0];
        rewrite_page(&mut data, 8, &[0; 16]).unwrap();
        assert_eq!(data, [0xC6, 0x06, 0x71, 0xDF, 0x64, 0x4E, 0x44, 0x19]);
    }

    #[test]
    fn one_word_is_rewritten_and_the_tail_stays() {
        let mut data = [1, 2, 3, 4, 0xEE];
        rewrite_page(&mut data, 4, &[0; 16]).unwrap();
        assert_eq!(&data[..4], &[0x8F, 0x93, 0xD4, 0x89]);
        assert_eq!(data[4], 0xEE);
    }

    #[test]
    fn a_page_with_no_words_is_refused() {
        let mut data = [1, 2, 3];
        assert_eq!(
            rewrite_page(&mut data, 3, &[0; 16]),
            Err(RewriteError::Empty)
        );
        assert_eq!(data, [1, 2, 3]);
    }

    #[test]
    fn a_page_shorter_than_its_length_is_refused() {
        let mut data = [1, 0, 0, 0];
        assert_eq!(
            rewrite_page(&mut data, 8, &[0; 16]),
            Err(RewriteError::Short)
        );
        assert_eq!(data, [1, 0, 0, 0]);
    }
}
