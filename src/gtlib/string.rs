//! GT 标准库 —— string 模块（编译为 string.dll + string.lib）

use std::ffi::CStr;
use std::os::raw::c_char;

unsafe fn to_str<'a>(p: *const c_char) -> &'a str {
    if p.is_null() { return ""; }
    CStr::from_ptr(p).to_str().unwrap_or("")
}

#[link(name = "kernel32")]
extern "system" {
    fn GetProcessHeap() -> *mut core::ffi::c_void;
    fn HeapAlloc(h: *mut core::ffi::c_void, flags: u32, n: usize) -> *mut u8;
}
pub(crate) unsafe fn heap_alloc(n: usize) -> *mut u8 { HeapAlloc(GetProcessHeap(), 0, n) }

fn out_string(s: String) -> *mut c_char {
    let bytes = s.as_bytes();
    let p = unsafe { heap_alloc(bytes.len() + 1) };
    if p.is_null() { return std::ptr::null_mut(); }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        *p.add(bytes.len()) = 0;
    }
    p as *mut c_char
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

// ---------- Python 风格增强 ----------

/// strip(s[, chars])：去两端空白（或指定字符集）
#[no_mangle]
pub extern "C" fn py_strip(s: *const c_char, chars: *const c_char) -> *mut c_char {
    let v = unsafe { to_str(s) };
    let c = unsafe { to_str(chars) };
    let trimmed = if c.is_empty() { v.trim_matches(|x: char| x.is_whitespace()) } else { v.trim_matches(|x: char| c.contains(x)) };
    out_string(trimmed.to_string())
}

/// lstrip
#[no_mangle]
pub extern "C" fn py_lstrip(s: *const c_char, chars: *const c_char) -> *mut c_char {
    let v = unsafe { to_str(s) };
    let c = unsafe { to_str(chars) };
    let t = if c.is_empty() { v.trim_start_matches(|x: char| x.is_whitespace()) } else { v.trim_start_matches(|x: char| c.contains(x)) };
    out_string(t.to_string())
}

/// rstrip
#[no_mangle]
pub extern "C" fn py_rstrip(s: *const c_char, chars: *const c_char) -> *mut c_char {
    let v = unsafe { to_str(s) };
    let c = unsafe { to_str(chars) };
    let t = if c.is_empty() { v.trim_end_matches(|x: char| x.is_whitespace()) } else { v.trim_end_matches(|x: char| c.contains(x)) };
    out_string(t.to_string())
}

/// index(s, sub) -> int（找不到 -1）
#[no_mangle]
pub extern "C" fn py_index(s: *const c_char, sub: *const c_char) -> i64 {
    let v = unsafe { to_str(s) };
    let n = unsafe { to_str(sub) };
    match v.find(n) { Some(i) => i as i64, None => -1 }
}

/// rindex(s, sub) -> int（从右找）
#[no_mangle]
pub extern "C" fn py_rindex(s: *const c_char, sub: *const c_char) -> i64 {
    let v = unsafe { to_str(s) };
    let n = unsafe { to_str(sub) };
    match v.rfind(n) { Some(i) => i as i64, None => -1 }
}

/// replace_all(s, from, to) -> str
#[no_mangle]
pub extern "C" fn py_replace_all(s: *const c_char, from: *const c_char, to: *const c_char) -> *mut c_char {
    let v = unsafe { to_str(s) };
    let f = unsafe { to_str(from) };
    let t = unsafe { to_str(to) };
    out_string(v.replace(f, t))
}

/// join_list(sep, s) -> str（s 以 "\n" 分隔）
#[no_mangle]
pub extern "C" fn py_join_list(sep: *const c_char, s: *const c_char) -> *mut c_char {
    let sp = unsafe { to_str(sep) };
    let v = unsafe { to_str(s) };
    let parts: Vec<&str> = v.split('\n').collect();
    out_string(parts.join(sp))
}

