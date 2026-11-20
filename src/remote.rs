//! Retrieve a license and a newer firmware image.
//!
//! The session builder decrypts three buffers, appends the bytes past each
//! buffer's word count, and signs that concatenation with SHA-256. The signature
//! is standard base64 with `+` written as `%2B`. The license GET sends that
//! result as `Authorization: authcode`. Changing the XXTEA delta made the
//! timestamp test fail, and the delta was restored.
//!
//! The downloaded license is the buffer the manager writes with ordinal 22.
//! This module does not send that ordinal, and it does not send the erase.
//! [`firmware_for`](crate::firmware_for) is the image the catalog lists for a PID,
//! including when it matches the running version. [`newer_firmware`](crate::newer_firmware)
//! is that image only when it is newer. Neither download is written to the device.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::catalog::{FirmwareItem, firmware_for, newer_firmware, parse_catalog};
use crate::hash::{base64, md5_matches, sha256};

/// License service base from `LicenseServerURL`.
pub const LICENSE_SERVER: &str = "https://api.vxdiag.net/v1";

/// Firmware catalog from `FirmwareURL`.
pub const FIRMWARE_CATALOG: &str = "https://www.vxdiag.net/managecenter/fws.xml";

const KEY: [u32; 4] = [0x4534_2231, 0x9981_7964, 0xef7f_7810, 0xc9b8_e7a6];

/// Ciphertext plus the two bytes past the word count.
const APPID: [u8; 22] = [
    0x53, 0x95, 0xce, 0xbd, 0xf1, 0xd5, 0x8f, 0x3e, 0x89, 0xea, 0xb9, 0x73, 0xb9, 0xea, 0xa8, 0xc0,
    0xc1, 0x67, 0xb8, 0xe3, 0x64, 0x36,
];

/// Ciphertext plus the three bytes past the word count.
const STAMP: [u8; 11] = [
    0xfb, 0x37, 0x7c, 0x9c, 0x12, 0x58, 0x54, 0x52, 0x6d, 0x70, 0x3d,
];

/// Ciphertext. The word count covers the whole buffer.
const SECRET: [u8; 24] = [
    0x94, 0xfa, 0x77, 0xd5, 0x09, 0xe6, 0xc3, 0x54, 0xbf, 0x09, 0x57, 0x97, 0x05, 0xec, 0x60, 0xe6,
    0x07, 0xf6, 0x13, 0x61, 0xf6, 0x25, 0x78, 0xff,
];

/// Why a license or a firmware image was not retrieved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteError {
    /// The clock is before the Unix epoch.
    Clock,
    /// The device id or the URL is not one this client sends.
    Url,
    /// The response was not UTF-8.
    Body,
    /// The JSON had neither the payload nor a service code.
    Json,
    /// The service returned a non-zero code and no payload.
    Service(i64),
    /// `result.license` is not hex.
    Hex,
    /// The image length is not the catalog `SIZE`.
    Size,
    /// The image MD5 is not the catalog `MD5`.
    Digest,
    /// The catalog has no image for that PID.
    Missing,
    /// The GET failed.
    Http(HttpFailure),
}

/// The GET failure, without the URL or the body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpFailure {
    /// `WinHTTP` failed.
    Transport,
    /// The status was not 200.
    Status(u32),
}

impl std::fmt::Display for RemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clock => f.write_str("clock is before the Unix epoch"),
            Self::Url => f.write_str("device id or URL is not sent"),
            Self::Body => f.write_str("response is not UTF-8"),
            Self::Json => f.write_str("response has no license payload"),
            Self::Service(code) => write!(f, "license service returned {code}"),
            Self::Hex => f.write_str("license field is not hex"),
            Self::Size => f.write_str("firmware length is not the catalog size"),
            Self::Digest => f.write_str("firmware MD5 is not the catalog MD5"),
            Self::Missing => f.write_str("catalog has no image for that PID"),
            Self::Http(HttpFailure::Transport) => f.write_str("request failed"),
            Self::Http(HttpFailure::Status(status)) => write!(f, "HTTP status {status}"),
        }
    }
}

impl std::error::Error for RemoteError {}

/// The auth URL for `unix_secs`.
///
/// The query carries the session material. Do not log the returned string.
#[must_use]
pub fn auth_url(unix_secs: u64) -> String {
    let appid = piece(&APPID);
    let stamp = piece(&STAMP);
    let secret = piece(&SECRET);
    let digits = unix_secs.to_string();
    let mut signed = Vec::new();
    signed.extend_from_slice(&appid);
    signed.extend_from_slice(&stamp);
    signed.extend_from_slice(digits.as_bytes());
    signed.extend_from_slice(&secret);
    let sign = base64(&sha256(&signed)).replace('+', "%2B");
    let appid = String::from_utf8_lossy(&appid);
    let stamp = String::from_utf8_lossy(&stamp);
    format!("{LICENSE_SERVER}/auth?{appid}{stamp}{digits}&sign={sign}")
}

