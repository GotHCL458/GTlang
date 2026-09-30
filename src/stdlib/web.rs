//! GT 标准库 —— web 模块（编译为 web.dll + web.lib）
//! 极简 Web：URL 路由 + HTML 构建。基于 http 模块。
//! 不是服务器（GTLang 暂无线程池/套接字服务器 API），
//! 而是提供"路由表 + 请求分发"的纯函数工具。

#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::io::BufRead;
use std::os::raw::c_char;

unsafe fn to_string(p: *const c_char) -> String {
    if p.is_null() { return String::new(); }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}
fn ret_string(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()).into_raw()
}

/// HTML 转义
#[no_mangle]
pub extern "C" fn py_html_escape(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    ret_string(out)
}

/// URL 编码
#[no_mangle]
pub extern "C" fn py_url_encode(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    let mut out = String::with_capacity(v.len());
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    ret_string(out)
}

/// URL 解码
#[no_mangle]
pub extern "C" fn py_url_decode(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    let bytes = v.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&v[i+1..i+3], 16) { out.push(b); i += 3; continue; }
        }
        out.push(bytes[i]); i += 1;
    }
    ret_string(String::from_utf8_lossy(&out).into_owned())
}

/// 解析查询串 "a=1&b=2" -> "a=1;b=2"（分号分隔的 k=v 对）
#[no_mangle]
pub extern "C" fn py_parse_query(q: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(q) };
    let mut parts: Vec<String> = Vec::new();
    for pair in v.split('&') {
        if pair.is_empty() { continue; }
        parts.push(pair.to_string());
    }
    ret_string(parts.join(";"))
}

/// 拼接查询串：从 "k=v;k=v" 生成 "k=v&k=v"
#[no_mangle]
pub extern "C" fn py_build_query(s: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(s) };
    let mut parts: Vec<String> = Vec::new();
    for pair in v.split(';') {
        let pair = pair.trim();
        if pair.is_empty() { continue; }
        parts.push(pair.to_string());
    }
    ret_string(parts.join("&"))
}

/// 生成一个简单 HTML 页面：title + body
#[no_mangle]
pub extern "C" fn py_html_page(title: *const c_char, body: *const c_char) -> *mut c_char {
    let t = unsafe { to_string(title) };
    let b = unsafe { to_string(body) };
    let page = format!(
        "<!DOCTYPE html>\n<html>\n<head><meta charset=\"utf-8\"><title>{}</title></head>\n<body>\n{}\n</body>\n</html>\n",
        t, b
    );
    ret_string(page)
}

/// 路由匹配：pattern 支持 ":name" 段（返回匹配到的参数 "k=v;k=v"，不匹配返回 ""）
#[no_mangle]
pub extern "C" fn py_route_match(pattern: *const c_char, path: *const c_char) -> *mut c_char {
    let pat = unsafe { to_string(pattern) };
    let pth = unsafe { to_string(path) };
    let ps: Vec<&str> = pat.trim_matches('/').split('/').collect();
    let qs: Vec<&str> = pth.trim_matches('/').split('/').collect();
    if ps.len() != qs.len() { return ret_string(String::new()); }
    let mut params: Vec<String> = Vec::new();
    for (p, q) in ps.iter().zip(qs.iter()) {
        if let Some(name) = p.strip_prefix(':') {
            params.push(format!("{}={}", name, q));
        } else if p != q {
            return ret_string(String::new());
        }
    }
    ret_string(params.join(";"))
}

/// 从 "k=v;k=v" 取参数
#[no_mangle]
pub extern "C" fn py_query_get(qs: *const c_char, key: *const c_char) -> *mut c_char {
    let v = unsafe { to_string(qs) };
    let k = unsafe { to_string(key) };
    for pair in v.split(';') {
        if let Some(eq) = pair.find('=') {
            if &pair[..eq] == k.as_str() { return ret_string(pair[eq+1..].to_string()); }
        }
    }
    ret_string(String::new())
}

// ---------- 极简本地 HTTP 服务器 ----------

use std::io::{BufReader, Write};
use std::net::TcpListener;

/// 读取一行（到 \r\n），返回内容（不含 CRLF）
fn read_line(reader: &mut BufReader<std::net::TcpStream>) -> String {
    let mut s = String::new();
    let _ = reader.read_line(&mut s);
    s.trim_end().to_string()
}

/// 解析路由文本：每行 "METHOD /path=Body" 或 "ANY /path=Body"
fn match_route(routes: &str, method: &str, path: &str) -> Option<String> {
    let path_only = path.split('?').next().unwrap_or(path);
    for line in routes.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if let Some(eq) = line.find('=') {
            let lhs = line[..eq].trim();
            let body = &line[eq+1..];
            let mut it = lhs.splitn(2, ' ');
            let m = it.next().unwrap_or("ANY").trim().to_uppercase();
            let p = it.next().unwrap_or("/").trim();
            if m != "ANY" && m != method { continue; }
            // route_match 语义
            let ps: Vec<&str> = p.trim_matches('/').split('/').collect();
            let qs: Vec<&str> = path_only.trim_matches('/').split('/').collect();
            if ps.len() == qs.len() && ps.iter().zip(qs.iter()).all(|(a,b)| a.starts_with(':') || a == b) {
                return Some(body.to_string());
            }
        }
    }
    None
}

/// web.serve(port, routes) -> int（阻塞；返回 0 或错误码）
/// routes 每行 "METHOD /path=Response Body"；METHOD 可为 ANY。首行匹配优先。
#[no_mangle]
pub extern "C" fn py_serve(port: i64, routes: *const c_char) -> i64 {
    let r = unsafe { to_string(routes) };
    let listener = match TcpListener::bind(("127.0.0.1", port as u16)) { Ok(l) => l, Err(_) => return -1 };
    for stream in listener.incoming() {
        let mut s = match stream { Ok(s) => s, Err(_) => continue };
        let mut reader = BufReader::new(s.try_clone().unwrap());
        let req = read_line(&mut reader);
        let parts: Vec<&str> = req.split(' ').collect();
        let method = parts.get(0).copied().unwrap_or("GET");
        let path = parts.get(1).copied().unwrap_or("/");
        // 读完 headers
        loop {
            let l = read_line(&mut reader);
            if l.is_empty() { break; }
        }
        let body = match_route(&r, method, path).unwrap_or_else(|| "Not Found".to_string());
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.as_bytes().len(), body
        );
        let _ = s.write_all(resp.as_bytes());
    }
    0
}

/// web.match_route(routes, method, path) -> str（纯函数，便于测试）
#[no_mangle]
pub extern "C" fn py_match_route(routes: *const c_char, method: *const c_char, path: *const c_char) -> *mut c_char {
    let r = unsafe { to_string(routes) };
    let m = unsafe { to_string(method) };
    let p = unsafe { to_string(path) };
    ret_string(match_route(&r, &m, &p).unwrap_or_default())
}
