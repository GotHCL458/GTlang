//! GT 标准库 —— file 模块（编译为 file.dll + file.lib）
//! Python 风格：read_text / write_text / append_text / read_lines / exists

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

/// file.read_text(path) -> str（失败空串）
#[no_mangle]
pub extern "C" fn py_read_text(path: *const c_char) -> *mut c_char {
    let p = unsafe { to_string(path) };
    match std::fs::read_to_string(&p) { Ok(t) => ret_string(t), Err(_) => ret_string(String::new()) }
}

/// file.write_text(path, s) -> bool（覆盖）
#[no_mangle]
pub extern "C" fn py_write_text(path: *const c_char, s: *const c_char) -> std::os::raw::c_int {
    let p = unsafe { to_string(path) };
    let v = unsafe { to_string(s) };
    match std::fs::write(&p, v.as_bytes()) { Ok(_) => 1, Err(_) => 0 }
}

/// file.append_text(path, s) -> bool（追加）
#[no_mangle]
pub extern "C" fn py_append_text(path: *const c_char, s: *const c_char) -> std::os::raw::c_int {
    use std::io::Write;
    let p = unsafe { to_string(path) };
    let v = unsafe { to_string(s) };
    match std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        Ok(mut f) => match f.write_all(v.as_bytes()) { Ok(_) => 1, Err(_) => 0 },
        Err(_) => 0,
    }
}

/// file.read_lines(path) -> str（"\n" 分隔）
#[no_mangle]
pub extern "C" fn py_read_lines(path: *const c_char) -> *mut c_char {
    let p = unsafe { to_string(path) };
    match std::fs::read_to_string(&p) {
        Ok(t) => {
            let joined: Vec<String> = t.lines().map(|l| l.to_string()).collect();
            ret_string(joined.join("\n"))
        }
        Err(_) => ret_string(String::new()),
    }
}

/// file.exists(path) -> bool
#[no_mangle]
pub extern "C" fn py_file_exists(path: *const c_char) -> std::os::raw::c_int {
    let p = unsafe { to_string(path) };
    if std::path::Path::new(&p).is_file() { 1 } else { 0 }
}

/// file.copy(src, dst) -> bool
#[no_mangle]
pub extern "C" fn py_file_copy(src: *const c_char, dst: *const c_char) -> std::os::raw::c_int {
    let a = unsafe { to_string(src) };
    let b = unsafe { to_string(dst) };
    match std::fs::copy(&a, &b) { Ok(_) => 1, Err(_) => 0 }
}

/// file.size(path) -> int（字节数，失败 -1）
#[no_mangle]
pub extern "C" fn py_file_size(path: *const c_char) -> i64 {
    let p = unsafe { to_string(path) };
    std::fs::metadata(&p).map(|m| m.len() as i64).unwrap_or(-1)
}
