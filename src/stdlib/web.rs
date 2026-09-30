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

use std::io::{BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

/// 读取一行（到 \n），返回内容（去掉尾部 CRLF）
fn read_line(reader: &mut BufReader<TcpStream>) -> String {
    let mut s = String::new();
    let _ = reader.read_line(&mut s);
    s.trim_end().to_string()
}

/// 路由匹配结果
struct RouteHit {
    status: u16,
    content_type: String,
    body: String,
}

/// 解析路由文本，返回匹配项。
/// 行格式：\`METHOD /path = 响应\`
/// 响应可以是：
///   纯文本（默认 text/html；含 \`$body\` 会被替换为请求体）
///   \`{status}\` 前缀指定状态码，如 \`{201}\`
///   \`{Header: value}\` 前缀添加响应头
///   \`@file:路径\` 返回文件内容（按扩展名猜 Content-Type）
///   \`@dir:目录\`  返回该目录下的静态文件（按 URL 路径）
fn resolve(routes: &str, method: &str, path: &str, req_body: &str) -> Option<RouteHit> {
    let path_only = path.split('?').next().unwrap_or(path);
    for line in routes.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let eq = match line.find('=') { Some(i) => i, None => continue };
        let lhs = line[..eq].trim();
        let rhs = line[eq+1..].trim();
        let mut it = lhs.splitn(2, ' ');
        let m = it.next().unwrap_or("ANY").trim().to_uppercase();
        let p = it.next().unwrap_or("/").trim();
        if m != "ANY" && m != method { continue; }
        let (matched, dir_rel) = if let Some(prefix) = p.strip_suffix("/*") {
            let ok = path_only.starts_with(prefix);
            (ok, path_only.trim_start_matches(prefix).trim_start_matches('/').to_string())
        } else {
            let ps: Vec<&str> = p.trim_matches('/').split('/').collect();
            let qs: Vec<&str> = path_only.trim_matches('/').split('/').collect();
            (ps.len() == qs.len() && ps.iter().zip(qs.iter()).all(|(a,b)| a.starts_with(':') || a == b), path_only.trim_start_matches('/').to_string())
        };
        if !matched { continue; }
        return Some(build_hit(rhs, &dir_rel, req_body));
    }
    None
}

/// 依据"响应描述"构造实际响应
fn build_hit(rhs: &str, path_rel: &str, req_body: &str) -> RouteHit {
    let mut rest = rhs.trim();
    let mut status = 200u16;
    let mut content_type = String::from("text/html; charset=utf-8");
    loop {
        if !rest.starts_with('{') { break; }
        let close = match rest.find('}') { Some(i) => i, None => break };
        let inner = &rest[1..close];
        if let Ok(code) = inner.trim().parse::<u16>() {
            status = code;
        } else if let Some(colon) = inner.find(':') {
            let h = inner[..colon].trim().to_lowercase();
            let v = inner[colon+1..].trim().to_string();
            if h == "content-type" { content_type = v; }
        } else {
            break;
        }
        rest = rest[close+1..].trim();
    }
    if let Some(f) = rest.strip_prefix("@file:") {
        let fp = f.trim();
        if let Ok(bytes) = std::fs::read(fp) {
            let ct = guess_ct(fp);
            return RouteHit { status, content_type: ct, body: String::from_utf8_lossy(&bytes).into_owned() };
        }
        return RouteHit { status: 404, content_type: "text/plain; charset=utf-8".into(), body: "404 file not found".into() };
    }
    if let Some(d) = rest.strip_prefix("@dir:") {
        let base = d.trim();
        let rel = path_rel.trim_start_matches('/');
        if rel.is_empty() || rel.contains("..") {
            return RouteHit { status: 404, content_type: "text/plain; charset=utf-8".into(), body: "404".into() };
        }
        let fp = format!("{}/{}", base.trim_end_matches('/'), rel);
        if let Ok(bytes) = std::fs::read(&fp) {
            let ct = guess_ct(&fp);
            return RouteHit { status, content_type: ct, body: String::from_utf8_lossy(&bytes).into_owned() };
        }
        return RouteHit { status: 404, content_type: "text/plain; charset=utf-8".into(), body: "404 file not found".into() };
    }
    let body = rest.replace("{{body}}", req_body);
    RouteHit { status, content_type, body }
}

/// 由扩展名猜 Content-Type
fn guess_ct(path: &str) -> String {
    let lower = path.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    let ct = match ext {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    };
    ct.to_string()
}

/// 处理单个连接（读请求 → 匹配 → 写响应）
fn handle_conn(mut s: TcpStream, routes: String) {
    let mut reader = match s.try_clone() { Ok(r) => BufReader::new(r), Err(_) => return };
    let req = read_line(&mut reader);
    let parts: Vec<&str> = req.split(' ').collect();
    let method = parts.get(0).copied().unwrap_or("GET");
    let path = parts.get(1).copied().unwrap_or("/");
    let mut content_len = 0usize;
    loop {
        let l = read_line(&mut reader);
        if l.is_empty() { break; }
        if let Some((k, v)) = l.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_len = v.trim().parse::<usize>().unwrap_or(0);
            }
        }
    }
    let mut body_buf = vec![0u8; content_len];
    if content_len > 0 {
        let _ = reader.read_exact(&mut body_buf);
    }
    let req_body = String::from_utf8_lossy(&body_buf).into_owned();

    let hit = match resolve(&routes, method, path, &req_body) {
        Some(h) => h,
        None => RouteHit { status: 404, content_type: "text/html; charset=utf-8".into(), body: "404 Not Found".into() },
    };
    let reason = match hit.status {
        200 => "OK", 201 => "Created", 204 => "No Content", 301 => "Moved Permanently",
        302 => "Found", 400 => "Bad Request", 403 => "Forbidden", 404 => "Not Found",
        405 => "Method Not Allowed", 500 => "Internal Server Error", _ => "OK",
    };
    let resp = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        hit.status, reason, hit.content_type, hit.body.as_bytes().len(), hit.body
    );
    let _ = s.write_all(resp.as_bytes());
}

/// web.serve(port, routes) -> int（阻塞；并发处理每个连接）
#[no_mangle]
pub extern "C" fn py_serve(port: i64, routes: *const c_char) -> i64 {
    let r = unsafe { to_string(routes) };
    let listener = match TcpListener::bind(("127.0.0.1", port as u16)) { Ok(l) => l, Err(_) => return -1 };
    for stream in listener.incoming() {
        let s = match stream { Ok(s) => s, Err(_) => continue };
        let routes = r.clone();
        std::thread::spawn(move || handle_conn(s, routes));
    }
    0
}

/// web.match_route(routes, method, path) -> str（纯函数；返回响应体）
#[no_mangle]
pub extern "C" fn py_match_route(routes: *const c_char, method: *const c_char, path: *const c_char) -> *mut c_char {
    let r = unsafe { to_string(routes) };
    let m = unsafe { to_string(method) };
    let p = unsafe { to_string(path) };
    ret_string(resolve(&r, &m, &p, "").map(|h| h.body).unwrap_or_default())
}