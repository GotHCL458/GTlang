//! GT 标准库 —— string 模块（编译为 string.dll + string.lib）

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

unsafe fn to_str<'a>(p: *const c_char) -> &'a str {
    if p.is_null() { return ""; }
    CStr::from_ptr(p).to_str().unwrap_or("")
}

fn out_string(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

#[no_mangle]
pub extern "C" fn py_isnumeric(p: *const c_char) -> i64 {
    let s = unsafe { to_str(p) };
    if s.is_empty() { return 0; }
    if s.parse::<f64>().is_ok() { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn py_capitalize(p: *const c_char) -> *mut c_char {
    let s = unsafe { to_str(p) };
    let mut c = s.chars();
    match c.next() {
        Some(f) => { let rest: String = c.collect(); out_string(f.to_uppercase().collect::<String>() + &rest.to_lowercase()) }
        None => out_string(String::new()),
    }
}

#[no_mangle]
pub extern "C" fn py_reverse(p: *const c_char) -> *mut c_char {
    let s = unsafe { to_str(p) };
    out_string(s.chars().rev().collect())
}

#[no_mangle]
pub extern "C" fn py_count(hay: *const c_char, needle: *const c_char) -> i64 {
    let h = unsafe { to_str(hay) };
    let n = unsafe { to_str(needle) };
    if n.is_empty() { return 0; }
    h.matches(n).count() as i64
}

#[no_mangle]
pub extern "C" fn py_startswith(s: *const c_char, pre: *const c_char) -> i64 {
    let a = unsafe { to_str(s) };
    let b = unsafe { to_str(pre) };
    if a.starts_with(b) { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn py_endswith(s: *const c_char, suf: *const c_char) -> i64 {
    let a = unsafe { to_str(s) };
    let b = unsafe { to_str(suf) };
    if a.ends_with(b) { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn py_center(s: *const c_char, width: i64) -> *mut c_char {
    let t = unsafe { to_str(s) };
    let n = t.chars().count() as i64;
    if width <= n { return out_string(t.to_string()); }
    let total = (width - n) as usize;
    let left = total / 2;
    let right = total - left;
    out_string(format!("{}{}{}", " ".repeat(left), t, " ".repeat(right)))
}

#[no_mangle]
pub extern "C" fn py_zfill(s: *const c_char, width: i64) -> *mut c_char {
    let t = unsafe { to_str(s) };
    let n = t.chars().count() as i64;
    if width <= n { return out_string(t.to_string()); }
    let pad = "0".repeat((width - n) as usize);
    if let Some(rest) = t.strip_prefix('-') { out_string(format!("-{}{}", pad, rest)) }
    else { out_string(format!("{}{}", pad, t)) }
}

#[no_mangle]
pub extern "C" fn py_ljust(s: *const c_char, width: i64) -> *mut c_char {
    let t = unsafe { to_str(s) };
    let n = t.chars().count() as i64;
    if width <= n { return out_string(t.to_string()); }
    out_string(format!("{}{}", t, " ".repeat((width - n) as usize)))
}

#[no_mangle]
pub extern "C" fn py_rjust(s: *const c_char, width: i64) -> *mut c_char {
    let t = unsafe { to_str(s) };
    let n = t.chars().count() as i64;
    if width <= n { return out_string(t.to_string()); }
    out_string(format!("{}{}", " ".repeat((width - n) as usize), t))
}

#[no_mangle]
pub extern "C" fn py_title(s: *const c_char) -> *mut c_char {
    let t = unsafe { to_str(s) };
    let mut out = String::with_capacity(t.len());
    let mut new_word = true;
    for c in t.chars() {
        if c.is_alphanumeric() {
            if new_word { out.extend(c.to_uppercase()); new_word = false; }
            else { out.extend(c.to_lowercase()); }
        } else { out.push(c); new_word = true; }
    }
    out_string(out)
}

#[no_mangle]
pub extern "C" fn py_swapcase(s: *const c_char) -> *mut c_char {
    let t = unsafe { to_str(s) };
    let mut out = String::with_capacity(t.len());
    for c in t.chars() {
        if c.is_uppercase() { out.extend(c.to_lowercase()); }
        else if c.is_lowercase() { out.extend(c.to_uppercase()); }
        else { out.push(c); }
    }
    out_string(out)
}

#[no_mangle]
pub extern "C" fn py_isalpha(s: *const c_char) -> i64 {
    let t = unsafe { to_str(s) };
    if t.is_empty() { return 0; }
    if t.chars().all(|c| c.is_alphabetic()) { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn py_isspace(s: *const c_char) -> i64 {
    let t = unsafe { to_str(s) };
    if t.is_empty() { return 0; }
    if t.chars().all(|c| c.is_whitespace()) { 1 } else { 0 }
}
