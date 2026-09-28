//! GT 标准库 —— json 模块（编译为 json.dll + json.lib）
//! Python 风格：dumps(str) / loads(str) —— 最小实现（转义/反转义）

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

/// json.dumps(s) -> str：返回带引号并转义的 JSON 字符串
#[no_mangle]
pub extern "C" fn py_json_dumps(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    let mut out = String::with_capacity(v.len() + 2);
    out.push('"');
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    ret_string(out)
}

/// json.loads(s) -> str：把 JSON 字符串字面量反转义（要求带引号）
#[no_mangle]
pub extern "C" fn py_json_loads(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    let bytes: Vec<char> = v.chars().collect();
    let n = bytes.len();
    let mut i = 0usize;
    if i < n && bytes[i] == '"' { i += 1; }
    let mut out = String::new();
    while i < n {
        let c = bytes[i];
        if c == '"' { break; }
        if c == '\\' && i + 1 < n {
            i += 1;
            match bytes[i] {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'u' => {
                    if i + 4 < n {
                        let hex: String = bytes[i+1..i+5].iter().collect();
                        if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                            if let Some(ch) = char::from_u32(cp) { out.push(ch); }
                        }
                        i += 4;
                    }
                }
                other => out.push(other),
            }
            i += 1;
        } else {
            out.push(c);
            i += 1;
        }
    }
    ret_string(out)
}
