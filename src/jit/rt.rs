//! JIT 运行时：rt_* 函数（C ABI）+ 容器/字符串/内存辅助。

use super::*;

// ============================================================
// 供 JIT 代码调用的运行时（Rust 实现，直接注册为符号）
// ============================================================


// ============================================================
// 宿主机 boot 模拟后端（boot_* 符号；裸机由 rt_bare.c 提供）
// ============================================================

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

static BOOT_T0: Mutex<Option<Instant>> = Mutex::new(None);
static BOOT_MEM: AtomicUsize = AtomicUsize::new(0);
static BOOT_HEAP: Mutex<Option<Vec<u8>>> = Mutex::new(None);

fn boot_now_ms() -> u64 {
    let mut g = BOOT_T0.lock().unwrap();
    if g.is_none() { *g = Some(Instant::now()); }
    g.as_ref().unwrap().elapsed().as_millis() as u64
}

// ---- serial（stdout）----
pub(crate) extern "C" fn boot_serial_init() {}
pub(crate) extern "C" fn boot_serial_putc(c: i64) {
    let b = [c as u8];
    let _ = std::io::Write::write_all(&mut std::io::stdout(), &b);
    let _ = std::io::Write::flush(&mut std::io::stdout());
}
pub(crate) extern "C" fn boot_serial_puts(s: i64) {
    if s == 0 { return; }
    let cs = unsafe { std::ffi::CStr::from_ptr(s as *const i8) };
    let _ = std::io::Write::write_all(&mut std::io::stdout(), cs.to_bytes());
    let _ = std::io::Write::flush(&mut std::io::stdout());
}
pub(crate) extern "C" fn boot_serial_getc() -> i64 {
    use std::io::Read;
    let mut b = [0u8; 1];
    match std::io::stdin().read(&mut b) { Ok(1) => b[0] as i64, _ => -1 }
}
pub(crate) extern "C" fn boot_serial_poll() -> i64 { -1 }

// ---- screen（ANSI 清屏；putc/puts 复用 serial）----
pub(crate) extern "C" fn boot_clear() { print!("\x1b[2J\x1b[H"); }
pub(crate) extern "C" fn boot_putc_at(c: i64, x: i64, y: i64) {
    print!("\x1b[{};{}H{}", y + 1, x + 1, c as u8 as char);
}
pub(crate) extern "C" fn boot_puts(s: i64) { boot_serial_puts(s); }
pub(crate) extern "C" fn boot_vga_clear() { boot_clear(); }
pub(crate) extern "C" fn boot_vga_set_color(_fg: i64, _bg: i64) {}
pub(crate) extern "C" fn boot_vga_putc(c: i64) { boot_serial_putc(c); }
pub(crate) extern "C" fn boot_vga_puts(s: i64) { boot_serial_puts(s); }

// ---- keyboard（stdin 行缓冲）----
static BOOT_KBD: Mutex<Vec<u8>> = Mutex::new(Vec::new());
pub(crate) extern "C" fn boot_getkey() -> i64 { boot_serial_getc() }
pub(crate) extern "C" fn boot_keyboard_handler() -> i64 {
    let mut b = BOOT_KBD.lock().unwrap();
    if b.is_empty() { -1 } else { b.remove(0) as i64 }
}
pub(crate) extern "C" fn boot_keyboard_modifiers() -> i64 { 0 }

// ---- memory（Vec 堆）----
pub(crate) extern "C" fn boot_mem_alloc(n: i64) -> i64 {
    if n <= 0 { return 0; }
    let mut g = BOOT_HEAP.lock().unwrap();
    if g.is_none() { *g = Some(vec![0u8; 16 * 1024 * 1024]); }
    let base = g.as_ref().unwrap().as_ptr() as i64;
    let off = BOOT_MEM.fetch_add(n as usize, Ordering::SeqCst);
    if off + (n as usize) > 16 * 1024 * 1024 { return 0; }   // 越界保护
    base + off as i64
}
pub(crate) extern "C" fn boot_mem_free(_p: i64) {}
pub(crate) extern "C" fn boot_mem_size() -> i64 { 16 * 1024 * 1024 }
pub(crate) extern "C" fn boot_mem_free_bytes() -> i64 {
    (16 * 1024 * 1024) - BOOT_MEM.load(Ordering::SeqCst) as i64
}
pub(crate) extern "C" fn boot_paging_init(_p: i64) -> i64 { 0 }

