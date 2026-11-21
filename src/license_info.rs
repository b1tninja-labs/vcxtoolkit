//! The on-device license-info buffer.
//!
//! A stride of 8 bytes was parsed once and the product-name test failed; each record is 16 bytes.
//! This module does not call a license server and does not write the buffer back.
//!
//! The buffer is `0xBEC` bytes. The serial is the raw 8 bytes at `+0x00`. Twelve bytes at
//! `+0x08` are zero and unread. Two little-endian Unix timestamps follow: `issued` at
//! `+0x14` and `updated` at `+0x18`. The period dword is at `+0x1C`. A displayed date is
//! the UTC calendar day (`YYYY-MM-DD`). The header stamps also carry a time of day, which
//! is not kept. `updated` is the stamp that moved from 2025-12-06 to 2026-11-20 between
//! the two captures, so it is not an end date. The dword `3` at `+0x20` is unread.
//!
//! At `+0x64` there are 168 records of 16 bytes: an 8-byte name, a Unix timestamp, and a
//! zero dword. The name is space-padded to 8 bytes. A NUL ends it earlier, and trailing
//! spaces are padding. A row whose name is empty or whose timestamp is 0 is skipped. The
//! tail at `+0xAE4` is 264 bytes of padding, `0x53` in the first capture and `0x38` in the
//! later one, and it is not a row.
//!
//! The decipher key's second half is the eight-byte `80 00 A0 00` reply raised
//! to the same secret that produced the request payload as `5^secret`, both
//! modulo `2^64 - 59`.

const LEN: usize = 0xBEC;
const ISSUED: usize = 0x14;
const UPDATED: usize = 0x18;
const PERIOD: usize = 0x1C;
const PRODUCTS: usize = 0x64;
const PRODUCT_END: usize = 0xAE4;
const RECORD: usize = 16;
const NAME: usize = 8;
const DAY_SECONDS: u32 = 86_400;

/// Why a license-info buffer was not parsed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LicenseInfoError {
    /// The slice is not exactly `0xBEC` bytes.
    Length,
    /// The ciphertext is not [`LICENSE_CIPHER`] bytes.
    Cipher,
    /// A product name is not a UTF-8 C string.
    Name,
}

impl std::fmt::Display for LicenseInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Length => "license info is not 0xBEC bytes",
            Self::Cipher => "license ciphertext is not 3068 bytes",
            Self::Name => "product name is not UTF-8",
        })
    }
}

impl std::error::Error for LicenseInfoError {}

/// UTC calendar day of a little-endian Unix timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LicenseDate {
    /// Year of that UTC day.
    pub year: u16,
    /// Month, from 1 through 12.
    pub month: u8,
    /// Day of the month, from 1 through 31.
    pub day: u8,
}

impl std::fmt::Display for LicenseDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// One product row whose name is nonempty and whose timestamp is nonzero.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Product {
    /// The 8-byte name as UTF-8, without its trailing space padding.
    pub name: String,
    /// UTC day of the row's Unix timestamp.
    pub date: LicenseDate,
}

/// The license fields read from the on-device buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LicenseInfo {
    /// The raw 8 bytes at `+0x00`.
    pub serial: [u8; 8],
    /// UTC day of the Unix timestamp at `+0x14`.
    pub issued: LicenseDate,
    /// UTC day of the Unix timestamp at `+0x18`.
    pub updated: LicenseDate,
    /// The dword at `+0x1C`, `0x004F1A00` on the measured device.
    pub period_seconds: u32,
    /// Product rows from `+0x64`, in buffer order.
    pub products: Vec<Product>,
}

