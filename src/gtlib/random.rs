//! GT 标准库 —— random 模块（编译为 random.dll + random.lib）
//! Python 风格：seed / random / randint / randrange / uniform / choice / shuffle / sample / gauss

#![allow(clippy::missing_safety_doc)]

use std::cell::Cell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

thread_local! {
    static RNG_STATE: Cell<u64> = Cell::new(0x9E3779B97F4A7C15);
}
fn next_u64() -> u64 {
    RNG_STATE.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        x
    })
}
fn next_f64() -> f64 {
    (next_u64() >> 11) as f64 / (1u64 << 53) as f64
}
fn ret_string(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()).into_raw()
}

/// random.seed(a)
#[no_mangle]
pub extern "C" fn py_seed(a: i64) {
    let v = if a == 0 { 0x9E3779B97F4A7C15 } else { a as u64 };
    RNG_STATE.with(|s| s.set(v));
}

/// random.random() -> f64 in [0.0, 1.0)
#[no_mangle]
pub extern "C" fn py_random() -> f64 { next_f64() }

/// random.randint(a, b) -> i64 in [a, b]
#[no_mangle]
pub extern "C" fn py_randint(a: i64, b: i64) -> i64 {
    if b < a { return a; }
    let span = (b - a + 1) as u64;
    a + (next_u64() % span) as i64
}

/// random.randrange(stop) / randrange(start, stop) —— 用两个参数模拟
#[no_mangle]
pub extern "C" fn py_randrange(start: i64, stop: i64) -> i64 {
    if stop <= start { return start; }
    start + (next_u64() % ((stop - start) as u64)) as i64
}

/// random.uniform(a, b) -> f64
#[no_mangle]
pub extern "C" fn py_uniform(a: f64, b: f64) -> f64 {
    a + (b - a) * next_f64()
}

/// random.choice(s) -> str：从 s（"\n" 分隔）中随机取一项
#[no_mangle]
pub extern "C" fn py_choice(s: *const c_char) -> *mut c_char {
    let v = unsafe { CStr::from_ptr(s).to_string_lossy().into_owned() };
    let items: Vec<&str> = v.lines().filter(|l| !l.is_empty()).collect();
    if items.is_empty() { return ret_string(String::new()); }
    let idx = (next_u64() as usize) % items.len();
    ret_string(items[idx].to_string())
}

/// random.shuffle(s) -> str：s（"\n" 分隔）洗牌后返回
#[no_mangle]
pub extern "C" fn py_shuffle(s: *const c_char) -> *mut c_char {
    let v = unsafe { CStr::from_ptr(s).to_string_lossy().into_owned() };
    let mut items: Vec<String> = v.lines().filter(|l| !l.is_empty()).map(|l| l.to_string()).collect();
    let n = items.len();
    if n > 1 {
        for i in (1..n).rev() {
            let j = (next_u64() as usize) % (i + 1);
            items.swap(i, j);
        }
    }
    ret_string(items.join("\n"))
}

/// random.sample(s, k) -> str：从 s（"\n" 分隔）不放回抽 k 个
#[no_mangle]
pub extern "C" fn py_sample(s: *const c_char, k: i64) -> *mut c_char {
    let v = unsafe { CStr::from_ptr(s).to_string_lossy().into_owned() };
    let mut items: Vec<String> = v.lines().filter(|l| !l.is_empty()).map(|l| l.to_string()).collect();
    let n = items.len();
    let k = k.max(0).min(n as i64) as usize;
    // partial Fisher-Yates
    for i in 0..k {
        let j = i + (next_u64() as usize) % (n - i);
        items.swap(i, j);
    }
    items.truncate(k);
    ret_string(items.join("\n"))
}

/// random.gauss(mu, sigma) -> f64（Box-Muller）
#[no_mangle]
pub extern "C" fn py_gauss(mu: f64, sigma: f64) -> f64 {
    let u1 = next_f64().max(1e-12);
    let u2 = next_f64();
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    mu + sigma * z
}
