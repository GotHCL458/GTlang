//! GT 标准库 —— crypto 模块（编译为 crypto.dll + crypto.lib）
//! 纯实现（无外部依赖）：SHA-256、HMAC-SHA256、随机字节、十六进制编解码。

#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

unsafe fn to_string(p: *const c_char) -> String {
    if p.is_null() { return String::new(); }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}
fn ret_string(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()).into_raw()
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 { msg.push(0); }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i*4], chunk[i*4+1], chunk[i*4+2], chunk[i*4+3]]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(t1);
            d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for i in 0..8 { out[i*4..i*4+4].copy_from_slice(&h[i].to_be_bytes()); }
    out
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes { s.push(HEX[(b >> 4) as usize] as char); s.push(HEX[(b & 0xf) as usize] as char); }
    s
}

#[no_mangle]
pub extern "C" fn py_sha256(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    ret_string(to_hex(&sha256_bytes(v.as_bytes())))
}

#[no_mangle]
pub extern "C" fn py_hmac_sha256(key: *const c_char, msg: *const c_char) -> *mut c_char {
    let k = unsafe { to_string(key) };
    let m = unsafe { to_string(msg) };
    let mut key_block = [0u8; 64];
    if k.len() > 64 {
        let d = sha256_bytes(k.as_bytes());
        key_block[..32].copy_from_slice(&d);
    } else {
        key_block[..k.len()].copy_from_slice(k.as_bytes());
    }
    let mut o_pad = [0x5cu8; 64];
    let mut i_pad = [0x36u8; 64];
    for i in 0..64 { o_pad[i] ^= key_block[i]; i_pad[i] ^= key_block[i]; }
    let mut inner = Vec::with_capacity(64 + m.len());
    inner.extend_from_slice(&i_pad);
    inner.extend_from_slice(m.as_bytes());
    let inner_hash = sha256_bytes(&inner);
    let mut outer = Vec::with_capacity(64 + 32);
    outer.extend_from_slice(&o_pad);
    outer.extend_from_slice(&inner_hash);
    ret_string(to_hex(&sha256_bytes(&outer)))
}

#[no_mangle]
pub extern "C" fn py_sha256_hexlen() -> i64 { 64 }

#[no_mangle]
pub extern "C" fn py_random_hex(n: i64) -> *mut c_char {
    let count = if n <= 0 { 16 } else { n as usize };
    let mut buf = vec![0u8; count];
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let addr = &buf as *const _ as u64;
    let mut state = t ^ ((addr as u128) << 32) as u128;
    for i in 0..count {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        buf[i] = ((state >> 33) & 0xff) as u8;
    }
    ret_string(to_hex(&buf))
}

#[no_mangle]
pub extern "C" fn py_hex_encode(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    ret_string(to_hex(v.as_bytes()))
}

#[no_mangle]
pub extern "C" fn py_hex_decode(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    let b = v.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len() / 2);
    let mut i = 0;
    while i + 1 < b.len() {
        let hi = (b[i] as char).to_digit(16);
        let lo = (b[i+1] as char).to_digit(16);
        if let (Some(h), Some(l)) = (hi, lo) { out.push(((h << 4) | l) as u8); }
        i += 2;
    }
    ret_string(String::from_utf8_lossy(&out).into_owned())
}

/// PBKDF2 简化版：反复 SHA-256 迭代（提高暴力破解成本）
const PBKDF2_ROUNDS: u32 = 20000;

fn pbkdf2_hash(pwd: &[u8], salt: &[u8], rounds: u32) -> [u8; 32] {
    let mut data = Vec::with_capacity(salt.len() + pwd.len());
    data.extend_from_slice(salt);
    data.extend_from_slice(pwd);
    let mut h = sha256_bytes(&data);
    for _ in 1..rounds {
        let mut d = Vec::with_capacity(32 + pwd.len());
        d.extend_from_slice(&h);
        d.extend_from_slice(pwd);
        h = sha256_bytes(&d);
    }
    h
}