/// Read the license from one on-device buffer.
///
/// # Errors
///
/// Returns [`LicenseInfoError::Length`] when `bytes` is not exactly `0xBEC` bytes,
/// and [`LicenseInfoError::Name`] when a product name is not UTF-8.
#[must_use = "a license that is parsed and dropped was not read"]
pub fn parse_license_info(bytes: &[u8]) -> Result<LicenseInfo, LicenseInfoError> {
    if bytes.len() != LEN {
        return Err(LicenseInfoError::Length);
    }
    let mut serial = [0_u8; 8];
    serial.copy_from_slice(&bytes[..8]);
    let mut products = Vec::new();
    for (index, record) in bytes[PRODUCTS..PRODUCT_END]
        .chunks_exact(RECORD)
        .enumerate()
    {
        let at = PRODUCTS + index * RECORD;
        let stamp = u32_at(bytes, at + NAME);
        if stamp == 0 {
            continue;
        }
        let Some(name) = name_at(&record[..NAME])? else {
            continue;
        };
        products.push(Product {
            name,
            date: unix_date(stamp),
        });
    }
    Ok(LicenseInfo {
        serial,
        issued: unix_date(u32_at(bytes, ISSUED)),
        updated: unix_date(u32_at(bytes, UPDATED)),
        period_seconds: u32_at(bytes, PERIOD),
        products,
    })
}

/// Byte length of the ciphertext `GetLicense` passes to the decipher.
///
/// The reply payload is one leading byte plus this body.
pub const LICENSE_CIPHER: usize = 0xBFC;

const SUM_START: u32 = 0x8DDE_6E40;
const ROUND: u32 = 0x61C8_8647;

/// Decipher the `0xBFC`-byte body of a `GetLicense` reply.
///
/// The key is the raw 8-byte serial followed by [`license_key_half`].
/// The plain text is the on-device buffer.
///
/// # Errors
///
/// Returns [`LicenseInfoError::Cipher`] when `body` is not [`LICENSE_CIPHER`]
/// bytes, and [`LicenseInfoError::Name`] when a product name is not UTF-8.
pub fn decrypt_license(body: &[u8], key: &[u8; 16]) -> Result<LicenseInfo, LicenseInfoError> {
    if body.len() != LICENSE_CIPHER {
        return Err(LicenseInfoError::Cipher);
    }
    let mut plain = body.to_vec();
    decipher(&mut plain, key);
    parse_license_info(&plain[..LEN])
}

/// `2^64 - 59`. The clear multiply at `0x7B2FB340` reduces by this.
const MODULUS: u128 = (1_u128 << 64) - 59;

/// Base of the nonce. Ordinal 21 raises this to the caller's secret.
const GENERATOR: u64 = 5;

/// Eight payload bytes for header `80 00 A0 00`.
///
/// The value is `5^secret` modulo `2^64 - 59`, written little-endian.
/// `secret` is the caller's choice. The matching key half is
/// [`license_key_half`] of the eight-byte reply.
#[must_use]
pub fn license_nonce(secret: u64) -> [u8; 8] {
    mod_pow(GENERATOR, secret).to_le_bytes()
}

/// The eight key bytes stored beside the raw serial.
///
/// `reply` is the eight bytes after the status byte of the `80 00 A0 00`
/// answer. The result is that integer, raised to `secret`, modulo
/// `2^64 - 59`, written little-endian.
#[must_use]
pub fn license_key_half(secret: u64, reply: [u8; 8]) -> [u8; 8] {
    mod_pow(u64::from_le_bytes(reply), secret).to_le_bytes()
}

/// The 16-byte decipher key: `serial`, then [`license_key_half`].
#[must_use]
pub fn license_key(serial: [u8; 8], secret: u64, reply: [u8; 8]) -> [u8; 16] {
    let mut key = [0_u8; 16];
    key[..8].copy_from_slice(&serial);
    key[8..].copy_from_slice(&license_key_half(secret, reply));
    key
}

fn mod_mul(left: u64, right: u64) -> u64 {
    let product = u128::from(left) * u128::from(right);
    u64::try_from(product % MODULUS).unwrap_or(0)
}

fn mod_pow(mut base: u64, mut exponent: u64) -> u64 {
    let mut acc = 1_u64;
    while exponent != 0 {
        if exponent & 1 == 1 {
            acc = mod_mul(acc, base);
        }
        base = mod_mul(base, base);
        exponent >>= 1;
    }
    acc
}

/// Header `80 00 A0 00`. No clear export writes this byte.
const CHALLENGE_HEADER: [u8; 4] = [0x80, 0x00, 0xA0, 0x00];

/// The eight reply bytes after a leading status of `0x00`.
fn challenge_reply(payload: &[u8]) -> Option<[u8; 8]> {
    if payload.len() != 9 || payload.first() != Some(&0) {
        return None;
    }
    let mut reply = [0_u8; 8];
    reply.copy_from_slice(&payload[1..]);
    Some(reply)
}