/// `{LICENSE_SERVER}/devices/{device_id}/license` when `device_id` is ASCII alphanumeric.
///
/// # Errors
///
/// Returns [`RemoteError::Url`] when `device_id` is empty or contains any other byte.
pub fn license_url(device_id: &str) -> Result<String, RemoteError> {
    if device_id.is_empty() || !device_id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(RemoteError::Url);
    }
    Ok(format!("{LICENSE_SERVER}/devices/{device_id}/license"))
}

/// Decode the hex text in `result.license`.
///
/// # Errors
///
/// Returns [`RemoteError::Hex`] when `text` is empty, odd, or not hex.
pub fn decode_license(text: &str) -> Result<Vec<u8>, RemoteError> {
    if text.is_empty() || text.len() % 2 != 0 {
        return Err(RemoteError::Hex);
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    let mut index = 0;
    while index < bytes.len() {
        let high = hex_value(bytes[index]).ok_or(RemoteError::Hex)?;
        let low = hex_value(bytes[index + 1]).ok_or(RemoteError::Hex)?;
        out.push((high << 4) | low);
        index += 2;
    }
    Ok(out)
}

/// GET the license for `device_id`.
///
/// The returned bytes are the server license. They are not written to the device.
///
/// # Errors
///
/// Returns [`RemoteError`] when the clock, the id, the GET, or the JSON fails.
#[cfg(windows)]
pub fn fetch_license(device_id: &str) -> Result<Vec<u8>, RemoteError> {
    let now = unix_seconds()?;
    let auth = get_text(&auth_url(now), None)?;
    let Some(code) = json_string(&auth, "authCode") else {
        return Err(service_or_json(&auth));
    };
    if code.is_empty() || code.bytes().any(|byte| byte == b'\r' || byte == b'\n') {
        return Err(RemoteError::Json);
    }
    let header = format!("Authorization: authcode {code}\r\n");
    let body = get_text(&license_url(device_id)?, Some(&header))?;
    let Some(hex) = json_string(&body, "license") else {
        return Err(service_or_json(&body));
    };
    decode_license(&hex)
}

/// GET the catalog image for `pid`, including when it matches the running version.
///
/// The bytes are not written to the device.
///
/// # Errors
///
/// Returns [`RemoteError`] when the GET fails, the catalog has no image for `pid`,
/// or the image length or MD5 does not match the item.
#[cfg(windows)]
pub fn fetch_firmware(pid: u32) -> Result<Vec<u8>, RemoteError> {
    let items = catalog_items()?;
    let item = firmware_for(&items, pid).ok_or(RemoteError::Missing)?;
    download_item(item)
}

/// GET the catalog and, when an item is newer than `running` for `pid`, GET that image.
///
/// [`None`] means the catalog has nothing newer. The image is not written to the device.
///
/// # Errors
///
/// Returns [`RemoteError`] when a GET fails, the catalog is not the expected document,
/// or the image length or MD5 does not match the item.
#[cfg(windows)]
pub fn fetch_newer_firmware(pid: u32, running: [u8; 4]) -> Result<Option<Vec<u8>>, RemoteError> {
    let items = catalog_items()?;
    let Some(item) = newer_firmware(&items, pid, running) else {
        return Ok(None);
    };
    download_item(item).map(Some)
}

#[cfg(windows)]
fn catalog_items() -> Result<Vec<FirmwareItem>, RemoteError> {
    let text = get_text(FIRMWARE_CATALOG, None)?;
    if !text.contains("<ITEM") {
        return Err(RemoteError::Json);
    }
    Ok(parse_catalog(&text))
}

#[cfg(windows)]
fn download_item(item: &FirmwareItem) -> Result<Vec<u8>, RemoteError> {
    if !(item.url.starts_with("https://") || item.url.starts_with("http://")) {
        return Err(RemoteError::Url);
    }
    let bytes = get_bytes(&item.url, None)?;
    if u64::try_from(bytes.len()).ok() != Some(item.size) {
        return Err(RemoteError::Size);
    }
    if !md5_matches(&bytes, &item.md5) {
        return Err(RemoteError::Digest);
    }
    Ok(bytes)
}

#[cfg(windows)]
fn get_text(url: &str, header: Option<&str>) -> Result<String, RemoteError> {
    let bytes = get_bytes(url, header)?;
    String::from_utf8(bytes).map_err(|_| RemoteError::Body)
}

#[cfg(windows)]
fn get_bytes(url: &str, header: Option<&str>) -> Result<Vec<u8>, RemoteError> {
    vcxtoolkit_open::http_get(url, header).map_err(|error| {
        RemoteError::Http(match error {
            vcxtoolkit_open::HttpError::Status(status) => HttpFailure::Status(status),
            _ => HttpFailure::Transport,
        })
    })
}

#[cfg(windows)]
fn unix_seconds() -> Result<u64, RemoteError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| RemoteError::Clock)
}

