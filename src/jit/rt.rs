//! JIT 运行时：rt_* 函数（C ABI）+ 容器/字符串/内存辅助。

use super::*;

// ============================================================
// 供 JIT 代码调用的运行时（Rust 实现，直接注册为符号）
// ============================================================

/// 原样写出一段 UTF-8 字节
pub(crate) extern "C" fn rt_write(ptr: i64, len: i64) {
    if ptr == 0 || len <= 0 {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let mut out = std::io::stdout();
    let _ = std::io::Write::write_all(&mut out, bytes);
}

/// 写 NUL 结尾的 C 字符串
pub(crate) extern "C" fn rt_write_cstr(ptr: i64) {
    if ptr == 0 {
        return;
    }
    let s = unsafe { std::ffi::CStr::from_ptr(ptr as *const i8) };
    let mut out = std::io::stdout();
    let _ = std::io::Write::write_all(&mut out, s.to_bytes());
}

pub(crate) extern "C" fn rt_write_i64(v: i64) {
    print!("{}", v);
}

pub(crate) extern "C" fn rt_write_f64(v: f64) {
    print!("{}", fmt_g(v));
}

pub(crate) extern "C" fn rt_write_bool(v: i64) {
    print!("{}", v != 0);
}

/// 比较两个 NUL 结尾字符串是否相等
pub(crate) extern "C" fn rt_str_eq(a: i64, b: i64) -> i64 {
    if a == b {
        return 1;
    }
    if a == 0 || b == 0 {
        return 0;
    }
    let sa = unsafe { std::ffi::CStr::from_ptr(a as *const i8) };
    let sb = unsafe { std::ffi::CStr::from_ptr(b as *const i8) };
    if sa == sb {
        1
    } else {
        0
    }
}

/// 取 NUL 结尾字符串的字节长度
pub(crate) extern "C" fn rt_str_len(p: i64) -> i64 {
    if p == 0 {
        return 0;
    }
    unsafe { std::ffi::CStr::from_ptr(p as *const i8) }
        .to_bytes()
        .len() as i64
}

/// 字符串是否非空（bool(str) 用）
pub(crate) extern "C" fn rt_str_nonempty(p: i64) -> i64 {
    if p == 0 {
        return 0;
    }
    if unsafe { std::ffi::CStr::from_ptr(p as *const i8) }
        .to_bytes()
        .is_empty()
    {
        0
    } else {
        1
    }
}

/// 字符串 → 整数：解析开头可选数值前缀，忽略尾部；
/// 固定十进制，与 LLVM 后端的 `strtoll(s, NULL, 10)` 语义一致。
pub(crate) extern "C" fn rt_to_i64(p: i64) -> i64 {
    if p == 0 {
        return 0;
    }
    let s = unsafe { std::ffi::CStr::from_ptr(p as *const i8) }.to_string_lossy();
    let t = s.trim_start();
    let (neg, digits) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let mut v: i64 = 0;
    let mut any = false;
    for ch in digits.chars() {
        match ch.to_digit(10) {
            Some(d) => {
                any = true;
                v = v.wrapping_mul(10).wrapping_add(d as i64);
            }
            None => break,
        }
    }
    if !any {
        return 0;
    }
    if neg {
        -v
    } else {
        v
    }
}

/// 字符串 → 浮点：与 LLVM 后端的 `strtod` 保持同样的"前缀解析"语义
pub(crate) extern "C" fn rt_to_f64(p: i64) -> f64 {
    if p == 0 {
        return 0.0;
    }
    let s = unsafe { std::ffi::CStr::from_ptr(p as *const i8) }.to_string_lossy();
    let t = s.trim_start();
    let bytes: Vec<char> = t.chars().collect();
    let mut i = 0usize;
    if i < bytes.len() && (bytes[i] == '+' || bytes[i] == '-') {
        i += 1;
    }
    let mut seen_digit = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
        seen_digit = true;
    }
    if i < bytes.len() && bytes[i] == '.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            seen_digit = true;
        }
    }
    if seen_digit && i < bytes.len() && (bytes[i] == 'e' || bytes[i] == 'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == '+' || bytes[j] == '-') {
            j += 1;
        }
        if j < bytes.len() && bytes[j].is_ascii_digit() {
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    if !seen_digit {
        return 0.0;
    }
    bytes[..i].iter().collect::<String>().parse().unwrap_or(0.0)
}