/// Raw serial at `DevInfo.Serial`, which is the license key's first half.
fn raw_serial(frame: &crate::Frame) -> Option<[u8; 8]> {
    if frame.header != crate::Command::GetInfo.header(0) {
        return None;
    }
    let start = 1 + 0x10;
    let end = start + 8;
    if frame.payload.len() < end {
        return None;
    }
    let mut serial = [0_u8; 8];
    serial.copy_from_slice(&frame.payload[start..end]);
    Some(serial)
}

/// Why [`read_license`] did not return the on-device buffer.
#[cfg(windows)]
#[derive(Debug)]
pub enum LicenseReadError {
    /// The port exchange failed.
    Exchange(crate::ExchangeError),
    /// The `GetInfo` reply did not hold the raw serial.
    Info,
    /// The `80 00 A0 00` reply was not a zero status and eight bytes.
    Challenge,
    /// The `GetLicense` body did not decipher.
    License(LicenseInfoError),
}

#[cfg(windows)]
impl std::fmt::Display for LicenseReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exchange(error) => write!(f, "{error}"),
            Self::Info => f.write_str("GetInfo reply does not contain the raw serial"),
            Self::Challenge => f.write_str("challenge reply is not a status byte and eight bytes"),
            Self::License(error) => write!(f, "{error}"),
        }
    }
}

#[cfg(windows)]
impl std::error::Error for LicenseReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Exchange(error) => Some(error),
            Self::License(error) => Some(error),
            Self::Info | Self::Challenge => None,
        }
    }
}

/// Read the on-device license.
///
/// Sends `GetInfo`, then header `80 00 A0 00` with [`license_nonce`] of a
/// fresh secret, then empty `GetLicense`. The decipher key is the raw serial
/// plus [`license_key_half`] of the challenge reply. Nothing is written.
///
/// # Errors
///
/// Returns [`LicenseReadError::Exchange`] when a frame is not exchanged,
/// [`LicenseReadError::Info`] when the serial is missing,
/// [`LicenseReadError::Challenge`] when the middle reply is not eight bytes
/// after a zero status, and [`LicenseReadError::License`] when the ciphertext
/// does not decipher.
#[cfg(windows)]
#[must_use = "a license that is read and dropped was not shown"]
pub fn read_license(
    port: &crate::Port,
    timeout: std::time::Duration,
) -> Result<LicenseInfo, LicenseReadError> {
    let info = crate::exchange(port, crate::Command::GetInfo.header(0), &[], timeout)
        .map_err(LicenseReadError::Exchange)?;
    let serial = raw_serial(&info).ok_or(LicenseReadError::Info)?;
    let secret = fresh_secret();
    let reply = crate::exchange(port, CHALLENGE_HEADER, &license_nonce(secret), timeout)
        .map_err(LicenseReadError::Exchange)?;
    if reply.header != CHALLENGE_HEADER {
        return Err(LicenseReadError::Challenge);
    }
    let scalar = challenge_reply(&reply.payload).ok_or(LicenseReadError::Challenge)?;
    let body = crate::exchange(port, crate::Command::GetLicense.header(0), &[], timeout)
        .map_err(LicenseReadError::Exchange)?;
    if body.payload.len() != 1 + LICENSE_CIPHER {
        return Err(LicenseReadError::License(LicenseInfoError::Cipher));
    }
    decrypt_license(&body.payload[1..], &license_key(serial, secret, scalar))
        .map_err(LicenseReadError::License)
}

#[cfg(windows)]
fn fresh_secret() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1_u128, |elapsed| elapsed.as_nanos());
    let mixed = u64::try_from(nanos & u128::from(u64::MAX)).unwrap_or(1);
    if mixed == 0 { 1 } else { mixed }
}

