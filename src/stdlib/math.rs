//! GT 标准库 —— math 模块（编译为 math.dll + math.lib）

#![allow(clippy::missing_safety_doc)]

#[no_mangle]
pub extern "C" fn py_sqrt(x: f64) -> f64 { x.sqrt() }
#[no_mangle]
pub extern "C" fn py_pow(x: f64, y: f64) -> f64 { x.powf(y) }
#[no_mangle]
pub extern "C" fn py_floor(x: f64) -> f64 { x.floor() }
#[no_mangle]
pub extern "C" fn py_ceil(x: f64) -> f64 { x.ceil() }
#[no_mangle]
pub extern "C" fn py_sin(x: f64) -> f64 { x.sin() }
#[no_mangle]
pub extern "C" fn py_cos(x: f64) -> f64 { x.cos() }
#[no_mangle]
pub extern "C" fn py_tan(x: f64) -> f64 { x.tan() }
#[no_mangle]
pub extern "C" fn py_asin(x: f64) -> f64 { x.asin() }
#[no_mangle]
pub extern "C" fn py_acos(x: f64) -> f64 { x.acos() }
#[no_mangle]
pub extern "C" fn py_atan(x: f64) -> f64 { x.atan() }
#[no_mangle]
pub extern "C" fn py_atan2(y: f64, x: f64) -> f64 { y.atan2(x) }
#[no_mangle]
pub extern "C" fn py_exp(x: f64) -> f64 { x.exp() }
#[no_mangle]
pub extern "C" fn py_log(x: f64) -> f64 { x.ln() }
#[no_mangle]
pub extern "C" fn py_log2(x: f64) -> f64 { x.log2() }
#[no_mangle]
pub extern "C" fn py_log10(x: f64) -> f64 { x.log10() }
#[no_mangle]
pub extern "C" fn py_fmod(x: f64, y: f64) -> f64 { x % y }
#[no_mangle]
pub extern "C" fn py_hypot(x: f64, y: f64) -> f64 { (x*x + y*y).sqrt() }
#[no_mangle]
pub extern "C" fn py_cbrt(x: f64) -> f64 { x.cbrt() }
#[no_mangle]
pub extern "C" fn py_fabs(x: f64) -> f64 { x.abs() }

#[no_mangle]
pub extern "C" fn py_round(x: f64) -> i64 {
    if x >= 0.0 { (x + 0.5).floor() as i64 } else { (x - 0.5).ceil() as i64 }
}

#[no_mangle]
pub extern "C" fn py_gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 { let t = b; b = a % b; a = t; }
    a
}

#[no_mangle]
pub extern "C" fn py_lcm(a: i64, b: i64) -> i64 {
    if a == 0 || b == 0 { return 0; }
    (a / py_gcd(a, b) * b).abs()
}

#[no_mangle]
pub extern "C" fn py_pi() -> f64 { std::f64::consts::PI }
#[no_mangle]
pub extern "C" fn py_e() -> f64 { std::f64::consts::E }

#[no_mangle]
pub extern "C" fn py_ipow(base: i64, exp: i64) -> i64 {
    if exp < 0 { return 0; }
    let mut r: i64 = 1;
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if e & 1 == 1 { r = r.wrapping_mul(b); }
        b = b.wrapping_mul(b);
        e >>= 1;
    }
    r
}

#[no_mangle]
pub extern "C" fn py_isqrt(n: i64) -> i64 {
    if n < 0 { return -1; }
    if n < 2 { return n; }
    let mut x = (n as f64).sqrt() as i64;
    while x * x > n { x -= 1; }
    while (x + 1) * (x + 1) <= n { x += 1; }
    x
}

#[no_mangle]
pub extern "C" fn py_powmod(base: i64, exp: i64, m: i64) -> i64 {
    if m == 0 { return 0; }
    let mut result: i64 = 1;
    let mut b = base % m;
    let mut e = exp;
    if e < 0 { return 0; }
    while e > 0 {
        if e & 1 == 1 { result = (result * b) % m; }
        b = (b * b) % m;
        e >>= 1;
    }
    result
}

#[no_mangle]
pub extern "C" fn py_factorial(n: i64) -> i64 {
    if n < 0 { return 0; }
    let mut r: i64 = 1;
    let mut i: i64 = 2;
    while i <= n { r = r.wrapping_mul(i); i += 1; }
    r
}

#[no_mangle]
pub extern "C" fn py_fib(n: i64) -> i64 {
    if n < 0 { return 0; }
    let (mut a, mut b) = (0i64, 1i64);
    let mut i = 0;
    while i < n { let t = a + b; a = b; b = t; i += 1; }
    a
}

#[no_mangle]
pub extern "C" fn py_isprime(n: i64) -> i64 {
    if n < 2 { return 0; }
    if n < 4 { return 1; }
    if n % 2 == 0 { return 0; }
    let mut i: i64 = 3;
    while i * i <= n {
        if n % i == 0 { return 0; }
        i += 2;
    }
    1
}

#[no_mangle]
pub extern "C" fn py_comb(n: i64, k: i64) -> i64 {
    if k < 0 || k > n { return 0; }
    let k = k.min(n - k);
    let mut r: i64 = 1;
    let mut i: i64 = 0;
    while i < k { r = r * (n - i) / (i + 1); i += 1; }
    r
}

// ---------- random ----------
use std::cell::Cell;
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
fn seed(v: u64) { RNG_STATE.with(|s| s.set(if v == 0 { 0x9E3779B97F4A7C15 } else { v })); }

/// math.seed(n)
#[no_mangle]
pub extern "C" fn py_seed(n: i64) { seed(n as u64); }

/// math.random() -> f64 in [0, 1)
#[no_mangle]
pub extern "C" fn py_random() -> f64 {
    (next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

/// math.randint(lo, hi) -> i64 in [lo, hi]
#[no_mangle]
pub extern "C" fn py_randint(lo: i64, hi: i64) -> i64 {
    if hi < lo { return lo; }
    let span = (hi - lo + 1) as u64;
    lo + (next_u64() % span) as i64
}

/// math.uniform(lo, hi) -> f64
#[no_mangle]
pub extern "C" fn py_uniform(lo: f64, hi: f64) -> f64 {
    lo + (hi - lo) * ((next_u64() >> 11) as f64 / (1u64 << 53) as f64)
}

/// math.choice(s) -> str：从 s（"\n" 分隔）中随机取一项
#[no_mangle]
pub extern "C" fn py_choice(s: *const std::os::raw::c_char) -> *mut std::os::raw::c_char {
    let v = unsafe { std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned() };
    let items: Vec<&str> = v.lines().filter(|l| !l.is_empty()).collect();
    if items.is_empty() {
        let c = std::ffi::CString::new("").unwrap();
        return c.into_raw();
    }
    let idx = (next_u64() as usize) % items.len();
    std::ffi::CString::new(items[idx]).unwrap_or_else(|_| std::ffi::CString::new("").unwrap()).into_raw()
}
