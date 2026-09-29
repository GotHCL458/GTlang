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

/// json.dump(s, path) -> bool：把 s 当 JSON 字符串写入文件
#[no_mangle]
pub extern "C" fn py_json_dump(s: *const c_char, path: *const c_char) -> std::os::raw::c_int {
    let v = unsafe { to_string(s) };
    let p = unsafe { to_string(path) };
    match std::fs::write(&p, v.as_bytes()) { Ok(_) => 1, Err(_) => 0 }
}

/// json.load(path) -> str：读文件
#[no_mangle]
pub extern "C" fn py_json_load(path: *const c_char) -> *mut c_char {
    let p = unsafe { to_string(path) };
    match std::fs::read_to_string(&p) { Ok(t) => ret_string(t), Err(_) => ret_string(String::new()) }
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

/// json.pretty(s) -> str：把紧凑 JSON 美化（缩进 2 空格）—— 简单缩进器
#[no_mangle]
pub extern "C" fn py_json_pretty(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    let mut out = String::with_capacity(v.len() * 2);
    let mut depth = 0usize;
    let mut in_str = false;
    let mut esc = false;
    for c in v.chars() {
        if in_str {
            out.push(c);
            if esc { esc = false; }
            else if c == '\\' { esc = true; }
            else if c == '"' { in_str = false; }
            continue;
        }
        match c {
            '"' => { in_str = true; out.push(c); }
            '{' | '[' => { out.push(c); depth += 1; out.push('\n'); for _ in 0..depth { out.push_str("  "); } }
            '}' | ']' => { depth = depth.saturating_sub(1); out.push('\n'); for _ in 0..depth { out.push_str("  "); } out.push(c); }
            ',' => { out.push(c); out.push('\n'); for _ in 0..depth { out.push_str("  "); } }
            ':' => { out.push_str(": "); }
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    ret_string(out)
}

/// json.minify(s) -> str：去掉 JSON 中字符串外的空白
#[no_mangle]
pub extern "C" fn py_json_minify(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    let mut out = String::with_capacity(v.len());
    let mut in_str = false;
    let mut esc = false;
    for c in v.chars() {
        if in_str {
            out.push(c);
            if esc { esc = false; }
            else if c == '\\' { esc = true; }
            else if c == '"' { in_str = false; }
            continue;
        }
        match c {
            '"' => { in_str = true; out.push(c); }
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    ret_string(out)
}

/// json.valid(s) -> bool：粗略校验（括号配对 + 引号闭合）
#[no_mangle]
pub extern "C" fn py_json_valid(s: *const c_char) -> std::os::raw::c_int {
    let v = unsafe { to_string(s) };
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut esc = false;
    for c in v.chars() {
        if in_str {
            if esc { esc = false; }
            else if c == '\\' { esc = true; }
            else if c == '"' { in_str = false; }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' | '[' => depth += 1,
            '}' | ']' => { depth -= 1; if depth < 0 { return 0; } }
            _ => {}
        }
    }
    if in_str || depth != 0 { 0 } else { 1 }
}