fn decipher(body: &mut [u8], key: &[u8; 16]) {
    let count = body.len() / 4;
    let mut words = Vec::with_capacity(count);
    for chunk in body.chunks_exact(4) {
        let mut raw = [0_u8; 4];
        raw.copy_from_slice(chunk);
        words.push(u32::from_le_bytes(raw));
    }
    let mut key_words = [0_u32; 4];
    for (index, chunk) in key.chunks_exact(4).enumerate() {
        let mut raw = [0_u8; 4];
        raw.copy_from_slice(chunk);
        key_words[index] = u32::from_le_bytes(raw);
    }
    let mut total =
        SUM_START.wrapping_sub((0x34 / u32::try_from(count).unwrap_or(1)).wrapping_mul(ROUND));
    let mut mixed = words[0];
    while total != 0 {
        let slot = usize::try_from((total >> 2) & 3).unwrap_or(0);
        let mut index = count;
        while index > 1 {
            index -= 1;
            let right = words[index - 1];
            words[index] =
                words[index].wrapping_sub(mix(mixed, right, total, key_words[(index & 3) ^ slot]));
            mixed = words[index];
        }
        let right = words[count - 1];
        words[0] = words[0].wrapping_sub(mix(mixed, right, total, key_words[slot]));
        mixed = words[0];
        total = total.wrapping_add(ROUND);
    }
    for (index, word) in words.iter().enumerate() {
        let start = index * 4;
        body[start..start + 4].copy_from_slice(&word.to_le_bytes());
    }
}

fn mix(mixed: u32, right: u32, total: u32, key_word: u32) -> u32 {
    let left = (right >> 5) ^ mixed.wrapping_shl(2);
    let shifted = (mixed >> 3) ^ right.wrapping_shl(4);
    (left.wrapping_add(shifted)) ^ (total ^ mixed).wrapping_add(key_word ^ right)
}

const fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn name_at(raw: &[u8]) -> Result<Option<String>, LicenseInfoError> {
    let nul = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
    let padded = &raw[..nul];
    let end = padded
        .iter()
        .rposition(|byte| *byte != b' ')
        .map_or(0, |index| index + 1);
    let text = &padded[..end];
    if text.is_empty() {
        return Ok(None);
    }
    std::str::from_utf8(text)
        .map(|name| Some(name.to_owned()))
        .map_err(|_| LicenseInfoError::Name)
}

/// Howard Hinnant's `civil_from_days`, for a nonnegative Unix timestamp.
fn unix_date(seconds: u32) -> LicenseDate {
    let shifted_days = seconds / DAY_SECONDS + 719_468;
    let era_index = shifted_days / 146_097;
    let remainder = shifted_days - era_index * 146_097;
    let year_within =
        (remainder - remainder / 1_460 + remainder / 36_524 - remainder / 146_096) / 365;
    let absolute_year = year_within + era_index * 400;
    let ordinal = remainder - (365 * year_within + year_within / 4 - year_within / 100);
    let slot = (5 * ordinal + 2) / 153;
    let calendar_day = ordinal - (153 * slot + 2) / 5 + 1;
    let calendar_month = if slot < 10 { slot + 3 } else { slot - 9 };
    let calendar_year = absolute_year + u32::from(calendar_month <= 2);
    LicenseDate {
        year: u16::try_from(calendar_year).expect("unix year fits in u16"),
        month: u8::try_from(calendar_month).expect("month fits in u8"),
        day: u8::try_from(calendar_day).expect("day fits in u8"),
    }
}

#[cfg(test)]
mod tests {
    use super::{LicenseDate, LicenseInfoError, parse_license_info};

    const SERIAL: [u8; 8] = [0x4E, 0x43, 0x41, 0x47, 0x4D, 0x67, 0x8B, 0x28];
    const ISSUED_STAMP: u32 = 0x650D_2EFA;
    const UPDATED_STAMP: u32 = 0x6933_9FD3;
    const PERIOD: u32 = 0x004F_1A00;
    const PRODUCT_STAMP: u32 = 0x72BB_BA80;

    fn padded_record(name: &[u8], stamp: u32) -> [u8; 16] {
        let mut raw = [b' '; 16];
        raw[..name.len()].copy_from_slice(name);
        raw[8..12].copy_from_slice(&stamp.to_le_bytes());
        raw[12..16].fill(0);
        raw
    }

    fn buffer(records: &[[u8; 16]]) -> Vec<u8> {
        let mut bytes = vec![0_u8; 0xBEC];
        bytes[..8].copy_from_slice(&SERIAL);
        bytes[0x14..0x18].copy_from_slice(&ISSUED_STAMP.to_le_bytes());
        bytes[0x18..0x1C].copy_from_slice(&UPDATED_STAMP.to_le_bytes());
        bytes[0x1C..0x20].copy_from_slice(&PERIOD.to_le_bytes());
        for (index, record) in records.iter().enumerate() {
            let at = 0x64 + index * 16;
            bytes[at..at + 16].copy_from_slice(record);
        }
        bytes
    }

