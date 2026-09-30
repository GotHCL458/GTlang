//! GT 标准库 —— http 模块（编译为 http.dll + http.lib）
//! Python-requests 风格：get / post / put / delete / request / download
//! HTTP 走 TcpStream；HTTPS 走系统 curl（Windows 10+ 自带）。

#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::raw::c_char;

unsafe fn to_string(p: *const c_char) -> String {
    if p.is_null() { return String::new(); }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}
fn ret_string(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()).into_raw()
}

/// 解析 URL：返回 (scheme, host, port, path)
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
        return curl_request(method, url, body);
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
    // 去掉 header，返回 body
    match text.find("\r\n\r\n") {
        Some(i) => text[i+4..].to_string(),
        None => text,
    }
}

fn curl_request(method: &str, url: &str, body: &str) -> String {
    let mut cmd = std::process::Command::new("curl");
    cmd.args(["-s", "-L", "-X", method]);
    if !body.is_empty() {
        cmd.args(["--data-binary", body]);
    }
    cmd.arg(url);
    match cmd.output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(_) => String::new(),
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
    let m = unsafe { to_string(method) };
    let u = unsafe { to_string(url) };
    let b = unsafe { to_string(body) };
    ret_string(http_request(&m, &u, &b))
}

/// http.download(url, path) -> bool
#[no_mangle]
pub extern "C" fn py_http_download(url: *const c_char, path: *const c_char) -> std::os::raw::c_int {
    let u = unsafe { to_string(url) };
    let p = unsafe { to_string(path) };
    let body = http_request("GET", &u, "");
    if body.is_empty() { return 0; }
    match std::fs::write(&p, body.as_bytes()) { Ok(_) => 1, Err(_) => 0 }
}

/// http.status(url) -> int（返回 HTTP 状态码，失败 -1）
#[no_mangle]
pub extern "C" fn py_http_status(url: *const c_char) -> i64 {
    let u = unsafe { to_string(url) };
    let out = std::process::Command::new("curl").args(["-s", "-o", "NUL", "-w", "%{http_code}", &u]).output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().parse::<i64>().unwrap_or(-1),
        Err(_) => -1,
    }
}
