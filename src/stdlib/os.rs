//! GT 标准库 —— os 模块（编译为 os.dll + os.lib）
//! Python 风格：getcwd / getenv / setenv / path_exists / remove / mkdir / system

#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

unsafe fn cstr_to_string(p: *const c_char) -> String {
    if p.is_null() { return String::new(); }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}
fn to_cstring(s: *const c_char) -> Option<CString> {
    if s.is_null() { return None; }
    let bytes = unsafe { CStr::from_ptr(s).to_bytes().to_vec() };
    CString::new(bytes).ok()
}
fn ret_string(s: String) -> *mut c_char {
    let c = CString::new(s).unwrap_or_else(|_| CString::new("").unwrap());
    c.into_raw()
}

/// os.getcwd() -> str
#[no_mangle]
pub extern "C" fn py_getcwd() -> *mut c_char {
    ret_string(std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())
}

/// os.getenv(name) -> str（不存在返回空串）
#[no_mangle]
pub extern "C" fn py_getenv(name: *const c_char) -> *mut c_char {
    let k = unsafe { cstr_to_string(name) };
    ret_string(std::env::var(&k).unwrap_or_default())
}

/// os.setenv(name, value) -> bool
#[no_mangle]
pub extern "C" fn py_setenv(name: *const c_char, value: *const c_char) -> c_int {
    let k = unsafe { cstr_to_string(name) };
    let v = unsafe { cstr_to_string(value) };
    std::env::set_var(&k, &v);
    1
}

/// os.path_exists(path) -> bool
#[no_mangle]
pub extern "C" fn py_path_exists(path: *const c_char) -> c_int {
    let p = unsafe { cstr_to_string(path) };
    if std::path::Path::new(&p).exists() { 1 } else { 0 }
}

/// os.remove(path) -> bool
#[no_mangle]
pub extern "C" fn py_remove(path: *const c_char) -> c_int {
    let p = unsafe { cstr_to_string(path) };
    match std::fs::remove_file(&p) {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

/// os.mkdir(path) -> bool
#[no_mangle]
pub extern "C" fn py_mkdir(path: *const c_char) -> c_int {
    let p = unsafe { cstr_to_string(path) };
    match std::fs::create_dir(&p) {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

/// os.system(cmd) -> int（返回退出码）
#[no_mangle]
pub extern "C" fn py_system(cmd: *const c_char) -> i64 {
    let c = unsafe { cstr_to_string(cmd) };
    if c.is_empty() { return -1; }
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd").arg("/C").arg(&c).status();
        status.map(|s| s.code().unwrap_or(-1) as i64).unwrap_or(-1)
    }
    #[cfg(not(windows))]
    {
        let status = std::process::Command::new("sh").arg("-c").arg(&c).status();
        status.map(|s| s.code().unwrap_or(-1) as i64).unwrap_or(-1)
    }
}
