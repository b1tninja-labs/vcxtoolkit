//! SHA-256, MD5, and standard base64.
//!
//! The license query signs with SHA-256 and standard base64. A firmware file is
//! checked with the MD5 in the catalog. Neither hash is a crate dependency.

const SHA_K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

const MD5_T: [u32; 64] = [
    0xd76a_a478,
    0xe8c7_b756,
    0x2420_70db,
    0xc1bd_ceee,
    0xf57c_0faf,
    0x4787_c62a,
    0xa830_4613,
    0xfd46_9501,
    0x6980_98d8,
    0x8b44_f7af,
    0xffff_5bb1,
    0x895c_d7be,
    0x6b90_1122,
    0xfd98_7193,
    0xa679_438e,
    0x49b4_0821,
    0xf61e_2562,
    0xc040_b340,
    0x265e_5a51,
    0xe9b6_c7aa,
    0xd62f_105d,
    0x0244_1453,
    0xd8a1_e681,
    0xe7d3_fbc8,
    0x21e1_cde6,
    0xc337_07d6,
    0xf4d5_0d87,
    0x455a_14ed,
    0xa9e3_e905,
    0xfcef_a3f8,
    0x676f_02d9,
    0x8d2a_4c8a,
    0xfffa_3942,
    0x8771_f681,
    0x6d9d_6122,
    0xfde5_380c,
    0xa4be_ea44,
    0x4bde_cfa9,
    0xf6bb_4b60,
    0xbebf_bc70,
    0x289b_7ec6,
    0xeaa1_27fa,
    0xd4ef_3085,
    0x0488_1d05,
    0xd9d4_d039,
    0xe6db_99e5,
    0x1fa2_7cf8,
    0xc4ac_5665,
    0xf429_2244,
    0x432a_ff97,
    0xab94_23a7,
    0xfc93_a039,
    0x655b_59c3,
    0x8f0c_cc92,
    0xffef_f47d,
    0x8584_5dd1,
    0x6fa8_7e4f,
    0xfe2c_e6e0,
    0xa301_4314,
    0x4e08_11a1,
    0xf753_7e82,
    0xbd3a_f235,
    0x2ad7_d2bb,
    0xeb86_d391,
];

const MD5_SHIFTS: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let msg = padded(data, true);
    for chunk in msg.chunks_exact(64) {
        sha_block(&mut state, chunk);
    }
    let mut out = [0_u8; 32];
    for (index, word) in state.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

