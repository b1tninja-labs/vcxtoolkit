//! The VCX firmware unlock exponent, recovered from the square-and-multiply
//! ladder trace in `keycap\pow.txt`.
//!
//! Both ladders in that trace share one exponent, and the model reproduces
//! every one of its 87 steps (see `POW_RESULT.txt`): `pow(5, EXPONENT, M)` is
//! the first ladder final and `pow(BASE_R, EXPONENT, M)` is the second, where
//! `M = 2^64 - 59`. These are the constants the firmware verifier's ladder
//! computes; they do not depend on a caller secret.

/// `2^64 - 59`, the modulus the ladder reduces by.
const MODULUS: u128 = (1_u128 << 64) - 59;

/// The shared ladder exponent.
pub const EXPONENT: u64 = 0x35FD_04B5_2A54_0154;

/// The fixed base of the second ladder.
pub const BASE_R: u64 = 0xC39D_C4C0_1277_F63F;

/// The first ladder final `pow(5, EXPONENT, 2^64 - 59)`.
pub const FINAL_5: u64 = 0x4974_619E_1C24_1D40;

/// The second ladder final `pow(BASE_R, EXPONENT, 2^64 - 59)`.
pub const FINAL_R: u64 = 0x0318_2CF6_EFAF_0326;

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

/// The firmware unlock key: `5^EXPONENT` modulo `2^64 - 59`, little-endian.
#[must_use]
pub fn unlock_key() -> [u8; 8] {
    mod_pow(5, EXPONENT).to_le_bytes()
}

/// The second ladder final: `BASE_R^EXPONENT` modulo `2^64 - 59`, little-endian.
#[must_use]
pub fn unlock_key_r() -> [u8; 8] {
    mod_pow(BASE_R, EXPONENT).to_le_bytes()
}

/// The A0 unlock handshake, as the two modular steps the device runs.
///
/// The A0 command (`80 00 A0 00`) is the firmware unlock verifier
/// (`FUN_08026b6c` -> `FUN_080105e2`): a Diffie-Hellman step over the group
/// modulo `2^64 - 59` with generator `5`. The host sends the request payload
/// [`crate::license_nonce`] = `5^secret`; the device answers with an eight-byte
/// scalar `reply`; the decipher key's second half is `reply^secret`, the shared
/// secret. [`Unlock`] names that pair so the open path reads as one operation.
///
/// It does not open a port: the live exchange is [`crate::read_license`], which
/// already runs `GetInfo` -> A0 -> `GetLicense` and returns the deciphered
/// buffer. The ladder [`EXPONENT`] is the firmware verifier's fixed exponent;
/// the per-session A0 `secret` is the caller's, so the two are distinct and
/// both are needed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Unlock {
    /// The caller's secret that produced [`Unlock::nonce`].
    pub secret: u64,
    /// The eight request bytes for header `80 00 A0 00`: `5^secret` mod `M`.
    pub nonce: [u8; 8],
}

impl Unlock {
    /// Begin an unlock with `secret`: the A0 request payload is `5^secret`.
    #[must_use]
    pub fn begin(secret: u64) -> Self {
        Self {
            secret,
            nonce: crate::license_nonce(secret),
        }
    }

    /// The decipher key's second half from the device's eight-byte `reply`:
    /// `reply^secret` mod `M`, little-endian.
    #[must_use]
    pub fn key_half(&self, reply: [u8; 8]) -> [u8; 8] {
        crate::license_key_half(self.secret, reply)
    }

    /// The full 16-byte decipher key: `serial`, then [`Unlock::key_half`].
    #[must_use]
    pub fn key(&self, serial: [u8; 8], reply: [u8; 8]) -> [u8; 16] {
        crate::license_key(serial, self.secret, reply)
    }
}

/// The A0 command header (`80 00 A0 00`), the firmware unlock verifier.
const A0_HEADER: [u8; 4] = [0x80, 0x00, 0xA0, 0x00];