    fn day(year: u16, month: u8, day: u8) -> LicenseDate {
        LicenseDate { year, month, day }
    }

    #[test]
    fn header_stamps_display_as_utc_dates() {
        let cases = [
            (0x650D_2EFA_u32, "2023-09-22"),
            (0x6933_9FD3_u32, "2025-12-06"),
            (0x6B00_5C93_u32, "2026-11-20"),
            (0x72BB_BA80_u32, "2030-12-31"),
            (0x65DF_C900_u32, "2024-02-29"),
            (0x65E1_1A80_u32, "2024-03-01"),
            (0_u32, "1970-01-01"),
            (86_400_u32, "1970-01-02"),
        ];
        for (stamp, text) in cases {
            let mut bytes = buffer(&[]);
            bytes[0x14..0x18].copy_from_slice(&stamp.to_le_bytes());
            let info = parse_license_info(&bytes).unwrap();
            assert_eq!(info.issued.to_string(), text);
            assert_eq!(info.products, Vec::new());
        }
    }

    #[test]
    fn product_names_are_the_padded_rows() {
        let bytes = buffer(&[
            padded_record(b"PASSTHRU", PRODUCT_STAMP),
            padded_record(b"PDU", PRODUCT_STAMP),
            padded_record(b"GMGDS", PRODUCT_STAMP),
            padded_record(b"GMT2WIN", PRODUCT_STAMP),
            padded_record(b"_NA_NA_N", 0),
            padded_record(b"   ", PRODUCT_STAMP),
            padded_record(b"AB\0XXXX", PRODUCT_STAMP),
        ]);
        let info = parse_license_info(&bytes).unwrap();
        assert_eq!(info.serial, SERIAL);
        assert_eq!(info.issued.to_string(), "2023-09-22");
        assert_eq!(info.updated.to_string(), "2025-12-06");
        assert_eq!(info.period_seconds, 0x004F_1A00);
        let names: Vec<&str> = info
            .products
            .iter()
            .map(|product| product.name.as_str())
            .collect();
        assert_eq!(names, ["PASSTHRU", "PDU", "GMGDS", "GMT2WIN", "AB"]);
        assert!(
            info.products
                .iter()
                .all(|product| product.date == day(2030, 12, 31))
        );
        assert!(
            info.products
                .iter()
                .all(|product| product.date.to_string() == "2030-12-31")
        );
    }

    #[test]
    fn a_buffer_that_is_not_the_device_length_is_refused() {
        let full = buffer(&[]);
        assert_eq!(
            parse_license_info(&full[..full.len() - 1]),
            Err(LicenseInfoError::Length)
        );
        assert_eq!(parse_license_info(&[]), Err(LicenseInfoError::Length));
        let mut long = full;
        long.push(0);
        assert_eq!(parse_license_info(&long), Err(LicenseInfoError::Length));
    }

    #[test]
    fn a_name_that_is_not_utf8_is_refused() {
        let bytes = buffer(&[padded_record(&[0xFF], PRODUCT_STAMP)]);
        assert_eq!(parse_license_info(&bytes), Err(LicenseInfoError::Name));
    }

    #[test]
    fn the_device_buffer_names_four_products() {
        let path = std::path::Path::new(r"D:\temp\vxdiag-re\fanout\license.bin");
        if !path.is_file() {
            return;
        }
        let bytes = std::fs::read(path).unwrap();
        let info = parse_license_info(&bytes).unwrap();
        assert_eq!(info.serial, SERIAL);
        assert_eq!(info.issued.to_string(), "2023-09-22");
        assert_eq!(info.updated.to_string(), "2026-11-20");
        assert_eq!(info.period_seconds, 0x004F_1A00);
        let names: Vec<&str> = info
            .products
            .iter()
            .map(|product| product.name.as_str())
            .collect();
        assert_eq!(names, ["PASSTHRU", "PDU", "GMGDS", "GMT2WIN"]);
        assert!(
            info.products
                .iter()
                .all(|product| product.date.to_string() == "2030-12-31")
        );
    }