// ---- time ----
pub(crate) extern "C" fn boot_time_ms() -> i64 { boot_now_ms() as i64 }
pub(crate) extern "C" fn boot_sleep_ms(ms: i64) {
    if ms > 0 { std::thread::sleep(std::time::Duration::from_millis(ms as u64)); }
}
pub(crate) extern "C" fn boot_rtc_read(out: i64) -> i64 {
    if out != 0 {
        let p = out as *mut u8;
        unsafe { for i in 0..6 { *p.add(i) = 0; } }
    }
    0
}

// ---- system ----
pub(crate) extern "C" fn boot_hlt() { std::process::exit(0); }
pub(crate) extern "C" fn boot_exit() { std::process::exit(0); }
pub(crate) extern "C" fn boot_reboot() { std::process::exit(0); }
pub(crate) extern "C" fn boot_shutdown() { std::process::exit(0); }
pub(crate) extern "C" fn boot_cpuid(_leaf: i64, out: i64) {
    if out != 0 {
        let p = out as *mut u32;
        unsafe { for i in 0..4 { *p.add(i) = 0; } }
    }
}
pub(crate) extern "C" fn boot_cpu_vendor(out: i64) -> i64 {
    if out != 0 {
        let p = out as *mut u8;
        let v = b"GTLangHost  ";
        unsafe { for i in 0..12 { *p.add(i) = v[i]; } }
    }
    0
}