/// split_str(s, sep) -> str（"\n" 分隔）
#[no_mangle]
pub extern "C" fn py_split_str(s: *const c_char, sep: *const c_char) -> *mut c_char {
    let v = unsafe { to_str(s) };
    let sp = unsafe { to_str(sep) };
    let parts: Vec<&str> = if sp.is_empty() { v.split_whitespace().collect() } else { v.split(sp).collect() };
    out_string(parts.join("\n"))
}

/// format(tmpl, args)：tmpl 里的 "{}" 用 args（"\n" 分隔）依次替换
#[no_mangle]
pub extern "C" fn py_format(tmpl: *const c_char, args: *const c_char) -> *mut c_char {
    let t = unsafe { to_str(tmpl) };
    let a = unsafe { to_str(args) };
    let mut out = String::with_capacity(t.len() + a.len());
    let mut it = a.split('\n');
    let mut rest = t;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        out.push_str(it.next().unwrap_or(""));
        rest = &rest[i+2..];
    }
    out.push_str(rest);
    out_string(out)
}

/// isdigit(s) -> bool
#[no_mangle]
pub extern "C" fn py_isdigit(s: *const c_char) -> i64 {
    let v = unsafe { to_str(s) };
    if v.is_empty() { return 0; }
    if v.chars().all(|c| c.is_ascii_digit()) { 1 } else { 0 }
}

/// isalnum(s) -> bool
#[no_mangle]
pub extern "C" fn py_isalnum(s: *const c_char) -> i64 {
    let v = unsafe { to_str(s) };
    if v.is_empty() { return 0; }
    if v.chars().all(|c| c.is_alphanumeric()) { 1 } else { 0 }
}

/// islower(s) / isupper(s)
#[no_mangle]
pub extern "C" fn py_islower(s: *const c_char) -> i64 {
    let v = unsafe { to_str(s) };
    if v.is_empty() { return 0; }
    if v.chars().any(|c| c.is_lowercase()) && !v.chars().any(|c| c.is_uppercase()) { 1 } else { 0 }
}
#[no_mangle]
pub extern "C" fn py_isupper(s: *const c_char) -> i64 {
    let v = unsafe { to_str(s) };
    if v.is_empty() { return 0; }
    if v.chars().any(|c| c.is_uppercase()) && !v.chars().any(|c| c.is_lowercase()) { 1 } else { 0 }
}

/// partition(s, sep) -> str（"before\nsep\nafter"）
#[no_mangle]
pub extern "C" fn py_partition(s: *const c_char, sep: *const c_char) -> *mut c_char {
    let v = unsafe { to_str(s) };
    let sp = unsafe { to_str(sep) };
    match v.find(sp) {
        Some(i) => out_string(format!("{}\n{}\n{}", &v[..i], sp, &v[i+sp.len()..])),
        None => out_string(format!("{}\n\n", v)),
    }
}

/// rpartition(s, sep) -> str
#[no_mangle]
pub extern "C" fn py_rpartition(s: *const c_char, sep: *const c_char) -> *mut c_char {
    let v = unsafe { to_str(s) };
    let sp = unsafe { to_str(sep) };
    match v.rfind(sp) {
        Some(i) => out_string(format!("{}\n{}\n{}", &v[..i], sp, &v[i+sp.len()..])),
        None => out_string(format!("\n\n{}", v)),
    }
}

/// contains(s, sub) -> bool
#[no_mangle]
pub extern "C" fn py_contains(s: *const c_char, sub: *const c_char) -> i64 {
    let v = unsafe { to_str(s) };
    let n = unsafe { to_str(sub) };
    if v.contains(n) { 1 } else { 0 }
}

/// is_ascii(s) -> bool
#[no_mangle]
pub extern "C" fn py_is_ascii(s: *const c_char) -> i64 {
    let v = unsafe { to_str(s) };
    if v.is_ascii() { 1 } else { 0 }
}

/// utf8_len(s) -> int（字符数，非字节数）
#[no_mangle]
pub extern "C" fn py_utf8_len(s: *const c_char) -> i64 {
    let v = unsafe { to_str(s) };
    v.chars().count() as i64
}