    #[test]
    fn an_eight_byte_body_deciphers_to_the_pinned_bytes() {
        let mut body = [0_u8, 1, 2, 3, 4, 5, 6, 7];
        let key: [u8; 16] = core::array::from_fn(|index| u8::try_from(index).unwrap_or(0));
        super::decipher(&mut body, &key);
        assert_eq!(body, [0x43, 0xD2, 0x6A, 0x33, 0x04, 0x8B, 0x08, 0xEC]);
    }

    #[test]
    fn the_captured_reply_deciphers_to_the_device_buffer() {
        let dir = std::path::Path::new(r"D:\temp\vxdiag-re\fanout");
        let cipher = dir.join("cipher-in.bin");
        let key_path = dir.join("cipher-key.bin");
        let plain = dir.join("license.bin");
        if !cipher.is_file() || !key_path.is_file() || !plain.is_file() {
            return;
        }
        let body = std::fs::read(cipher).unwrap();
        let key_bytes = std::fs::read(key_path).unwrap();
        let mut key = [0_u8; 16];
        key.copy_from_slice(&key_bytes);
        let info = super::decrypt_license(&body, &key).unwrap();
        let stored = parse_license_info(&std::fs::read(plain).unwrap()).unwrap();
        assert_eq!(info, stored);
    }

    #[test]
    fn the_key_half_is_the_reply_raised_to_the_secret() {
        let odd_secret = 0xD575_0455_4A18_67C1;
        let odd_reply = 0x72F9_709F_9968_7849_u64.to_le_bytes();
        assert_eq!(
            super::license_nonce(odd_secret),
            0xFEFF_81F9_5FA6_6669_u64.to_le_bytes()
        );
        assert_eq!(
            super::license_key_half(odd_secret, odd_reply),
            0x2F1E_32FC_07CD_9A56_u64.to_le_bytes()
        );
        let even_secret = 0x9CD1_CB84_59B8_154E;
        let even_reply = 0x0993_06D4_B8C3_6817_u64.to_le_bytes();
        assert_eq!(
            super::license_nonce(even_secret),
            0x3617_08AD_52FF_5E31_u64.to_le_bytes()
        );
        assert_eq!(
            super::license_key_half(even_secret, even_reply),
            0x688A_B093_57B8_3BD4_u64.to_le_bytes()
        );
        let mut key = [0_u8; 16];
        key[..8].copy_from_slice(&SERIAL);
        key[8..].copy_from_slice(&super::license_key_half(even_secret, even_reply));
        assert_eq!(super::license_key(SERIAL, even_secret, even_reply), key);
    }

    #[test]
    fn a_secret_of_one_sends_the_generator() {
        assert_eq!(super::license_nonce(1), 5_u64.to_le_bytes());
        assert_eq!(super::license_nonce(0), 1_u64.to_le_bytes());
        assert_eq!(
            super::license_key_half(0, 5_u64.to_le_bytes()),
            1_u64.to_le_bytes()
        );
    }

    #[test]
    fn the_challenge_reply_is_a_zero_status_and_eight_bytes() {
        assert_eq!(
            super::challenge_reply(&[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            Some([1, 2, 3, 4, 5, 6, 7, 8])
        );
        assert_eq!(super::challenge_reply(&[1, 1, 2, 3, 4, 5, 6, 7, 8]), None);
        assert_eq!(super::challenge_reply(&[0, 1, 2, 3, 4, 5, 6, 7]), None);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "needs the CH343 dongle on COM4"]
    fn com4_reads_the_four_products() {
        let port = crate::open_usb(4, 0).expect("COM4");
        let info = super::read_license(&port, std::time::Duration::from_secs(3)).expect("license");
        assert_eq!(info.serial, SERIAL);
        assert_eq!(info.issued.to_string(), "2023-09-22");
        assert_eq!(info.period_seconds, 0x004F_1A00);
        let names: Vec<&str> = info
            .products
            .iter()
            .map(|product| product.name.as_str())
            .collect();
        assert_eq!(names, ["PASSTHRU", "PDU", "GMGDS", "GMT2WIN"]);
        assert!(
            info.products
                .iter()
                .all(|product| product.date.to_string() == "2030-12-31")
        );
    }
}