#[no_mangle]
pub extern "C" fn py_password_hash(pwd: *const c_char, salt: *const c_char) -> *mut c_char {
    let p = unsafe { to_string(pwd) };
    let s = unsafe { to_string(salt) };
    let h = pbkdf2_hash(p.as_bytes(), s.as_bytes(), PBKDF2_ROUNDS);
    ret_string(format!("v2${}${}${}", PBKDF2_ROUNDS, s, to_hex(&h)))
}

#[no_mangle]
pub extern "C" fn py_password_verify(pwd: *const c_char, stored: *const c_char) -> i64 {
    let p = unsafe { to_string(pwd) };
    let st = unsafe { to_string(stored) };
    // 新格式：v2$rounds$salt$hash
    if let Some(rest) = st.strip_prefix("v2$") {
        let parts: Vec<&str> = rest.splitn(3, '$').collect();
        if parts.len() != 3 { return 0; }
        let rounds: u32 = parts[0].parse().unwrap_or(PBKDF2_ROUNDS);
        let salt = parts[1];
        let expect = to_hex(&pbkdf2_hash(p.as_bytes(), salt.as_bytes(), rounds));
        return ct_eq(expect.as_bytes(), parts[2].as_bytes());
    }
    // 旧格式兼容：salt$sha256(salt+pwd)
    let parts: Vec<&str> = st.splitn(2, '$').collect();
    if parts.len() != 2 { return 0; }
    let salt = parts[0];
    let mut data = Vec::with_capacity(salt.len() + p.len());
    data.extend_from_slice(salt.as_bytes());
    data.extend_from_slice(p.as_bytes());
    let expect = to_hex(&sha256_bytes(&data));
    ct_eq(expect.as_bytes(), parts[1].as_bytes())
}

/// 恒定时间字节比较
fn ct_eq(a: &[u8], b: &[u8]) -> i64 {
    if a.len() != b.len() { return 0; }
    let mut diff = 0u8;
    for i in 0..a.len() { diff |= a[i] ^ b[i]; }
    if diff == 0 { 1 } else { 0 }
}
// ---------- SHA-512 ----------
const K512: [u64; 80] = [
    0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
    0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
    0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
    0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
    0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
    0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
    0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
    0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
    0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
    0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
    0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
    0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
    0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
    0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
];

fn sha512_bytes(data: &[u8]) -> [u8; 64] {
    let mut h: [u64; 8] = [
        0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1,
        0x510e527fade682d1, 0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u128) * 8;
    msg.push(0x80);
    while msg.len() % 128 != 112 { msg.push(0); }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(128) {
        let mut w = [0u64; 80];
        for i in 0..16 {
            let mut b = [0u8; 8];
            b.copy_from_slice(&chunk[i*8..i*8+8]);
            w[i] = u64::from_be_bytes(b);
        }
        for i in 16..80 {
            let s0 = w[i-15].rotate_right(1) ^ w[i-15].rotate_right(8) ^ (w[i-15] >> 7);
            let s1 = w[i-2].rotate_right(19) ^ w[i-2].rotate_right(61) ^ (w[i-2] >> 6);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K512[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(t1);
            d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 64];
    for i in 0..8 { out[i*8..i*8+8].copy_from_slice(&h[i].to_be_bytes()); }
    out
}

#[no_mangle]
pub extern "C" fn py_sha512(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    ret_string(to_hex(&sha512_bytes(v.as_bytes())))
}

// ---------- SHA-1 ----------
fn sha1_bytes(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 { msg.push(0); }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i*4], chunk[i*4+1], chunk[i*4+2], chunk[i*4+3]]);
        }
        for i in 16..80 { w[i] = (w[i-3] ^ w[i-8] ^ w[i-14] ^ w[i-16]).rotate_left(1); }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for i in 0..80 {
            let (f, k) = if i < 20 { ((b & c) | ((!b) & d), 0x5A827999) }
                else if i < 40 { (b ^ c ^ d, 0x6ED9EBA1) }
                else if i < 60 { ((b & c) | (b & d) | (c & d), 0x8F1BBCDC) }
                else { (b ^ c ^ d, 0xCA62C1D6) };
            let tmp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
            e = d; d = c; c = b.rotate_left(30); b = a; a = tmp;
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b); h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d); h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for i in 0..5 { out[i*4..i*4+4].copy_from_slice(&h[i].to_be_bytes()); }
    out
}