/// 整数除零：打印诊断并终止（与编译器后端的 gt_div_zero 同语义）
pub(crate) extern "C" fn rt_div_zero(line: i64) {
    if crate::lang::is_zh() {
        eprintln!("\n[运行时错误] 第 {} 行：整数除数为 0", line);
    } else {
        eprintln!("\n[runtime error] line {}: integer division by zero", line);
    }
    std::process::exit(1);
}

/// 整数算术溢出：打印诊断并终止（与编译器后端的 gt_overflow 同语义）
pub(crate) extern "C" fn rt_overflow(line: i64) {
    if crate::lang::is_zh() {
        eprintln!("
[运行时错误] 第 {} 行：整数运算溢出", line);
    } else {
        eprintln!("
[runtime error] line {}: integer arithmetic overflow", line);
    }
    std::process::exit(1);
}

/// 数组/字符串下标越界：打印诊断并终止进程（与编译器后端的 gt_bounds 同语义）
pub(crate) extern "C" fn rt_bounds(idx: i64, len: i64, line: i64) {
    if crate::lang::is_zh() {
        eprintln!("\n[运行时错误] 第 {} 行：下标 {} 越界（长度 {}）", line, idx, len);
    } else {
        eprintln!("\n[runtime error] line {}: index {} out of bounds (length {})", line, idx, len);
    }
    std::process::exit(1);
}

/// 模拟 C 的 `%g`（默认精度 6），使解释器与编译器的浮点输出逐字节一致。
pub(crate) fn fmt_g(v: f64) -> String {
    if v.is_nan() {
        return "nan".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    const P: i32 = 6;
    let exp = v.abs().log10().floor() as i32;
    if exp < -4 || exp >= P {
        // 科学计数法：尾数 P-1 位小数，再去掉尾随 0
        let raw = format!("{:.*e}", (P - 1) as usize, v);
        let (mant, e) = raw.split_once('e').unwrap_or((raw.as_str(), "0"));
        let mant = strip_zeros(mant);
        let ev: i32 = e.parse().unwrap_or(0);
        format!("{}e{}{:02}", mant, if ev < 0 { '-' } else { '+' }, ev.abs())
    } else {
        let prec = (P - 1 - exp).max(0) as usize;
        strip_zeros(&format!("{:.*}", prec, v))
    }
}

/// 去掉小数部分末尾多余的 0 与孤立的小数点
pub(crate) fn strip_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let t = s.trim_end_matches('0');
    t.trim_end_matches('.').to_string()
}

// ------------------------------------------------------------
// 字符串构造器：让插值字符串可以作为"值"使用（不只是 put 的参数）
// ------------------------------------------------------------

thread_local! {
    /// 句柄 → 缓冲区。用 HashMap 保证 push/finish 都是 O(1)；
    /// 嵌套插值会产生多个并存的句柄，不能用简单栈。
    static BUILDERS: std::cell::RefCell<HashMap<i64, String>> =
        std::cell::RefCell::new(HashMap::new());
    /// 已完成字符串的环形池，保证返回指针在短期内存活
    static SB_POOL: std::cell::RefCell<VecDeque<std::ffi::CString>> =
        std::cell::RefCell::new(VecDeque::new());
    static NEXT_HANDLE: std::cell::Cell<i64> = const { std::cell::Cell::new(1) };
}

pub(crate) extern "C" fn rt_sb_new() -> i64 {
    NEXT_HANDLE.with(|c| {
        let h = c.get();
        c.set(h + 1);
        BUILDERS.with(|bs| bs.borrow_mut().insert(h, String::new()));
        h
    })
}

/// 直接对指定句柄的缓冲区做一次原地追加，避免中间临时 String。
pub(crate) fn sb_with<F: FnOnce(&mut String)>(h: i64, f: F) {
    BUILDERS.with(|bs| {
        if let Some(buf) = bs.borrow_mut().get_mut(&h) {
            f(buf);
        }
    });
}

pub(crate) extern "C" fn rt_sb_push_str(h: i64, p: i64) {
    if p == 0 {
        return;
    }
    let bytes = unsafe { std::ffi::CStr::from_ptr(p as *const i8) }.to_bytes();
    // 合法 UTF-8 占绝大多数，走无分配的零拷贝路径
    match std::str::from_utf8(bytes) {
        Ok(s) => sb_with(h, |buf| buf.push_str(s)),
        Err(_) => sb_with(h, |buf| buf.push_str(&String::from_utf8_lossy(bytes))),
    }
}

pub(crate) extern "C" fn rt_sb_push_i64(h: i64, v: i64) {
    sb_with(h, |buf| {
        let _ = write!(buf, "{}", v);
    });
}

pub(crate) extern "C" fn rt_sb_push_f64(h: i64, v: f64) {
    let s = fmt_g(v);
    sb_with(h, |buf| buf.push_str(&s));
}

pub(crate) extern "C" fn rt_sb_push_bool(h: i64, v: i64) {
    sb_with(h, |buf| buf.push_str(if v != 0 { "true" } else { "false" }));
}

/// 收尾：返回 NUL 结尾字符串的指针（放入环形池，短期有效）
pub(crate) extern "C" fn rt_sb_finish(h: i64) -> i64 {
    let s = BUILDERS
        .with(|bs| bs.borrow_mut().remove(&h))
        .unwrap_or_default();
    let c = std::ffi::CString::new(s).unwrap_or_default();
    SB_POOL.with(|pool| {
        let mut p = pool.borrow_mut();
        if p.len() >= 32 {
            p.pop_front();
        }
        p.push_back(c);
        p.back().unwrap().as_ptr() as i64
    })
}


// ============================================================
// 容器运行时（Rust 版，与 gt_rt.c 的 gt_list_*/gt_set_*/gt_map_* 语义一致）
//
// 容器在 GTLang 侧统一是 I64 指针；元素/键/值一律 8 字节槽。
// 数据布局与 C 版逐字段对齐，保证双后端行为一致。
// ============================================================

#[repr(C)]
pub(crate) struct RtList {
    data: *mut i64,
    len: i64,
    cap: i64,
}

pub(crate) fn rt_list_new() -> *mut RtList {
    let mut v: Vec<i64> = Vec::with_capacity(4);
    let data = v.as_mut_ptr();
    std::mem::forget(v);
    Box::into_raw(Box::new(RtList { data, len: 0, cap: 4 }))
}

pub(crate) unsafe fn rt_list_grow(l: *mut RtList) {
    let l = &mut *l;
    if l.len >= l.cap {
        l.cap *= 2;
        let mut v = Vec::from_raw_parts(l.data, l.len as usize, l.len as usize);
        v.reserve((l.cap - l.len) as usize);
        l.data = v.as_mut_ptr();
        std::mem::forget(v);
    }
}

pub(crate) extern "C" fn rt_list_push(l: *mut RtList, v: i64) {
    if l.is_null() { return; }
    unsafe {
        rt_list_grow(l);
        let l = &mut *l;
        *l.data.add(l.len as usize) = v;
        l.len += 1;
    }
}

pub(crate) extern "C" fn rt_list_pop(l: *mut RtList) -> i64 {
    if l.is_null() { return 0; }
    unsafe {
        let l = &mut *l;
        if l.len == 0 { return 0; }
        l.len -= 1;
        *l.data.add(l.len as usize)
    }
}

pub(crate) extern "C" fn rt_list_at(l: *mut RtList, i: i64) -> i64 {
    if l.is_null() { return 0; }
    unsafe {
        let l = &*l;
        if i < 0 || i >= l.len { return 0; }
        *l.data.add(i as usize)
    }
}

pub(crate) extern "C" fn rt_list_set(l: *mut RtList, i: i64, v: i64) {
    if l.is_null() { return; }
    unsafe {
        let l = &mut *l;
        if i >= 0 && i < l.len { *l.data.add(i as usize) = v; }
    }
}

pub(crate) extern "C" fn rt_list_len(l: *mut RtList) -> i64 {
    if l.is_null() { 0 } else { unsafe { (*l).len } }
}

pub(crate) extern "C" fn rt_list_has(l: *mut RtList, v: i64) -> i64 {
    if l.is_null() { return 0; }
    unsafe {
        let l = &*l;
        for i in 0..l.len {
            if *l.data.add(i as usize) == v { return 1; }
        }
        0
    }
}

pub(crate) extern "C" fn rt_list_remove(l: *mut RtList, i: i64) {
    if l.is_null() { return; }
    unsafe {
        let l = &mut *l;
        if i < 0 || i >= l.len { return; }
        let mut k = i;
        while k + 1 < l.len {
            *l.data.add(k as usize) = *l.data.add((k + 1) as usize);
            k += 1;
        }
        l.len -= 1;
    }
}

// ---------- set ----------
#[repr(C)]
pub(crate) struct RtSet {
    data: *mut i64,
    len: i64,
    cap: i64,
}

pub(crate) fn rt_set_new() -> *mut RtSet {
    let mut v: Vec<i64> = Vec::with_capacity(4);
    let data = v.as_mut_ptr();
    std::mem::forget(v);
    Box::into_raw(Box::new(RtSet { data, len: 0, cap: 4 }))
}

pub(crate) extern "C" fn rt_set_insert(s: *mut RtSet, v: i64) {
    if s.is_null() { return; }
    unsafe {
        let s = &mut *s;
        for i in 0..s.len {
            if *s.data.add(i as usize) == v { return; }
        }
        if s.len >= s.cap {
            s.cap *= 2;
            let mut vec = Vec::from_raw_parts(s.data, s.len as usize, s.len as usize);
            vec.reserve((s.cap - s.len) as usize);
            s.data = vec.as_mut_ptr();
            std::mem::forget(vec);
        }
        *s.data.add(s.len as usize) = v;
        s.len += 1;
    }
}

pub(crate) extern "C" fn rt_set_has(s: *mut RtSet, v: i64) -> i64 {
    if s.is_null() { return 0; }
    unsafe {
        let s = &*s;
        for i in 0..s.len {
            if *s.data.add(i as usize) == v { return 1; }
        }
        0
    }
}

pub(crate) extern "C" fn rt_set_remove(s: *mut RtSet, v: i64) {
    if s.is_null() { return; }
    unsafe {
        let s = &mut *s;
        for i in 0..s.len {
            if *s.data.add(i as usize) == v {
                let mut k = i;
                while k + 1 < s.len {
                    *s.data.add(k as usize) = *s.data.add((k + 1) as usize);
                    k += 1;
                }
                s.len -= 1;
                return;
            }
        }
    }
}

pub(crate) extern "C" fn rt_set_len(s: *mut RtSet) -> i64 {
    if s.is_null() { 0 } else { unsafe { (*s).len } }
}

// ---------- map ----------
#[repr(C)]
pub(crate) struct RtMap {
    keys: *mut i64,
    vals: *mut i64,
    len: i64,
    cap: i64,
}

pub(crate) fn rt_map_new() -> *mut RtMap {
    let mut k: Vec<i64> = Vec::with_capacity(4);
    let mut v: Vec<i64> = Vec::with_capacity(4);
    let (kp, vp) = (k.as_mut_ptr(), v.as_mut_ptr());
    std::mem::forget(k);
    std::mem::forget(v);
    Box::into_raw(Box::new(RtMap { keys: kp, vals: vp, len: 0, cap: 4 }))
}

pub(crate) unsafe fn rt_map_index(m: *mut RtMap, k: i64) -> i64 {
    let m = &*m;
    for i in 0..m.len {
        if *m.keys.add(i as usize) == k { return i; }
    }
    -1
}

pub(crate) extern "C" fn rt_map_insert(m: *mut RtMap, k: i64, v: i64) {
    if m.is_null() { return; }
    unsafe {
        let idx = rt_map_index(m, k);
        let m = &mut *m;
        if idx >= 0 {
            *m.vals.add(idx as usize) = v;
            return;
        }
        if m.len >= m.cap {
            m.cap *= 2;
            let mut kv = Vec::from_raw_parts(m.keys, m.len as usize, m.len as usize);
            kv.reserve((m.cap - m.len) as usize);
            m.keys = kv.as_mut_ptr();
            std::mem::forget(kv);
            let mut vv = Vec::from_raw_parts(m.vals, m.len as usize, m.len as usize);
            vv.reserve((m.cap - m.len) as usize);
            m.vals = vv.as_mut_ptr();
            std::mem::forget(vv);
        }
        *m.keys.add(m.len as usize) = k;
        *m.vals.add(m.len as usize) = v;
        m.len += 1;
    }
}

pub(crate) extern "C" fn rt_map_get(m: *mut RtMap, k: i64) -> i64 {
    if m.is_null() { return 0; }
    unsafe {
        let i = rt_map_index(m, k);
        if i >= 0 { *(*m).vals.add(i as usize) } else { 0 }
    }
}

pub(crate) extern "C" fn rt_map_has(m: *mut RtMap, k: i64) -> i64 {
    if m.is_null() { return 0; }
    unsafe { if rt_map_index(m, k) >= 0 { 1 } else { 0 } }
}

pub(crate) extern "C" fn rt_map_remove(m: *mut RtMap, k: i64) {
    if m.is_null() { return; }
    unsafe {
        let i = rt_map_index(m, k);
        if i < 0 { return; }
        let m = &mut *m;
        let mut j = i;
        while j + 1 < m.len {
            *m.keys.add(j as usize) = *m.keys.add((j + 1) as usize);
            *m.vals.add(j as usize) = *m.vals.add((j + 1) as usize);
            j += 1;
        }
        m.len -= 1;
    }
}

pub(crate) extern "C" fn rt_map_len(m: *mut RtMap) -> i64 {
    if m.is_null() { 0 } else { unsafe { (*m).len } }
}

pub(crate) extern "C" fn rt_map_keys(m: *mut RtMap) -> *mut RtList {
    let out = rt_list_new();
    if m.is_null() { return out; }
    unsafe {
        let m = &*m;
        for i in 0..m.len {
            rt_list_push(out, *m.keys.add(i as usize));
        }
    }
    out
}

pub(crate) extern "C" fn rt_map_values(m: *mut RtMap) -> *mut RtList {
    let out = rt_list_new();
    if m.is_null() { return out; }
    unsafe {
        let m = &*m;
        for i in 0..m.len {
            rt_list_push(out, *m.vals.add(i as usize));
        }
    }
    out
}


// ============================================================
// 字符串内置（Rust 版，与 gt_rt.c 的 gt_str_* 语义一致）
// 返回的 CString 放进环形池，保证指针短期有效（与 sb_finish 同策略）。
// ============================================================

/// 把一段字节放进字符串结果池，返回 NUL 结尾指针
pub(crate) fn str_pool(bytes: Vec<u8>) -> i64 {
    let c = std::ffi::CString::new(bytes).unwrap_or_default();
    SB_POOL.with(|pool| {
        let mut p = pool.borrow_mut();
        if p.len() >= 64 {
            p.pop_front();
        }
        p.push_back(c);
        p.back().unwrap().as_ptr() as i64
    })
}

pub(crate) unsafe fn cstr_bytes<'a>(p: i64) -> &'a [u8] {
    if p == 0 { return b""; }
    std::ffi::CStr::from_ptr(p as *const i8).to_bytes()
}

pub(crate) extern "C" fn rt_str_substr(s: i64, start: i64, len: i64) -> i64 {
    let b = unsafe { cstr_bytes(s) };
    let n = b.len() as i64;
    let start = start.max(0).min(n);
    let len = len.max(0).min(n - start);
    str_pool(b[start as usize..(start + len) as usize].to_vec())
}

pub(crate) extern "C" fn rt_str_find(s: i64, sub: i64) -> i64 {
    let hs = unsafe { cstr_bytes(s) };
    let nd = unsafe { cstr_bytes(sub) };
    if nd.is_empty() { return 0; }
    if nd.len() > hs.len() { return -1; }
    for i in 0..=(hs.len() - nd.len()) {
        if &hs[i..i + nd.len()] == nd { return i as i64; }
    }
    -1
}

pub(crate) extern "C" fn rt_str_upper(s: i64) -> i64 {
    let b = unsafe { cstr_bytes(s) };
    str_pool(b.iter().map(|c| c.to_ascii_uppercase()).collect())
}

pub(crate) extern "C" fn rt_str_lower(s: i64) -> i64 {
    let b = unsafe { cstr_bytes(s) };
    str_pool(b.iter().map(|c| c.to_ascii_lowercase()).collect())
}

pub(crate) extern "C" fn rt_str_trim(s: i64) -> i64 {
    let b = unsafe { cstr_bytes(s) };
    let is_ws = |c: u8| c == b' ' || c == b'\t' || c == b'\n' || c == b'\r';
    let mut a = 0;
    let mut z = b.len();
    while a < z && is_ws(b[a]) { a += 1; }
    while z > a && is_ws(b[z - 1]) { z -= 1; }
    str_pool(b[a..z].to_vec())
}

pub(crate) extern "C" fn rt_str_repeat(s: i64, n: i64) -> i64 {
    let b = unsafe { cstr_bytes(s) };
    if n <= 0 { return str_pool(Vec::new()); }
    let mut out = Vec::with_capacity(b.len() * n as usize);
    for _ in 0..n { out.extend_from_slice(b); }
    str_pool(out)
}

pub(crate) extern "C" fn rt_str_replace(s: i64, from: i64, to: i64) -> i64 {
    let hs = unsafe { cstr_bytes(s) };
    let f = unsafe { cstr_bytes(from) };
    let t = unsafe { cstr_bytes(to) };
    if f.is_empty() { return str_pool(hs.to_vec()); }
    let mut out = Vec::new();
    let mut i = 0;
    while i < hs.len() {
        if i + f.len() <= hs.len() && &hs[i..i + f.len()] == f {
            out.extend_from_slice(t);
            i += f.len();
        } else {
            out.push(hs[i]);
            i += 1;
        }
    }
    str_pool(out)
}

pub(crate) extern "C" fn rt_str_split(s: i64, sep: i64) -> *mut RtList {
    let out = rt_list_new();
    let hs = unsafe { cstr_bytes(s) };
    let sp = unsafe { cstr_bytes(sep) };
    if sp.is_empty() {
        rt_list_push(out, str_pool(hs.to_vec()));
        return out;
    }
    let mut start = 0;
    let mut i = 0;
    while i + sp.len() <= hs.len() {
        if &hs[i..i + sp.len()] == sp {
            rt_list_push(out, str_pool(hs[start..i].to_vec()));
            i += sp.len();
            start = i;
        } else {
            i += 1;
        }
    }
    rt_list_push(out, str_pool(hs[start..].to_vec()));
    out
}

pub(crate) extern "C" fn rt_str_join(l: *mut RtList, sep: i64) -> i64 {
    let sp = unsafe { cstr_bytes(sep) };
    let mut out = Vec::new();
    if !l.is_null() {
        unsafe {
            let l = &*l;
            for i in 0..l.len {
                if i > 0 { out.extend_from_slice(sp); }
                let e = *l.data.add(i as usize);
                out.extend_from_slice(cstr_bytes(e));
            }
        }
    }
    str_pool(out)
}


// ---------- 数值内置（Rust 版，与 gt_rt.c 一致） ----------
pub(crate) extern "C" fn rt_abs_i(v: i64) -> i64 { v.abs() }
pub(crate) extern "C" fn rt_abs_f(v: f64) -> f64 { v.abs() }
pub(crate) extern "C" fn rt_min_i(a: i64, b: i64) -> i64 { a.min(b) }
pub(crate) extern "C" fn rt_max_i(a: i64, b: i64) -> i64 { a.max(b) }
pub(crate) extern "C" fn rt_min_f(a: f64, b: f64) -> f64 { a.min(b) }
pub(crate) extern "C" fn rt_max_f(a: f64, b: f64) -> f64 { a.max(b) }

/// sum(list<i64>)：遍历 RtList
pub(crate) extern "C" fn rt_sum_i(l: *mut RtList) -> i64 {
    if l.is_null() { return 0; }
    unsafe {
        let l = &*l;
        let mut s = 0i64;
        for i in 0..l.len { s = s.wrapping_add(*l.data.add(i as usize)); }
        s
    }
}
/// sum(list<f64>)：元素是 f64 位模式
pub(crate) extern "C" fn rt_sum_f(l: *mut RtList) -> f64 {
    if l.is_null() { return 0.0; }
    unsafe {
        let l = &*l;
        let mut s = 0.0f64;
        for i in 0..l.len {
            let bits = *l.data.add(i as usize) as u64;
            s += f64::from_bits(bits);
        }
        s
    }
}


// ---------- 裸内存操作（Rust 版，与 gt_rt.c 一致） ----------
// 指针是 i64 字节地址；用全局分配器，需记录块大小以便释放。
use std::alloc::{alloc, Layout};

pub(crate) extern "C" fn rt_mem_alloc(n: i64) -> i64 {
    let n = if n <= 0 { 1 } else { n as usize };
    unsafe {
        let layout = Layout::from_size_align(n, 8).unwrap();
        let p = alloc(layout);
        p as i64
    }
}

// ---------- Result[T,E] 运行时（堆块 [tag, payload]，tag=0 Ok / 1 Err） ----------
pub(crate) extern "C" fn rt_result_new(tag: i64, payload: i64) -> i64 {
    let p = rt_mem_alloc(16);
    if p != 0 {
        unsafe {
            let q = p as *mut i64;
            *q = tag;
            *q.add(1) = payload;
        }
    }
    p
}
pub(crate) extern "C" fn rt_result_tag(p: i64) -> i64 {
    if p == 0 { return 0; }
    unsafe { *(p as *const i64) }
}
pub(crate) extern "C" fn rt_result_val(p: i64) -> i64 {
    if p == 0 { return 0; }
    unsafe { *((p as *const i64).add(1)) }
}

pub(crate) extern "C" fn rt_mem_free(p: i64) {
    // 无法得知大小，简单实现：仅当 p 非空时什么都不做（避免 UB）；
    // 真实释放需记录大小，这里对齐 C 语义用 libc free 更稳妥。
    let _ = p;
}

pub(crate) extern "C" fn rt_mem_store_i64(p: i64, off: i64, v: i64) {
    if p == 0 { return; }
    unsafe { *((p + off) as *mut i64) = v; }
}

pub(crate) extern "C" fn rt_mem_load_i64(p: i64, off: i64) -> i64 {
    if p == 0 { return 0; }
    unsafe { *((p + off) as *const i64) }
}

pub(crate) extern "C" fn rt_mem_store_u8(p: i64, off: i64, v: i64) {
    if p == 0 { return; }
    unsafe { *((p + off) as *mut u8) = v as u8; }
}

pub(crate) extern "C" fn rt_mem_load_u8(p: i64, off: i64) -> i64 {
    if p == 0 { return 0; }
    unsafe { *((p + off) as *const u8) as i64 }
}

pub(crate) extern "C" fn rt_mem_copy(dst: i64, src: i64, n: i64) {
    if dst == 0 || src == 0 || n <= 0 { return; }
    unsafe { std::ptr::copy_nonoverlapping(src as *const u8, dst as *mut u8, n as usize); }
}

pub(crate) extern "C" fn rt_mem_set(p: i64, byte: i64, n: i64) {
    if p == 0 || n <= 0 { return; }
    unsafe { std::ptr::write_bytes(p as *mut u8, byte as u8, n as usize); }
}



pub(crate) extern "C" fn rt_range(a: i64, b: i64) -> i64 {
    let out = rt_list_new();
    let mut i = a;
    while i < b {
        rt_list_push(out, i);
        i += 1;
    }
    out as i64
}


pub(crate) extern "C" fn rt_assert(cond: i64, msg: i64, line: i64) {
    if cond != 0 { return; }
    let m = if msg == 0 { String::new() } else { unsafe { std::ffi::CStr::from_ptr(msg as *const i8).to_string_lossy().into_owned() } };
    let text = if crate::lang::is_zh() {
        format!("\n[断言失败] 第 {} 行：{}\n", line, if m.is_empty() { "断言条件为假".into() } else { m })
    } else {
        format!("\n[assertion failed] line {}: {}\n", line, if m.is_empty() { "assertion failed".into() } else { m })
    };
    let _ = std::io::Write::write_all(&mut std::io::stderr(), text.as_bytes());
    std::process::exit(1);
}


pub(crate) extern "C" fn rt_pad_left(s: i64, width: i64, fill: i64) -> i64 {
    let src = unsafe { std::ffi::CStr::from_ptr(s as *const i8) }.to_string_lossy().into_owned();
    let f = if fill == 0 { " ".to_string() } else { unsafe { std::ffi::CStr::from_ptr(fill as *const i8) }.to_string_lossy().into_owned() };
    let n = src.chars().count() as i64;
    if n >= width { return s; }
    let f = if f.is_empty() { " ".to_string() } else { f };
    let mut pad = String::new();
    let need = width - n;
    while (pad.chars().count() as i64) < need {
        for c in f.chars() {
            if (pad.chars().count() as i64) < need { pad.push(c); }
        }
    }
    str_pool((pad + &src).into_bytes())
}

pub(crate) extern "C" fn rt_pad_right(s: i64, width: i64, fill: i64) -> i64 {
    let src = unsafe { std::ffi::CStr::from_ptr(s as *const i8) }.to_string_lossy().into_owned();
    let f = if fill == 0 { " ".to_string() } else { unsafe { std::ffi::CStr::from_ptr(fill as *const i8) }.to_string_lossy().into_owned() };
    let n = src.chars().count() as i64;
    if n >= width { return s; }
    let f = if f.is_empty() { " ".to_string() } else { f };
    let mut out = src.clone();
    let need = width - n;
    let mut got = 0i64;
    while got < need {
        for c in f.chars() {
            if got < need { out.push(c); got += 1; }
        }
    }
    str_pool(out.into_bytes())
}

pub(crate) extern "C" fn rt_fmt_int(x: i64, width: i64) -> i64 {
    let s = x.to_string();
    if (s.len() as i64) >= width { return str_pool(s.into_bytes()); }
    let mut out = " ".repeat((width - s.len() as i64) as usize);
    out.push_str(&s);
    str_pool(out.into_bytes())
}


pub(crate) extern "C" fn rt_thread_spawn(fn_ptr: i64, args: i64, n: i64) {
    unsafe {
        let f: extern "C" fn(i64, i64, i64, i64) -> i64 = std::mem::transmute(fn_ptr as usize);
        let mut vals = [0i64; 4];
        for i in 0..(n as usize).min(4) {
            vals[i] = *((args as *const i64).add(i));
        }
        std::thread::spawn(move || {
            f(vals[0], vals[1], vals[2], vals[3]);
        });
    }
}


pub(crate) extern "C" fn rt_sleep(ms: i64) {
    std::thread::sleep(std::time::Duration::from_millis(ms.max(0) as u64));
}


use std::sync::{Mutex, Condvar};
struct RtChan { mu: Mutex<Vec<i64>>, cv: Condvar } // 无界通道
pub(crate) extern "C" fn rt_chan_new() -> i64 {
    let c = Box::new(RtChan { mu: Mutex::new(Vec::new()), cv: Condvar::new() });
    Box::into_raw(c) as i64
}
pub(crate) extern "C" fn rt_chan_send(p: i64, v: i64) {
    let c = unsafe { &*(p as *const RtChan) };
    let mut q = c.mu.lock().unwrap();
    q.push(v);
    c.cv.notify_one();
}
pub(crate) extern "C" fn rt_chan_recv(p: i64) -> i64 {
    let c = unsafe { &*(p as *const RtChan) };
    let mut q = c.mu.lock().unwrap();
    while q.is_empty() { q = c.cv.wait(q).unwrap(); }
    q.remove(0)
}


pub(crate) extern "C" fn rt_rt_init() {
    // Windows：设控制台输出为 UTF-8（让内联 C 的 printf 正确显示）
    #[cfg(windows)]
    unsafe {
        extern "system" {
            fn SetConsoleOutputCP(cp: u32) -> i32;
        }
        SetConsoleOutputCP(65001);
    }
}
#[allow(dead_code)]
pub(crate) extern "C" fn rt_rc_inc(p: i64) {
    if p != 0 { unsafe { let rc = p as *mut i64; *rc += 1; } }
}
#[allow(dead_code)]
#[allow(dead_code)]
pub(crate) extern "C" fn rt_rc_dec(p: i64, free_fn: i64) -> i64 {
    if p == 0 { return 0; }
    unsafe {
        let rc = p as *mut i64;
        *rc -= 1;
        if *rc == 0 {
            if free_fn != 0 {
                let f: extern "C" fn(i64) = std::mem::transmute(free_fn as usize);
                f(p);
            }
            return 0;
        }
        *rc
    }
}
