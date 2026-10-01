//! GT 标准库 —— http 模块（编译为 http.dll + http.lib）
//! Python-requests 风格：get / post / put / delete / request / download / status
//! HTTP 走 TcpStream（Win7+）；HTTPS 走 WinHttp（Windows 内置，Win7+）。

#![allow(clippy::missing_safety_doc)]

use std::ffi::CStr;
use std::io::{Read, Write};
use std::net::TcpStream;
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

fn parse_url(url: &str) -> Option<(String, String, u16, String)> {
    let (scheme, rest) = if let Some(r) = url.strip_prefix("https://") { ("https", r) }
        else if let Some(r) = url.strip_prefix("http://") { ("http", r) }
        else { return None };
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match hostport.find(':') {
        Some(i) => (hostport[..i].to_string(), hostport[i+1..].parse::<u16>().unwrap_or(80)),
        None => (hostport.to_string(), if scheme == "https" { 443 } else { 80 }),
    };
    Some((scheme.to_string(), host, port, path.to_string()))
}

fn http_request(method: &str, url: &str, body: &str) -> String {
    let (scheme, host, port, path) = match parse_url(url) {
        Some(v) => v,
        None => return String::new(),
    };
    if scheme == "https" {
        return winhttp_request(method, &host, port, &path, body);
    }
    let mut stream = match TcpStream::connect((host.as_str(), port)) { Ok(s) => s, Err(_) => return String::new() };
    let req = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\nUser-Agent: GTLang/0.0.1\r\n\r\n{}",
        method, path, host, body.len(), body
    );
    if stream.write_all(req.as_bytes()).is_err() { return String::new(); }
    let mut resp = Vec::new();
    if stream.read_to_end(&mut resp).is_err() { return String::new(); }
    let text = String::from_utf8_lossy(&resp).into_owned();
    match text.find("\r\n\r\n") {
        Some(i) => text[i+4..].to_string(),
        None => text,
    }
}

// ---------- WinHttp（Win7+ 内置）----------
type HInternet = *mut core::ffi::c_void;

#[link(name = "winhttp")]
extern "system" {
    fn WinHttpOpen(agent: *const u16, access: u32, proxy: *const u16, bypass: *const u16, flags: u32) -> HInternet;
    fn WinHttpConnect(session: HInternet, server: *const u16, port: u16, reserved: u32) -> HInternet;
    fn WinHttpOpenRequest(connect: HInternet, verb: *const u16, path: *const u16, version: *const u16, referrer: *const u16, accept: *const *const u16, flags: u32) -> HInternet;
    fn WinHttpSendRequest(req: HInternet, headers: *const u16, headers_len: u32, optional: *const core::ffi::c_void, optional_len: u32, total_len: u32, context: usize) -> i32;
    fn WinHttpReceiveResponse(req: HInternet, reserved: *mut core::ffi::c_void) -> i32;
    fn WinHttpReadData(req: HInternet, buffer: *mut core::ffi::c_void, to_read: u32, read: *mut u32) -> i32;
    fn WinHttpCloseHandle(h: HInternet) -> i32;
}