fn piece(bytes: &[u8]) -> Vec<u8> {
    let count = bytes.len() >> 2;
    let mut words = Vec::with_capacity(count);
    for index in 0..count {
        let mut raw = [0_u8; 4];
        raw.copy_from_slice(&bytes[index * 4..index * 4 + 4]);
        words.push(u32::from_le_bytes(raw));
    }
    decrypt_words(&mut words);
    let mut out = Vec::with_capacity(bytes.len());
    for word in words {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.extend_from_slice(&bytes[count * 4..]);
    out
}

fn decrypt_words(words: &mut [u32]) {
    let count = words.len();
    if count == 0 {
        return;
    }
    let rounds = u32::try_from(0x34 / count).unwrap_or(0);
    let mut total = 0xb54c_da56_u32.wrapping_sub(rounds.wrapping_mul(0x61c8_8647));
    let mut mixed = words[0];
    while total != 0 {
        let slot = usize::try_from((total >> 2) & 3).unwrap_or(0);
        let mut index = count;
        while index > 1 {
            index -= 1;
            let right = words[index - 1];
            words[index] =
                words[index].wrapping_sub(mix(mixed, right, total, KEY[(index & 3) ^ slot]));
            mixed = words[index];
        }
        let right = words[count - 1];
        words[0] = words[0].wrapping_sub(mix(mixed, right, total, KEY[slot]));
        total = total.wrapping_add(0x61c8_8647);
        mixed = words[0];
    }
}

fn mix(mixed: u32, right: u32, total: u32, key_word: u32) -> u32 {
    let left = mixed.wrapping_mul(4) ^ (right >> 5);
    let shifted = (mixed >> 3) ^ right.wrapping_shl(4);
    left.wrapping_add(shifted) ^ ((key_word ^ right).wrapping_add(total ^ mixed))
}

fn json_string(body: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let mut rest = body;
    while let Some(at) = rest.find(&needle) {
        let after = rest[at + needle.len()..].trim_start();
        if let Some(after) = after.strip_prefix(':') {
            let after = after.trim_start();
            if let Some(value) = after.strip_prefix('"') {
                let end = value.find('"')?;
                if !value[..end].contains('\\') {
                    return Some(value[..end].to_owned());
                }
            }
        }
        rest = &rest[at + 1..];
    }
    None
}

fn json_int(body: &str, field: &str) -> Option<i64> {
    let needle = format!("\"{field}\"");
    let mut rest = body;
    while let Some(at) = rest.find(&needle) {
        let after = rest[at + needle.len()..].trim_start();
        if let Some(after) = after.strip_prefix(':') {
            let after = after.trim_start();
            let digits: String = after
                .chars()
                .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
                .collect();
            if let Ok(value) = digits.parse() {
                return Some(value);
            }
        }
        rest = &rest[at + 1..];
    }
    None
}

fn service_or_json(body: &str) -> RemoteError {
    match json_int(body, "code") {
        Some(0) | None => RemoteError::Json,
        Some(code) => RemoteError::Service(code),
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{RemoteError, auth_url, decode_license, json_string, license_url, service_or_json};

    #[test]
    fn the_auth_url_carries_the_timestamp_and_the_sign() {
        let url = auth_url(0);
        assert!(url.starts_with("https://api.vxdiag.net/v1/auth?appid="));
        assert!(url.contains("&timestamp=0&sign="));
        assert!(!url.contains("appkey"));
    }

    #[test]
    fn a_license_hex_field_decodes() {
        let body = r#"{"error":{"code":0},"result":{"license":"00ff"}}"#;
        let hex = json_string(body, "license").expect("license");
        assert_eq!(decode_license(&hex).expect("bytes"), vec![0x00, 0xff]);
        assert_eq!(decode_license("0").unwrap_err(), RemoteError::Hex);
        assert_eq!(
            license_url("NCAGM26507").expect("url"),
            "https://api.vxdiag.net/v1/devices/NCAGM26507/license"
        );
        assert_eq!(license_url("").unwrap_err(), RemoteError::Url);
        assert_eq!(license_url("NCAGM/1").unwrap_err(), RemoteError::Url);
    }

    #[test]
    fn a_service_code_is_not_a_license() {
        let body = r#"{"error":{"code":100,"desc_en":"Invalid parameter"}}"#;
        assert!(json_string(body, "authCode").is_none());
        assert_eq!(service_or_json(body), RemoteError::Service(100));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "talks to the license server and the firmware catalog"]
    fn the_nano_license_is_retrieved_and_its_firmware_is_current() {
        let license = super::fetch_license("NCAGM26507").expect("license");
        assert_eq!(license.len(), 4076);
        let image = super::fetch_newer_firmware(837, [1, 9, 4, 2]).expect("catalog");
        assert!(image.is_none());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "downloads the catalog image for PID 837"]
    fn the_nano_catalog_image_downloads() {
        let bytes = super::fetch_firmware(837).expect("image");
        let path = std::path::Path::new(r"D:\temp\vxdiag-re\fanout\vcxa-1.9.4.2.bin");
        if path.parent().is_some_and(std::path::Path::is_dir) {
            std::fs::write(path, &bytes).expect("save");
        }
        assert_eq!(bytes.len(), 191_770);
        let header = crate::image::read_header(&bytes).expect("header");
        assert_eq!(header.version, [1, 9, 4, 2]);
        assert_eq!(header.name(), Some("VCX-At40"));
    }
}
