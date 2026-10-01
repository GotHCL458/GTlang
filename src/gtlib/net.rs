//! GT 标准库 —— net 模块（编译为 net.dll + net.lib）
//! Python socket 风格：tcp_connect / tcp_listen / send / recv / close
//! 句柄用 i64（指向 Box<TcpStream> / TcpListener 的指针）。

#![allow(clippy::missing_safety_doc)]

use std::ffi::CStr;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
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

/// net.tcp_connect(host, port) -> i64（失败 0）
#[no_mangle]
pub extern "C" fn py_tcp_connect(host: *const c_char, port: i64) -> i64 {
    let h = unsafe { to_string(host) };
    match TcpStream::connect((h.as_str(), port as u16)) {
        Ok(s) => Box::into_raw(Box::new(s)) as i64,
        Err(_) => 0,
    }
}

/// net.tcp_listen(port) -> i64（失败 0）
#[no_mangle]
pub extern "C" fn py_tcp_listen(port: i64) -> i64 {
    match TcpListener::bind(("0.0.0.0", port as u16)) {
        Ok(l) => Box::into_raw(Box::new(l)) as i64,
        Err(_) => 0,
    }
}

/// net.accept(listener) -> i64（阻塞；失败 0）
#[no_mangle]
pub extern "C" fn py_accept(listener: i64) -> i64 {
    if listener == 0 { return 0; }
    let l = unsafe { &*(listener as *const TcpListener) };
    match l.accept() {
        Ok((s, _)) => Box::into_raw(Box::new(s)) as i64,
        Err(_) => 0,
    }
}

/// net.send(conn, data) -> i64（发送字节数，失败 -1）
#[no_mangle]
pub extern "C" fn py_net_send(conn: i64, data: *const c_char) -> i64 {
    if conn == 0 { return -1; }
    let mut s = unsafe { &*(conn as *const TcpStream) };
    let d = unsafe { to_string(data) };
    match s.write_all(d.as_bytes()) { Ok(_) => d.len() as i64, Err(_) => -1 }
}

/// net.recv(conn, max) -> str（读到 EOF 或 max 字节）
#[no_mangle]
pub extern "C" fn py_net_recv(conn: i64, max: i64) -> *mut c_char {
    if conn == 0 { return ret_string(String::new()); }
    let mut s = unsafe { &*(conn as *const TcpStream) };
    let n = if max <= 0 { 8192usize } else { max as usize };
    let mut buf = vec![0u8; n];
    match s.read(&mut buf) {
        Ok(k) => ret_string(String::from_utf8_lossy(&buf[..k]).into_owned()),
        Err(_) => ret_string(String::new()),
    }
}

/// net.recv_all(conn) -> str
#[no_mangle]
pub extern "C" fn py_recv_all(conn: i64) -> *mut c_char {
    if conn == 0 { return ret_string(String::new()); }
    let mut s = unsafe { &*(conn as *const TcpStream) };
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
    ret_string(String::from_utf8_lossy(&out).into_owned())
}

/// net.close(conn)
#[no_mangle]
pub extern "C" fn py_net_close(conn: i64) {
    if conn == 0 { return; }
    unsafe { drop(Box::from_raw(conn as *mut TcpStream)); }
}

/// net.close_listener(l)
#[no_mangle]
pub extern "C" fn py_close_listener(l: i64) {
    if l == 0 { return; }
    unsafe { drop(Box::from_raw(l as *mut TcpListener)); }
}

/// net.peer_addr(conn) -> str
#[no_mangle]
pub extern "C" fn py_peer_addr(conn: i64) -> *mut c_char {
    if conn == 0 { return ret_string(String::new()); }
    let s = unsafe { &*(conn as *const TcpStream) };
    match s.peer_addr() { Ok(a) => ret_string(a.to_string()), Err(_) => ret_string(String::new()) }
}
