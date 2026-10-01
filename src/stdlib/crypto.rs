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

#[no_mangle]
pub extern "C" fn py_password_hash(pwd: *const c_char, salt: *const c_char) -> *mut c_char {
    let p = unsafe { to_string(pwd) };
    let s = unsafe { to_string(salt) };
    let mut data = Vec::with_capacity(s.len() + p.len());
    data.extend_from_slice(s.as_bytes());
    data.extend_from_slice(p.as_bytes());
    ret_string(format!("{}${}", s, to_hex(&sha256_bytes(&data))))
}

#[no_mangle]
pub extern "C" fn py_password_verify(pwd: *const c_char, stored: *const c_char) -> i64 {
    let p = unsafe { to_string(pwd) };
    let st = unsafe { to_string(stored) };
    let parts: Vec<&str> = st.splitn(2, '$').collect();
    if parts.len() != 2 { return 0; }
    let salt = parts[0];
    let mut data = Vec::with_capacity(salt.len() + p.len());
    data.extend_from_slice(salt.as_bytes());
    data.extend_from_slice(p.as_bytes());
    let expect = to_hex(&sha256_bytes(&data));
    if expect == parts[1] { 1 } else { 0 }
}