pub(crate) fn md5(data: &[u8]) -> [u8; 16] {
    let mut state = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    let msg = padded(data, false);
    for chunk in msg.chunks_exact(64) {
        md5_block(&mut state, chunk);
    }
    let mut out = [0_u8; 16];
    for (index, word) in state.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

pub(crate) fn base64(data: &[u8]) -> String {
    let mut out = String::new();
    let mut index = 0;
    while index + 3 <= data.len() {
        let value = (u32::from(data[index]) << 16)
            | (u32::from(data[index + 1]) << 8)
            | u32::from(data[index + 2]);
        push_base64(&mut out, value, 4);
        index += 3;
    }
    match data.len() - index {
        1 => {
            let value = u32::from(data[index]) << 16;
            push_base64(&mut out, value, 2);
            out.push_str("==");
        }
        2 => {
            let value = (u32::from(data[index]) << 16) | (u32::from(data[index + 1]) << 8);
            push_base64(&mut out, value, 3);
            out.push('=');
        }
        _ => {}
    }
    out
}

pub(crate) fn md5_matches(bytes: &[u8], hex_text: &str) -> bool {
    use std::fmt::Write;

    let digest = md5(bytes);
    let mut encoded = String::new();
    for byte in digest {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded.eq_ignore_ascii_case(hex_text)
}

fn padded(data: &[u8], big_endian_bits: bool) -> Vec<u8> {
    let bits = u64::try_from(data.len())
        .unwrap_or(u64::MAX)
        .saturating_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    if big_endian_bits {
        msg.extend_from_slice(&bits.to_be_bytes());
    } else {
        msg.extend_from_slice(&bits.to_le_bytes());
    }
    msg
}

fn sha_block(state: &mut [u32; 8], chunk: &[u8]) {
    let mut words = [0_u32; 64];
    for index in 0..16 {
        let mut raw = [0_u8; 4];
        raw.copy_from_slice(&chunk[index * 4..index * 4 + 4]);
        words[index] = u32::from_be_bytes(raw);
    }
    for index in 16..64 {
        let s0 = words[index - 15].rotate_right(7)
            ^ words[index - 15].rotate_right(18)
            ^ (words[index - 15] >> 3);
        let s1 = words[index - 2].rotate_right(17)
            ^ words[index - 2].rotate_right(19)
            ^ (words[index - 2] >> 10);
        words[index] = words[index - 16]
            .wrapping_add(s0)
            .wrapping_add(words[index - 7])
            .wrapping_add(s1);
    }
    let mut working = *state;
    for index in 0..64 {
        let s1 =
            working[4].rotate_right(6) ^ working[4].rotate_right(11) ^ working[4].rotate_right(25);
        let choose = (working[4] & working[5]) ^ (!working[4] & working[6]);
        let first = working[7]
            .wrapping_add(s1)
            .wrapping_add(choose)
            .wrapping_add(SHA_K[index])
            .wrapping_add(words[index]);
        let s0 =
            working[0].rotate_right(2) ^ working[0].rotate_right(13) ^ working[0].rotate_right(22);
        let majority =
            (working[0] & working[1]) ^ (working[0] & working[2]) ^ (working[1] & working[2]);
        let second = s0.wrapping_add(majority);
        working[7] = working[6];
        working[6] = working[5];
        working[5] = working[4];
        working[4] = working[3].wrapping_add(first);
        working[3] = working[2];
        working[2] = working[1];
        working[1] = working[0];
        working[0] = first.wrapping_add(second);
    }
    for index in 0..8 {
        state[index] = state[index].wrapping_add(working[index]);
    }
}

fn md5_block(state: &mut [u32; 4], chunk: &[u8]) {
    let mut words = [0_u32; 16];
    for index in 0..16 {
        let mut raw = [0_u8; 4];
        raw.copy_from_slice(&chunk[index * 4..index * 4 + 4]);
        words[index] = u32::from_le_bytes(raw);
    }
    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    for index in 0..64 {
        let (mixed, source) = md5_step(index, b, c, d);
        let added = mixed
            .wrapping_add(a)
            .wrapping_add(MD5_T[index])
            .wrapping_add(words[source]);
        a = d;
        d = c;
        c = b;
        b = b.wrapping_add(added.rotate_left(MD5_SHIFTS[index]));
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

fn md5_step(index: usize, b: u32, c: u32, d: u32) -> (u32, usize) {
    match index {
        0..=15 => ((b & c) | (!b & d), index),
        16..=31 => ((d & b) | (!d & c), (5 * index + 1) % 16),
        32..=47 => (b ^ c ^ d, (3 * index + 5) % 16),
        _ => (c ^ (b | !d), (7 * index) % 16),
    }
}

fn push_base64(out: &mut String, value: u32, count: usize) {
    let shifts = [18, 12, 6, 0];
    for shift in shifts.into_iter().take(count) {
        let index = usize::try_from((value >> shift) & 0x3f).unwrap_or(0);
        out.push(char::from(B64[index]));
    }
}

#[cfg(test)]
mod tests {
    use super::{base64, md5, md5_matches, sha256};

    #[test]
    fn sha256_of_abc_is_the_test_vector() {
        let digest = sha256(b"abc");
        assert_eq!(
            digest,
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad,
            ]
        );
    }

    #[test]
    fn md5_of_abc_is_the_test_vector() {
        assert_eq!(
            md5(b"abc"),
            [
                0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0, 0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1,
                0x7f, 0x72,
            ]
        );
        assert!(md5_matches(b"abc", "900150983cd24fb0d6963f7d28e17f72"));
    }

    #[test]
    fn base64_pads_and_keeps_plus() {
        assert_eq!(base64(b"hello"), "aGVsbG8=");
        assert_eq!(base64(&[0xfb, 0xef, 0xbe]), "++++");
    }
}