fn winhttp_request(method: &str, host: &str, port: u16, path: &str, body: &str) -> String {
    fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }
    unsafe {
        let session = WinHttpOpen(wide("GTLang/0.0.1").as_ptr(), 0, std::ptr::null(), std::ptr::null(), 0);
        if session.is_null() { return String::new(); }
        let conn = WinHttpConnect(session, wide(host).as_ptr(), port, 0);
        if conn.is_null() { WinHttpCloseHandle(session); return String::new(); }
        let req = WinHttpOpenRequest(conn, wide(method).as_ptr(), wide(path).as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0x00800000 /* SECURE */);
        if req.is_null() { WinHttpCloseHandle(conn); WinHttpCloseHandle(session); return String::new(); }
        let ok = WinHttpSendRequest(
            req, std::ptr::null(), 0,
            if body.is_empty() { std::ptr::null() } else { body.as_ptr() as *const core::ffi::c_void },
            body.len() as u32, body.len() as u32, 0);
        if ok == 0 { WinHttpCloseHandle(req); WinHttpCloseHandle(conn); WinHttpCloseHandle(session); return String::new(); }
        if WinHttpReceiveResponse(req, std::ptr::null_mut()) == 0 { WinHttpCloseHandle(req); WinHttpCloseHandle(conn); WinHttpCloseHandle(session); return String::new(); }
        let mut out: Vec<u8> = Vec::new();
        loop {
            let mut buf = [0u8; 8192];
            let mut read: u32 = 0;
            if WinHttpReadData(req, buf.as_mut_ptr() as *mut core::ffi::c_void, 8192, &mut read) == 0 { break; }
            if read == 0 { break; }
            out.extend_from_slice(&buf[..read as usize]);
        }
        WinHttpCloseHandle(req);
        WinHttpCloseHandle(conn);
        WinHttpCloseHandle(session);
        String::from_utf8_lossy(&out).into_owned()
    }
}

/// http.get(url) -> str
#[no_mangle]
pub extern "C" fn py_http_get(url: *const c_char) -> *mut c_char {
    ret_string(http_request("GET", &unsafe { to_string(url) }, ""))
}
/// http.post(url, body) -> str
#[no_mangle]
pub extern "C" fn py_http_post(url: *const c_char, body: *const c_char) -> *mut c_char {
    ret_string(http_request("POST", &unsafe { to_string(url) }, &unsafe { to_string(body) }))
}
/// http.put(url, body) -> str
#[no_mangle]
pub extern "C" fn py_http_put(url: *const c_char, body: *const c_char) -> *mut c_char {
    ret_string(http_request("PUT", &unsafe { to_string(url) }, &unsafe { to_string(body) }))
}
/// http.delete(url) -> str
#[no_mangle]
pub extern "C" fn py_http_delete(url: *const c_char) -> *mut c_char {
    ret_string(http_request("DELETE", &unsafe { to_string(url) }, ""))
}
/// http.request(method, url, body) -> str
#[no_mangle]
pub extern "C" fn py_http_request(method: *const c_char, url: *const c_char, body: *const c_char) -> *mut c_char {
    ret_string(http_request(&unsafe { to_string(method) }, &unsafe { to_string(url) }, &unsafe { to_string(body) }))
}
/// http.download(url, path) -> bool
#[no_mangle]
pub extern "C" fn py_http_download(url: *const c_char, path: *const c_char) -> std::os::raw::c_int {
    let body = http_request("GET", &unsafe { to_string(url) }, "");
    if body.is_empty() { return 0; }
    match std::fs::write(&unsafe { to_string(path) }, body.as_bytes()) { Ok(_) => 1, Err(_) => 0 }
}
/// http.status(url) -> int
#[no_mangle]
pub extern "C" fn py_http_status(url: *const c_char) -> i64 {
    // 尝试 HEAD
    let u = unsafe { to_string(url) };
    let (scheme, host, port, path) = match parse_url(&u) { Some(v) => v, None => return -1 };
    if scheme == "https" { return -1; } // WinHttp 版本可扩展
    let mut stream = match TcpStream::connect((host.as_str(), port)) { Ok(s) => s, Err(_) => return -1 };
    let req = format!("HEAD {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", path, host);
    if stream.write_all(req.as_bytes()).is_err() { return -1; }
    let mut resp = Vec::new();
    if stream.read_to_end(&mut resp).is_err() { return -1; }
    let text = String::from_utf8_lossy(&resp);
    // HTTP/1.1 200 OK
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("HTTP/") {
            let parts: Vec<&str> = rest.splitn(3, ' ').collect();
            if parts.len() >= 2 { return parts[1].parse::<i64>().unwrap_or(-1); }
            break;
        }
    }
    -1
}
