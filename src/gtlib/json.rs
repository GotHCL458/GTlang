//! GT 标准库 —— json 模块（编译为 json.dll + json.lib）
//! Python 风格：dumps(str) / loads(str) —— 最小实现（转义/反转义）

#![allow(clippy::missing_safety_doc)]

use std::ffi::CStr;
use std::os::raw::c_char;

unsafe fn to_string(p: *const c_char) -> String {
    if p.is_null() { return String::new(); }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}
#[link(name = "kernel32")]
extern "system" {
    fn GetProcessHeap() -> *mut core::ffi::c_void;
    fn HeapAlloc(h: *mut core::ffi::c_void, flags: u32, n: usize) -> *mut u8;
}
pub(crate) unsafe fn heap_alloc(n: usize) -> *mut u8 { HeapAlloc(GetProcessHeap(), 0, n) }

// 统一分配器：用 Windows 进程堆（HeapAlloc），跨 CRT 释放安全
// （标准库 DLL 与 GTLang 运行时处于不同 CRT；进程堆是进程共享的，
//  任何 CRT 都能 HeapFree 释放对方分配的内存）。
fn ret_string(s: String) -> *mut c_char {
    let bytes = s.as_bytes();
    let p = unsafe { heap_alloc(bytes.len() + 1) };
    if p.is_null() { return std::ptr::null_mut(); }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        *p.add(bytes.len()) = 0;
    }
    p as *mut c_char
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
    if in_str || depth != 0 { return 0; }
    // 粗略校验通过后，再检查顶层是合法 JSON 值（拒绝裸标识符如 "bad"）
    let t = v.trim();
    if t.is_empty() { return 0; }
    let first = t.chars().next().unwrap();
    let ok = match first {
        '{' | '[' | '"' => true,
        't' => t.starts_with("true"),
        'f' => t.starts_with("false"),
        'n' => t.starts_with("null"),
        c if c == '-' || c.is_ascii_digit() => t.parse::<f64>().is_ok(),
        _ => false,
    };
    if !ok { return 0; }
    // 若以 true/false/null 开头，后面只能是空白
    if matches!(first, 't' | 'f' | 'n') {
        let lit = if first == 't' { "true" } else if first == 'f' { "false" } else { "null" };
        if t != lit { return 0; }
    }
    1
}

/// json.escape(s) -> str：转义为 JSON 字符串（带引号）
#[no_mangle]
pub extern "C" fn py_json_escape(s: *const c_char) -> *mut c_char {
    py_json_dumps(s)
}

/// json.number(v) -> str：整数 → JSON
#[no_mangle]
pub extern "C" fn py_json_number(v: i64) -> *mut c_char {
    ret_string(v.to_string())
}

/// json.number_f(v) -> str：浮点 → JSON
#[no_mangle]
pub extern "C" fn py_json_number_f(v: f64) -> *mut c_char {
    ret_string(format!("{}", v))
}

/// json.bool(v) -> str
#[no_mangle]
pub extern "C" fn py_json_bool(v: i64) -> *mut c_char {
    ret_string(if v != 0 { "true".to_string() } else { "false".to_string() })
}

/// json.null() -> str
#[no_mangle]
pub extern "C" fn py_json_null() -> *mut c_char {
    ret_string("null".to_string())
}

/// json.array(items) -> str：items 以 "\n" 分隔，每个已是 JSON 元素 → "[e1,e2]"
#[no_mangle]
pub extern "C" fn py_json_array(items: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(items) };
    let parts: Vec<&str> = v.split('\n').filter(|s| !s.is_empty()).collect();
    ret_string(format!("[{}]", parts.join(",")))
}

/// json.object(kv) -> str：kv 以 "k=v;k=v" 分隔，v 已是 JSON 值 → {"k":v,...}
#[no_mangle]
pub extern "C" fn py_json_object(kv: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(kv) };
    let mut parts: Vec<String> = Vec::new();
    for pair in v.split(';') {
        let pair = pair.trim();
        if pair.is_empty() { continue; }
        if let Some(eq) = pair.find('=') {
            let k = &pair[..eq];
            let val = &pair[eq+1..];
            parts.push(format!("\"{}\":{}", k, val));
        }
    }
    ret_string(format!("{{{}}}", parts.join(",")))
}

/// json.unquote(s) -> str：若 s 是 JSON 字符串（带引号），去引号 + 反转义
#[no_mangle]
pub extern "C" fn py_json_unquote(s: *const c_char) -> *mut c_char {
    py_json_loads(s)
}