/// Perform the firmware unlock over `port`: send the A0 command with
/// [`unlock_key`] (the ladder final the device's verifier computes) and return the
/// device's status byte. **A status of `0x00` means the device is now unlocked**, so
/// a following `VCX_PRO_Open` returns `0x00` instead of the locked `0xFE`. Verified
/// live on a VCX-NANO (2026-11-22): the A0 constant unlocks and the channel opens.
///
/// This is one command exchange; it opens no channel and sends no vehicle byte. Run
/// it once after the port settles, before the channel bring-up.
///
/// # Errors
///
/// Returns the [`crate::ExchangeError`] when the A0 command is not exchanged.
pub fn unlock_device<S: crate::SerialPort>(
    port: &S,
    timeout: std::time::Duration,
) -> Result<u8, crate::ExchangeError<S::Error>> {
    let reply = crate::exchange(port, A0_HEADER, &unlock_key(), timeout)?;
    Ok(reply.payload.first().copied().unwrap_or(0xFF))
}
#[cfg(test)]
mod tests {
    /// The captured A0 exchange from `unlock-ground-truth.txt` (VCX-NANO,
    /// 2026-11-22): secret `0x0123456789ABCDEF`, request nonce
    /// `81 A1 A7 F4 74 EA 5F 44`, reply scalar `D6 BE 56 49 94 1C C2 CE`.
    const CAPTURED_SECRET: u64 = 0x0123_4567_89AB_CDEF;
    const CAPTURED_NONCE: [u8; 8] = [0x81, 0xA1, 0xA7, 0xF4, 0x74, 0xEA, 0x5F, 0x44];
    const CAPTURED_REPLY: [u8; 8] = [0xD6, 0xBE, 0x56, 0x49, 0x94, 0x1C, 0xC2, 0xCE];

    #[test]
    fn begin_sends_the_captured_a0_nonce() {
        assert_eq!(super::Unlock::begin(CAPTURED_SECRET).nonce, CAPTURED_NONCE);
    }

    #[test]
    fn the_handshake_key_half_is_the_reply_raised_to_the_secret() {
        let unlock = super::Unlock::begin(CAPTURED_SECRET);
        assert_eq!(
            unlock.key_half(CAPTURED_REPLY),
            crate::license_key_half(CAPTURED_SECRET, CAPTURED_REPLY)
        );
    }

    #[test]
    fn the_handshake_key_is_serial_then_key_half() {
        let serial = [0x4E, 0x43, 0x41, 0x47, 0x4D, 0x67, 0x8B, 0x28];
        let unlock = super::Unlock::begin(CAPTURED_SECRET);
        let key = unlock.key(serial, CAPTURED_REPLY);
        assert_eq!(&key[..8], &serial);
        assert_eq!(&key[8..], &unlock.key_half(CAPTURED_REPLY));
    }

    /// The ladder finals pinned from the trace, little-endian.
    const FINAL_5_BYTES: [u8; 8] = [0x40, 0x1D, 0x24, 0x1C, 0x9E, 0x61, 0x74, 0x49];
    const FINAL_R_BYTES: [u8; 8] = [0x26, 0x03, 0xAF, 0xEF, 0xF6, 0x2C, 0x18, 0x03];

    #[test]
    fn the_exponent_reproduces_the_first_ladder_final() {
        assert_eq!(super::unlock_key(), FINAL_5_BYTES);
    }

    #[test]
    fn the_exponent_reproduces_the_second_ladder_final() {
        assert_eq!(super::unlock_key_r(), FINAL_R_BYTES);
    }

    #[test]
    fn the_pinned_constants_match_the_computed_ones() {
        assert_eq!(super::FINAL_5.to_le_bytes(), FINAL_5_BYTES);
        assert_eq!(super::FINAL_R.to_le_bytes(), FINAL_R_BYTES);
    }
}