#[no_mangle]
pub extern "C" fn py_sha1(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    ret_string(to_hex(&sha1_bytes(v.as_bytes())))
}

// ---------- MD5 ----------
const MD5_S: [u32; 64] = [
    7,12,17,22, 7,12,17,22, 7,12,17,22, 7,12,17,22,
    5,9,14,20, 5,9,14,20, 5,9,14,20, 5,9,14,20,
    4,11,16,23, 4,11,16,23, 4,11,16,23, 4,11,16,23,
    6,10,15,21, 6,10,15,21, 6,10,15,21, 6,10,15,21,
];
const MD5_K: [u32; 64] = [
    0xd76aa478,0xe8c7b756,0x242070db,0xc1bdceee,0xf57c0faf,0x4787c62a,0xa8304613,0xfd469501,
    0x698098d8,0x8b44f7af,0xffff5bb1,0x895cd7be,0x6b901122,0xfd987193,0xa679438e,0x49b40821,
    0xf61e2562,0xc040b340,0x265e5a51,0xe9b6c7aa,0xd62f105d,0x02441453,0xd8a1e681,0xe7d3fbc8,
    0x21e1cde6,0xc33707d6,0xf4d50d87,0x455a14ed,0xa9e3e905,0xfcefa3f8,0x676f02d9,0x8d2a4c8a,
    0xfffa3942,0x8771f681,0x6d9d6122,0xfde5380c,0xa4beea44,0x4bdecfa9,0xf6bb4b60,0xbebfbc70,
    0x289b7ec6,0xeaa127fa,0xd4ef3085,0x04881d05,0xd9d4d039,0xe6db99e5,0x1fa27cf8,0xc4ac5665,
    0xf4292244,0x432aff97,0xab9423a7,0xfc93a039,0x655b59c3,0x8f0ccc92,0xffeff47d,0x85845dd1,
    0x6fa87e4f,0xfe2ce6e0,0xa3014314,0x4e0811a1,0xf7537e82,0xbd3af235,0x2ad7d2bb,0xeb86d391,
];

fn md5_bytes(data: &[u8]) -> [u8; 16] {
    let mut a0: u32 = 0x67452301; let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe; let mut d0: u32 = 0x10325476;
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 { msg.push(0); }
    msg.extend_from_slice(&bit_len.to_le_bytes());
    for chunk in msg.chunks(64) {
        let mut m = [0u32; 16];
        for i in 0..16 { m[i] = u32::from_le_bytes([chunk[i*4], chunk[i*4+1], chunk[i*4+2], chunk[i*4+3]]); }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = if i < 16 { ((b & c) | ((!b) & d), i) }
                else if i < 32 { ((d & b) | ((!d) & c), (5*i + 1) % 16) }
                else if i < 48 { (b ^ c ^ d, (3*i + 5) % 16) }
                else { (c ^ (b | (!d)), (7*i) % 16) };
            let tmp = d;
            d = c; c = b;
            let x = a.wrapping_add(f).wrapping_add(MD5_K[i]).wrapping_add(m[g]);
            b = b.wrapping_add(x.rotate_left(MD5_S[i]));
            a = tmp;
        }
        a0 = a0.wrapping_add(a); b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c); d0 = d0.wrapping_add(d);
    }
    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out
}

#[no_mangle]
pub extern "C" fn py_md5(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    ret_string(to_hex(&md5_bytes(v.as_bytes())))
}

#[no_mangle]
pub extern "C" fn py_sha512_hexlen() -> i64 { 128 }