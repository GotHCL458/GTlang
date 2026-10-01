//! GT 标准库 —— toml 模块（编译为 toml.dll + toml.lib）
//! Python 风格：loads(str) / load(path) —— 最小实现（键=值 逐行解析为 "k=v;k=v"）

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

/// 把 TOML 文本解析为 "k=v;k=v"（仅顶层 key=value，跳过注释/空行/表头）
fn parse_toml(text: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with('[') { continue; }
        if let Some(eq) = l.find('=') {
            let k = l[..eq].trim();
            let v = l[eq+1..].trim().trim_matches('"');
            parts.push(format!("{}={}", k, v));
        }
    }
    parts.join(";")
}

/// toml.loads(text) -> str（"k=v;k=v"）
#[no_mangle]
pub extern "C" fn py_toml_loads(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    ret_string(parse_toml(&v))
}

/// toml.load(path) -> str
#[no_mangle]
pub extern "C" fn py_toml_load(path: *const c_char) -> *mut c_char {
    let p = unsafe { to_string(path) };
    match std::fs::read_to_string(&p) {
        Ok(t) => ret_string(parse_toml(&t)),
        Err(_) => ret_string(String::new()),
    }
}

/// toml.dumps(s) -> str：把 "k=v;k=v" 转为 TOML 文本
#[no_mangle]
pub extern "C" fn py_toml_dumps(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    let mut out = String::new();
    for pair in v.split(';') {
        let pair = pair.trim();
        if pair.is_empty() { continue; }
        if let Some(eq) = pair.find('=') {
            let k = pair[..eq].trim();
            let val = pair[eq+1..].trim();
            out.push_str(k);
            out.push_str(" = ");
            out.push_str(val);
            out.push('\n');
        }
    }
    ret_string(out)
}