// ---- disk（宿主机为空实现）----
pub(crate) extern "C" fn boot_disk_read(_lba: i64, _n: i64, _buf: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_disk_write(_lba: i64, _n: i64, _buf: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_disk_partitions(out: i64) -> i64 {
    if out != 0 {
        let p = out as *mut u32;
        unsafe { for i in 0..8 { *p.add(i) = 0; } }
    }
    0
}

// ---- port（宿主机为空实现）----
pub(crate) extern "C" fn boot_inb(_p: i64) -> i64 { 0 }
pub(crate) extern "C" fn boot_outb(_p: i64, _v: i64) {}
pub(crate) extern "C" fn boot_inw(_p: i64) -> i64 { 0 }
pub(crate) extern "C" fn boot_outw(_p: i64, _v: i64) {}

// ---- interrupt（宿主机为空实现）----
pub(crate) extern "C" fn boot_idt_init() {}
pub(crate) extern "C" fn boot_irq_enable() {}
pub(crate) extern "C" fn boot_irq_disable() {}
pub(crate) extern "C" fn boot_pic_init() {}
pub(crate) extern "C" fn boot_irq_register(_irq: i64, _fn: i64) -> i64 { 0 }

// ---- task（宿主机：立即执行）----
pub(crate) extern "C" fn boot_task_create(_entry: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_task_yield() {}
pub(crate) extern "C" fn boot_task_start() {}
pub(crate) extern "C" fn boot_task_exit() {}



// ---- str_builder（JIT：全局表 id -> String）----
use std::sync::atomic::AtomicI64;
static SB_NEXT: AtomicI64 = AtomicI64::new(1);
static SB_TABLE: Mutex<Vec<(i64, String)>> = Mutex::new(Vec::new());

pub(crate) extern "C" fn rt_str_builder() -> i64 {
    let id = SB_NEXT.fetch_add(1, Ordering::SeqCst);
    SB_TABLE.lock().unwrap().push((id, String::new()));
    id
}
pub(crate) extern "C" fn rt_sb_append(id: i64, s: i64) {
    if s == 0 { return; }
    let cs = unsafe { std::ffi::CStr::from_ptr(s as *const i8) };
    let mut t = SB_TABLE.lock().unwrap();
    if let Some(e) = t.iter_mut().find(|(i, _)| *i == id) { e.1.push_str(&cs.to_string_lossy()); }
}
pub(crate) extern "C" fn rt_sb_append_int(id: i64, v: i64) {
    let mut t = SB_TABLE.lock().unwrap();
    if let Some(e) = t.iter_mut().find(|(i, _)| *i == id) { e.1.push_str(&v.to_string()); }
}
pub(crate) extern "C" fn rt_sb_finish2(id: i64) -> i64 {
    let mut t = SB_TABLE.lock().unwrap();
    let s = t.iter().find(|(i, _)| *i == id).map(|(_, s)| s.clone()).unwrap_or_default();
    t.retain(|(i, _)| *i != id);
    let c = std::ffi::CString::new(s).unwrap_or_default();
    c.into_raw() as i64
}
// ---- fs_ram（内存文件系统，HashMap<name, Vec<u8>>）----
static BOOT_FS: Mutex<Vec<(Vec<u8>, Vec<u8>)>> = Mutex::new(Vec::new());

fn boot_cstr(s: i64) -> Vec<u8> {
    if s == 0 { return Vec::new(); }
    let cs = unsafe { std::ffi::CStr::from_ptr(s as *const i8) };
    cs.to_bytes().to_vec()
}

pub(crate) extern "C" fn boot_fs_ram_create(name: i64) -> i64 {
    let n = boot_cstr(name);
    let mut fs = BOOT_FS.lock().unwrap();
    if fs.iter().any(|(k, _)| *k == n) { return -1; }
    fs.push((n, Vec::new()));
    (fs.len() - 1) as i64
}
pub(crate) extern "C" fn boot_fs_ram_write(name: i64, data: i64, len: i64) -> i64 {
    let n = boot_cstr(name);
    if len < 0 { return -1; }
    let bytes = if data == 0 || len == 0 { Vec::new() } else {
        unsafe { std::slice::from_raw_parts(data as *const u8, len as usize) }.to_vec()
    };
    let mut fs = BOOT_FS.lock().unwrap();
    match fs.iter_mut().find(|(k, _)| *k == n) {
        Some((_, v)) => { *v = bytes; len }
        None => { fs.push((n, bytes)); len }
    }
}
pub(crate) extern "C" fn boot_fs_ram_read(name: i64, out: i64, cap: i64) -> i64 {
    let n = boot_cstr(name);
    let fs = BOOT_FS.lock().unwrap();
    match fs.iter().find(|(k, _)| *k == n) {
        Some((_, v)) => {
            let m = std::cmp::min(v.len() as i64, cap) as usize;
            if out != 0 { unsafe { std::ptr::copy_nonoverlapping(v.as_ptr(), out as *mut u8, m); } }
            m as i64
        }
        None => -1,
    }
}
pub(crate) extern "C" fn boot_fs_ram_size(name: i64) -> i64 {
    let n = boot_cstr(name);
    let fs = BOOT_FS.lock().unwrap();
    fs.iter().find(|(k, _)| *k == n).map(|(_, v)| v.len() as i64).unwrap_or(-1)
}
pub(crate) extern "C" fn boot_fs_ram_delete(name: i64) -> i64 {
    let n = boot_cstr(name);
    let mut fs = BOOT_FS.lock().unwrap();
    let before = fs.len();
    fs.retain(|(k, _)| *k != n);
    if fs.len() < before { 0 } else { -1 }
}
pub(crate) extern "C" fn boot_fs_ram_count() -> i64 { BOOT_FS.lock().unwrap().len() as i64 }
pub(crate) extern "C" fn boot_fs_ram_list(out: i64, cap: i64) -> i64 {
    let fs = BOOT_FS.lock().unwrap();
    let mut used = 0usize;
    for (k, _) in fs.iter() {
        if out != 0 && used + k.len() + 1 <= cap as usize {
            unsafe {
                std::ptr::copy_nonoverlapping(k.as_ptr(), (out as *mut u8).add(used), k.len());
                *(out as *mut u8).add(used + k.len()) = 0;
            }
            used += k.len() + 1;
        }
    }
    fs.len() as i64
}

// ---- fs_fat8/fat16/fat32（宿主机为空实现）----
pub(crate) extern "C" fn boot_fat8_find(_n: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_fat8_read(_n: i64, _o: i64, _c: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_fat8_list(_o: i64, _c: i64) -> i64 { 0 }
pub(crate) extern "C" fn boot_fat8_write(_n: i64, _d: i64, _l: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_fat8_delete(_n: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_fat16_find(_n: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_fat16_read(_n: i64, _o: i64, _c: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_fat16_write(_n: i64, _d: i64, _l: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_fat16_delete(_n: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_fat16_list(_o: i64, _c: i64) -> i64 { 0 }
pub(crate) extern "C" fn boot_fat32_find(_n: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_fat32_read(_n: i64, _o: i64, _c: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_fat32_write(_n: i64, _d: i64, _l: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_fat32_delete(_n: i64) -> i64 { -1 }
pub(crate) extern "C" fn boot_fat32_list(_o: i64, _c: i64) -> i64 { 0 }
// ---- info ----
pub(crate) extern "C" fn boot_version() -> i64 {
    b"boot 0.0.1e (host)\0".as_ptr() as i64
}
pub(crate) extern "C" fn boot_arch() -> i64 { std::mem::size_of::<usize>() as i64 * 8 }

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

pub(crate) extern "C" fn rt_sb_push_char(h: i64, c: i64) {
    let ch = (c as u8) as char;
    sb_with(h, |buf| buf.push(ch));
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
    rc: i64,
    data: *mut i64,
    len: i64,
    cap: i64,
    elem_ptr: i64,
}

pub(crate) fn rt_list_new(elem_ptr: i64) -> *mut RtList {
    let mut v: Vec<i64> = Vec::with_capacity(4);
    let data = v.as_mut_ptr();
    std::mem::forget(v);
    Box::into_raw(Box::new(RtList { rc: 1, data, len: 0, cap: 4, elem_ptr }))
}

pub(crate) unsafe fn rt_list_grow(l: *mut RtList) {
    let l = &mut *l;
    if l.len >= l.cap {
        let new_cap = if l.cap < 4 { 4 } else { l.cap * 2 };
        // 以真实容量 cap 恢复 Vec，避免 from_raw_parts 容量与实际分配不一致
        let mut v = Vec::from_raw_parts(l.data, l.len as usize, l.cap as usize);
        v.reserve((new_cap - l.len) as usize);
        l.cap = v.capacity() as i64;
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

pub(crate) extern "C" fn rt_list_slice(l: *mut RtList, lo: i64, hi: i64) -> *mut RtList {
    let elem_ptr = if l.is_null() { 0 } else { unsafe { (*l).elem_ptr } };
    let out = rt_list_new(elem_ptr);
    if l.is_null() { return out; }
    unsafe {
        let src = &*l;
        let lo = lo.max(0);
        let hi = hi.min(src.len);
        let mut i = lo;
        while i < hi { rt_list_push(out, *src.data.add(i as usize)); i += 1; }
    }
    out
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
    data: Vec<i64>,   // 插入顺序数组（for 遍历按此顺序）
    ht: Vec<i64>,     // 开放寻址哈希桶：存 data 下标，-1 空；长度 hcap 为 2 的幂
}

fn rt_set_rehash(s: &mut RtSet, newcap: usize) {
    let mut ht: Vec<i64> = vec![-1i64; newcap];
    let mask = (newcap - 1) as u64;
    for i in 0..s.data.len() {
        let v = s.data[i];
        let mut h = (hash64(v as u64) & mask) as usize;
        while ht[h] != -1 { h = ((h as u64 + 1) & mask) as usize; }
        ht[h] = i as i64;
    }
    s.ht = ht;
}

unsafe fn rt_set_find(s: *mut RtSet, v: i64) -> i64 {
    let s = &*s;
    if s.ht.is_empty() { return -1; }
    let mask = (s.ht.len() - 1) as u64;
    let mut h = (hash64(v as u64) & mask) as usize;
    loop {
        let idx = s.ht[h];
        if idx == -1 { return -1; }
        if s.data[idx as usize] == v { return idx; }
        h = ((h as u64 + 1) & mask) as usize;
    }
}

pub(crate) fn rt_set_new(_elem_ptr: i64) -> *mut RtSet {
    Box::into_raw(Box::new(RtSet { data: Vec::new(), ht: vec![-1i64; 8] }))
}

pub(crate) extern "C" fn rt_set_insert(s: *mut RtSet, v: i64) {
    if s.is_null() { return; }
    unsafe {
        let s = &mut *s;
        if rt_set_find(s, v) >= 0 { return; }
        if (s.data.len() + 1) * 10 >= s.ht.len() * 7 {
            let nc = s.ht.len() * 2;
            rt_set_rehash(s, nc);
        }
        let idx = s.data.len() as i64;
        s.data.push(v);
        let mask = (s.ht.len() - 1) as u64;
        let mut h = (hash64(v as u64) & mask) as usize;
        while s.ht[h] != -1 { h = ((h as u64 + 1) & mask) as usize; }
        s.ht[h] = idx;
    }
}

pub(crate) extern "C" fn rt_set_has(s: *mut RtSet, v: i64) -> i64 {
    if s.is_null() { return 0; }
    unsafe { if rt_set_find(s, v) >= 0 { 1 } else { 0 } }
}

pub(crate) extern "C" fn rt_set_remove(s: *mut RtSet, v: i64) {
    if s.is_null() { return; }
    unsafe {
        let s = &mut *s;
        let i = rt_set_find(s, v);
        if i < 0 { return; }
        s.data.remove(i as usize);
        let hcap = s.ht.len();
        rt_set_rehash(s, hcap);
    }
}

pub(crate) extern "C" fn rt_set_len(s: *mut RtSet) -> i64 {
    if s.is_null() { 0 } else { unsafe { (*s).data.len() as i64 } }
}

// ---------- map ----------
#[repr(C)]
pub(crate) struct RtMap {
    keys: Vec<i64>,   // 插入顺序数组（for 遍历按此顺序）
    vals: Vec<i64>,
    ht: Vec<i64>,     // 开放寻址哈希桶：存 keys 下标，-1 空；长度 hcap 为 2 的幂
}

fn hash64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^= x >> 33;
    x
}

pub(crate) fn rt_map_rehash(m: &mut RtMap, newcap: usize) {
    let mut ht: Vec<i64> = vec![-1i64; newcap];
    let mask = (newcap - 1) as u64;
    for i in 0..m.keys.len() {
        let k = m.keys[i];
        let mut h = (hash64(k as u64) & mask) as usize;
        while ht[h] != -1 { h = ((h as u64 + 1) & mask) as usize; }
        ht[h] = i as i64;
    }
    m.ht = ht;
}

pub(crate) unsafe fn rt_map_find(m: *mut RtMap, k: i64) -> i64 {
    let m = &*m;
    if m.ht.is_empty() { return -1; }
    let mask = (m.ht.len() - 1) as u64;
    let mut h = (hash64(k as u64) & mask) as usize;
    loop {
        let idx = m.ht[h];
        if idx == -1 { return -1; }
        if m.keys[idx as usize] == k { return idx; }
        h = ((h as u64 + 1) & mask) as usize;
    }
}

pub(crate) fn rt_map_new(_elem_ptr: i64) -> *mut RtMap {
    Box::into_raw(Box::new(RtMap { keys: Vec::new(), vals: Vec::new(), ht: vec![-1i64; 8] }))
}

pub(crate) unsafe fn rt_map_index(m: *mut RtMap, k: i64) -> i64 {
    rt_map_find(m, k)
}

pub(crate) extern "C" fn rt_map_insert(m: *mut RtMap, k: i64, v: i64) {
    if m.is_null() { return; }
    unsafe {
        let m = &mut *m;
        let found = rt_map_find(m, k);
        if found >= 0 {
            m.vals[found as usize] = v;
            return;
        }
        // 装载因子 > 0.7 → 扩容重建哈希
        if (m.keys.len() + 1) * 10 >= m.ht.len() * 7 {
            let newcap = m.ht.len() * 2;
            rt_map_rehash(m, newcap);
        }
        let idx = m.keys.len() as i64;
        m.keys.push(k);
        m.vals.push(v);
        let mask = (m.ht.len() - 1) as u64;
        let mut h = (hash64(k as u64) & mask) as usize;
        while m.ht[h] != -1 { h = ((h as u64 + 1) & mask) as usize; }
        m.ht[h] = idx;
    }
}

pub(crate) extern "C" fn rt_map_get(m: *mut RtMap, k: i64) -> i64 {
    if m.is_null() { return 0; }
    unsafe {
        let i = rt_map_find(m, k);
        let mm: &RtMap = &*m;
        if i >= 0 { mm.vals[i as usize] } else { 0 }
    }
}

pub(crate) extern "C" fn rt_map_has(m: *mut RtMap, k: i64) -> i64 {
    if m.is_null() { return 0; }
    unsafe { if rt_map_index(m, k) >= 0 { 1 } else { 0 } }
}

pub(crate) extern "C" fn rt_map_remove(m: *mut RtMap, k: i64) {
    if m.is_null() { return; }
    unsafe {
        let i = rt_map_find(m, k);
        if i < 0 { return; }
        let m = &mut *m;
        let j = i as usize;
        m.keys.remove(j);
        m.vals.remove(j);
        // 下标全变 → 重建哈希
        let hcap = m.ht.len();
        rt_map_rehash(m, hcap);
    }
}

pub(crate) extern "C" fn rt_map_len(m: *mut RtMap) -> i64 {
    if m.is_null() { 0 } else { unsafe { (*m).keys.len() as i64 } }
}

pub(crate) extern "C" fn rt_map_keys(m: *mut RtMap) -> *mut RtList {
    let out = rt_list_new(0);
    if m.is_null() { return out; }
    unsafe {
        let m = &*m;
        for i in 0..m.keys.len() {
            rt_list_push(out, m.keys[i]);
        }
    }
    out
}

pub(crate) extern "C" fn rt_map_values(m: *mut RtMap) -> *mut RtList {
    let out = rt_list_new(0);
    if m.is_null() { return out; }
    unsafe {
        let m = &*m;
        for i in 0..m.vals.len() {
            rt_list_push(out, m.vals[i]);
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
    let out = rt_list_new(0);
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
    let out = rt_list_new(0);
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


/// JIT 侧的 `go` 线程池（固定 N worker + FIFO 队列），避免每次 spawn 的开销。
/// 队列满时回退为"直接 spawn"，保证语义（不阻塞调用方）。
struct RtJob {
    f: extern "C" fn(i64, i64, i64, i64) -> i64,
    a: [i64; 4],
}
static RT_POOL: std::sync::OnceLock<std::sync::mpsc::Sender<RtJob>> = std::sync::OnceLock::new();

fn rt_pool_sender() -> &'static std::sync::mpsc::Sender<RtJob> {
    RT_POOL.get_or_init(|| {
        let n = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(4).clamp(4, 64);
        let (tx, rx) = std::sync::mpsc::channel::<RtJob>();
        let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
        for _ in 0..n {
            let rx = rx.clone();
            std::thread::spawn(move || loop {
                let job = { let g = rx.lock().unwrap(); g.recv() };
                match job { Ok(j) => { let _ = (j.f)(j.a[0], j.a[1], j.a[2], j.a[3]); }, Err(_) => break }
            });
        }
        tx
    })
}

pub(crate) extern "C" fn rt_thread_spawn(fn_ptr: i64, args: i64, n: i64) {
    unsafe {
        let f: extern "C" fn(i64, i64, i64, i64) -> i64 = std::mem::transmute(fn_ptr as usize);
        let mut vals = [0i64; 4];
        for i in 0..(n as usize).min(4) {
            vals[i] = *((args as *const i64).add(i));
        }
        // 通过共享接收端的线程池执行；失败（通道关闭）则直接 spawn
        if rt_pool_sender().send(RtJob { f, a: vals }).is_err() {
            std::thread::spawn(move || { f(vals[0], vals[1], vals[2], vals[3]); });
        }
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
        // 把 CRT 的 stdout 设为二进制模式：否则内联 C（libtcc 编译）的 printf
        // 在 Windows 文本模式下把 \n 翻成 \r\n，与 AOT 产物不一致。
        extern "C" {
            fn _setmode(fd: i32, mode: i32) -> i32;
        }
        const O_BINARY: i32 = 0x8000;
        const STDOUT_FD: i32 = 1;
        _setmode(STDOUT_FD, O_BINARY);
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

pub(crate) extern "C" fn rt_read_line() -> i64 {
    let mut s = String::new();
    let n = std::io::stdin().read_line(&mut s).unwrap_or(0);
    let _ = n;
    while s.ends_with('\n') || s.ends_with('\r') { s.pop(); }
    let b = s.into_bytes();
    let p = unsafe { libc_malloc(b.len() + 1) } as *mut u8;
    unsafe {
        std::ptr::copy_nonoverlapping(b.as_ptr(), p, b.len());
        *p.add(b.len()) = 0;
    }
    p as i64
}
pub(crate) extern "C" fn rt_read_int() -> i64 {
    let mut s = String::new();
    if std::io::stdin().read_line(&mut s).is_err() { return 0; }
    s.trim().parse::<i64>().unwrap_or(0)
}
// 统一分配器：Windows 进程堆（HeapAlloc），与 C 运行时 gt_rt.c 一致；
// 进程堆是「整个进程共享」的，跨 CRT（libcmt/msvcrt）释放安全。
#[link(name = "kernel32")]
extern "system" { fn GetProcessHeap() -> *mut core::ffi::c_void; fn HeapAlloc(h: *mut core::ffi::c_void, flags: u32, n: usize) -> *mut u8; }
unsafe fn libc_malloc(n: usize) -> *mut u8 { HeapAlloc(GetProcessHeap(), 0, n) }

pub(crate) extern "C" fn rt_str_char_at(s: i64, i: i64) -> i64 {
    if s == 0 || i < 0 { return rt_alloc_empty(); }
    let bytes = unsafe { std::ffi::CStr::from_ptr(s as *const i8).to_bytes() };
    let mut pos = 0usize;
    let mut k = 0i64;
    while pos < bytes.len() {
        let c = bytes[pos];
        let clen = if c & 0x80 == 0 { 1 } else if c & 0xE0 == 0xC0 { 2 } else if c & 0xF0 == 0xE0 { 3 } else if c & 0xF8 == 0xF0 { 4 } else { 1 };
        if k == i {
            let p = unsafe { libc_malloc(clen + 1) };
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr().add(pos), p, clen);
                *p.add(clen) = 0;
            }
            return p as i64;
        }
        pos += clen;
        k += 1;
    }
    rt_alloc_empty()
}
pub(crate) extern "C" fn rt_str_char_len(s: i64) -> i64 {
    if s == 0 { return 0; }
    let bytes = unsafe { std::ffi::CStr::from_ptr(s as *const i8).to_bytes() };
    let mut pos = 0usize;
    let mut n = 0i64;
    while pos < bytes.len() {
        let c = bytes[pos];
        let clen = if c & 0x80 == 0 { 1 } else if c & 0xE0 == 0xC0 { 2 } else if c & 0xF0 == 0xE0 { 3 } else if c & 0xF8 == 0xF0 { 4 } else { 1 };
        pos += clen;
        n += 1;
    }
    n
}
pub(crate) extern "C" fn rt_set_at(p: i64, i: i64) -> i64 {
    unsafe {
        let s = &*(p as *const RtSet);
        s.data[i as usize]
    }
}
pub(crate) extern "C" fn rt_map_key_at(p: i64, i: i64) -> i64 {
    unsafe {
        let m = &*(p as *const RtMap);
        m.keys[i as usize]
    }
}
fn rt_alloc_empty() -> i64 {
    let p = unsafe { libc_malloc(1) };
    unsafe { *p = 0; }
    p as i64
}

pub(crate) extern "C" fn rt_str_concat(a: i64, b: i64) -> i64 {
    let (pa, la) = if a == 0 { (std::ptr::null(), 0usize) } else { (a as *const u8, unsafe { libc_strlen(a as *const u8) }) };
    let (pb, lb) = if b == 0 { (std::ptr::null(), 0usize) } else { (b as *const u8, unsafe { libc_strlen(b as *const u8) }) };
    let total = la + lb;
    let p = unsafe { libc_malloc(total + 1) };
    unsafe {
        if la > 0 { std::ptr::copy_nonoverlapping(pa, p, la); }
        if lb > 0 { std::ptr::copy_nonoverlapping(pb, p.add(la), lb); }
        *p.add(total) = 0;
    }
    p as i64
}

unsafe fn libc_strlen(p: *const u8) -> usize {
    let mut n = 0usize;
    while *p.add(n) != 0 { n += 1; }
    n
}
