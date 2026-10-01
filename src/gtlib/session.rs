//! GT 标准库 —— session 模块（编译为 session.dll + session.lib）
//! 服务端会话表：会话 id 用 CSPRNG 生成，存 SQLite（表 sessions）。

#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

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
unsafe fn to_string(p: *const c_char) -> String {
    if p.is_null() { return String::new(); }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}

#[link(name = "winsqlite3")]
extern "C" {
    fn sqlite3_exec(db: *mut c_void, sql: *const c_char, cb: *mut c_void, arg: *mut c_void, errmsg: *mut *mut c_char) -> c_int;
    fn sqlite3_prepare_v2(db: *mut c_void, sql: *const c_char, n: c_int, stmt: *mut *mut c_void, tail: *mut *const c_char) -> c_int;
    fn sqlite3_step(stmt: *mut c_void) -> c_int;
    fn sqlite3_finalize(stmt: *mut c_void) -> c_int;
    fn sqlite3_column_text(stmt: *mut c_void, i: c_int) -> *const c_char;
    fn sqlite3_free(p: *mut c_void);
}

#[link(name = "bcrypt")]
extern "system" {
    fn BCryptGenRandom(algorithm: *mut c_void, buffer: *mut u8, len: u32, flags: u32) -> i32;
}
extern "C" { fn time(t: *mut i64) -> i64; }

fn rand_hex(n: usize) -> String {
    const USE_SYSTEM: u32 = 0x00000002;
    let mut b = vec![0u8; n];
    let rc = unsafe { BCryptGenRandom(std::ptr::null_mut(), b.as_mut_ptr(), n as u32, USE_SYSTEM) };
    if rc != 0 { for i in 0..n { b[i] = (i as u8).wrapping_mul(31); } }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(n * 2);
    for x in &b { s.push(HEX[(x >> 4) as usize] as char); s.push(HEX[(x & 0xf) as usize] as char); }
    s
}

fn exec(db: *mut c_void, sql: &str) {
    let c = match CString::new(sql) { Ok(c) => c, Err(_) => return };
    let mut err: *mut c_char = std::ptr::null_mut();
    unsafe { sqlite3_exec(db, c.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), &mut err); if !err.is_null() { sqlite3_free(err as *mut c_void); } }
}

fn query_one(db: *mut c_void, sql: &str) -> String {
    let c = match CString::new(sql) { Ok(c) => c, Err(_) => return String::new() };
    let mut stmt: *mut c_void = std::ptr::null_mut();
    let rc = unsafe { sqlite3_prepare_v2(db, c.as_ptr(), -1, &mut stmt, std::ptr::null_mut()) };
    if rc != 0 || stmt.is_null() { return String::new(); }
    let mut out = String::new();
    let step = unsafe { sqlite3_step(stmt) };
    if step == 100 {
        let p = unsafe { sqlite3_column_text(stmt, 0) };
        if !p.is_null() { out = unsafe { to_string(p) }; }
    }
    unsafe { sqlite3_finalize(stmt); }
    out
}

fn esc(s: &str) -> String { s.replace('\'', "''") }

/// 建表（幂等）
fn ensure(db: *mut c_void) {
    exec(db, "CREATE TABLE IF NOT EXISTS sessions (sid TEXT PRIMARY KEY, username TEXT, expire_at INTEGER)");
}

/// session.create(db, username, ttl_sec) -> sid（失败返回空串）
#[no_mangle]
pub extern "C" fn py_session_create(db: i64, user: *const c_char, ttl: i64) -> *mut c_char {
    if db == 0 { return ret_string(String::new()); }
    let d = db as *mut c_void;
    ensure(d);
    let u = unsafe { to_string(user) };
    let sid = rand_hex(24);
    let now = unsafe { time(std::ptr::null_mut()) };
    let exp = now + if ttl <= 0 { 86400 } else { ttl };
    let sql = format!("INSERT INTO sessions (sid, username, expire_at) VALUES ('{}', '{}', {})", esc(&sid), esc(&u), exp);
    exec(d, &sql);
    ret_string(sid)
}

/// session.get(db, sid) -> username（不存在/过期返回空串）
#[no_mangle]
pub extern "C" fn py_session_get(db: i64, sid: *const c_char) -> *mut c_char {
    if db == 0 { return ret_string(String::new()); }
    let d = db as *mut c_void;
    let s = unsafe { to_string(sid) };
    let now = unsafe { time(std::ptr::null_mut()) };
    let sql = format!("SELECT username FROM sessions WHERE sid='{}' AND expire_at > {}", esc(&s), now);
    ret_string(query_one(d, &sql))
}

/// session.destroy(db, sid)：删除该会话
#[no_mangle]
pub extern "C" fn py_session_destroy(db: i64, sid: *const c_char) {
    if db == 0 { return; }
    let d = db as *mut c_void;
    let s = unsafe { to_string(sid) };
    exec(d, &format!("DELETE FROM sessions WHERE sid='{}'", esc(&s)));
}

/// session.gc(db)：清理过期会话
#[no_mangle]
pub extern "C" fn py_session_gc(db: i64) {
    if db == 0 { return; }
    let now = unsafe { time(std::ptr::null_mut()) };
    exec(db as *mut c_void, &format!("DELETE FROM sessions WHERE expire_at <= {}", now));
}

/// session.count(db) -> 有效会话数
#[no_mangle]
pub extern "C" fn py_session_count(db: i64) -> i64 {
    if db == 0 { return 0; }
    let now = unsafe { time(std::ptr::null_mut()) };
    let s = query_one(db as *mut c_void, &format!("SELECT COUNT(*) FROM sessions WHERE expire_at > {}", now));
    s.parse::<i64>().unwrap_or(0)
}