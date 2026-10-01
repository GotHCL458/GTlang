//! GT 标准库 —— entropy 模块（编译为 entropy.dll + entropy.lib）
//! 真随机源：调用 Windows BCryptGenRandom（bcrypt.dll，系统 CSPRNG）。

#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};

fn ret_string(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()).into_raw()
}

const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x00000002;

#[link(name = "bcrypt")]
extern "system" {
    fn BCryptGenRandom(algorithm: *mut c_void, buffer: *mut u8, len: u32, flags: u32) -> i32;
}

/// 生成 n 个随机字节（失败返回空 Vec）
fn rand_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    let rc = unsafe { BCryptGenRandom(std::ptr::null_mut(), buf.as_mut_ptr(), n as u32, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    if rc != 0 { buf.clear(); }
    buf
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes { s.push(HEX[(b >> 4) as usize] as char); s.push(HEX[(b & 0xf) as usize] as char); }
    s
}

/// entropy.random_hex(n) -> str（2n 个十六进制字符，CSPRNG）
#[no_mangle]
pub extern "C" fn py_entropy_random_hex(n: i64) -> *mut c_char {
    let count = if n <= 0 { 16 } else { n as usize };
    ret_string(to_hex(&rand_bytes(count)))
}

/// entropy.random_int(max) -> i64（[0, max) 均匀随机；max <= 0 返回 0）
#[no_mangle]
pub extern "C" fn py_entropy_random_int(max: i64) -> i64 {
    if max <= 0 { return 0; }
    let b = rand_bytes(8);
    if b.len() < 8 { return 0; }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&b);
    let v = u64::from_le_bytes(arr);
    (v % (max as u64)) as i64
}

/// entropy.random_bytes(n) -> str（n 个原始字节，装进字符串返回）
#[no_mangle]
pub extern "C" fn py_entropy_random_bytes(n: i64) -> *mut c_char {
    let count = if n <= 0 { 16 } else { n as usize };
    let b = rand_bytes(count);
    ret_string(String::from_utf8_lossy(&b).into_owned())
}

/// entropy.uuid() -> str（RFC 4122 v4 格式：8-4-4-4-12 十六进制）
#[no_mangle]
pub extern "C" fn py_entropy_uuid() -> *mut c_char {
    let mut b = rand_bytes(16);
    if b.len() < 16 { return ret_string(String::new()); }
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // variant
    let h = to_hex(&b);
    ret_string(format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32]))
}