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
    extra_headers: Vec<(String, String)>,
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
/// 把 routes 文本切成"路由记录"：一条记录从"行首形如 METHOD /path ="开始，
/// 直到下一条这样的行之前（因此响应体可以含换行，例如多行 HTML）。
fn split_routes(routes: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in routes.lines() {
        let is_start = {
            let t = line.trim_start();
            let mut it = t.splitn(3, ' ');
            match (it.next(), it.next()) {
                (Some(m), Some(p)) => {
                    let m_up = m.to_uppercase();
                    let is_method = m_up == "GET" || m_up == "POST" || m_up == "PUT" || m_up == "DELETE" || m_up == "PATCH" || m_up == "HEAD" || m_up == "OPTIONS" || m_up == "ANY";
                    is_method && (p.starts_with('/') || p.starts_with('*'))
                }
                _ => false,
            }
        };
        if is_start && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() { cur.push('\n'); }
        cur.push_str(line);
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

/// URL 百分号解码（%XX 与 + ）
fn url_dec(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i+1..i+3], 16) { out.push(v); i += 3; continue; }
        }
        if b[i] == b'+' { out.push(b' '); i += 1; continue; }
        out.push(b[i]); i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn resolve(routes: &str, method: &str, path: &str, req_body: &str, headers: &[(String, String)]) -> Option<RouteHit> {
    let path_only = path.split('?').next().unwrap_or(path);
    let query = path.splitn(2, '?').nth(1).unwrap_or("");
    for line in split_routes(routes) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let eq = match line.find('=') { Some(i) => i, None => continue };
        let lhs = line[..eq].trim();
        let rhs = line[eq+1..].trim();
        let mut it = lhs.splitn(2, ' ');
        let m = it.next().unwrap_or("ANY").trim().to_uppercase();
        let p = it.next().unwrap_or("/").trim();
        if m != "ANY" && m != method { continue; }
        let mut params: Vec<(String, String)> = Vec::new();
        let (matched, dir_rel) = if let Some(prefix) = p.strip_suffix("/*") {
            let ok = path_only.starts_with(prefix);
            (ok, path_only.trim_start_matches(prefix).trim_start_matches('/').to_string())
        } else {
            let ps: Vec<&str> = p.trim_matches('/').split('/').collect();
            let qs: Vec<&str> = path_only.trim_matches('/').split('/').collect();
            if ps.len() == qs.len() {
                let mut ok = true;
                for (a, b) in ps.iter().zip(qs.iter()) {
                    if let Some(name) = a.strip_prefix(':') {
                        params.push((name.to_string(), b.to_string()));
                    } else if a != b {
                        ok = false; break;
                    }
                }
                (ok, path_only.trim_start_matches('/').to_string())
            } else {
                (false, String::new())
            }
        };
        if !matched { continue; }
        return Some(build_hit(rhs, &dir_rel, req_body, headers, &params, query));
    }
    None
}

/// 依据"响应描述"构造实际响应
fn build_hit(rhs: &str, path_rel: &str, req_body: &str, headers: &[(String, String)], params: &[(String, String)], query: &str) -> RouteHit {
    let mut rest = rhs.trim();
    let mut status = 200u16;
    let mut content_type = String::from("text/html; charset=utf-8");
    let mut extra_headers: Vec<(String, String)> = Vec::new();
    loop {
        if !rest.starts_with('{') { break; }
        let close = match rest.find('}') { Some(i) => i, None => break };
        let inner = &rest[1..close];
        if let Ok(code) = inner.trim().parse::<u16>() {
            status = code;
        } else if let Some(colon) = inner.find(':') {
            let h = inner[..colon].trim().to_string();
            let v = inner[colon+1..].trim().to_string();
            if h.eq_ignore_ascii_case("content-type") { content_type = v; }
            else { extra_headers.push((h, v)); }
        } else {
            break;
        }
        rest = rest[close+1..].trim();
    }
    // 相对路径解析：先按 cwd，找不到再回退到入口文件所在目录（GT_ENTRY_DIR）
    fn read_with_fallback(rel: &str) -> Option<(String, Vec<u8>)> {
        if std::path::Path::new(rel).is_absolute() {
            return std::fs::read(rel).ok().map(|b| (rel.to_string(), b));
        }
        // 相对路径：优先相对入口文件所在目录（与 import 语义一致），再相对 cwd。
        // 用 PathBuf::join 处理分隔符与 Windows \\?\ 扩展前缀。
        if let Ok(base) = std::env::var("GT_ENTRY_DIR") {
            let base = base.strip_prefix("\\\\?\\").unwrap_or(&base);
            let cand = std::path::Path::new(base).join(rel);
            if let Ok(bytes) = std::fs::read(&cand) {
                return Some((cand.to_string_lossy().into_owned(), bytes));
            }
        }
        std::fs::read(rel).ok().map(|b| (rel.to_string(), b))
    }
    if let Some(f) = rest.strip_prefix("@file:") {
        let fp = f.trim();
        if let Some((path, bytes)) = read_with_fallback(fp) {
            let ct = guess_ct(&path);
            return RouteHit { status, content_type: ct, extra_headers: Vec::new(), body: String::from_utf8_lossy(&bytes).into_owned() };
        }
        return RouteHit { status: 404, content_type: "text/plain; charset=utf-8".into(), extra_headers: Vec::new(), body: "404 file not found".into() };
    }
    if let Some(d) = rest.strip_prefix("@dir:") {
        let base = d.trim();
        let rel = path_rel.trim_start_matches('/');
        if rel.is_empty() || rel.contains("..") {
            return RouteHit { status: 404, content_type: "text/plain; charset=utf-8".into(), extra_headers: Vec::new(), body: "404".into() };
        }
        let fp = format!("{}/{}", base.trim_end_matches('/'), rel);
        if let Some((path, bytes)) = read_with_fallback(&fp) {
            let ct = guess_ct(&path);
            return RouteHit { status, content_type: ct, extra_headers: Vec::new(), body: String::from_utf8_lossy(&bytes).into_owned() };
        }
        return RouteHit { status: 404, content_type: "text/plain; charset=utf-8".into(), extra_headers: Vec::new(), body: "404 file not found".into() };
    }
    // 占位替换：{{body}} → 请求体；{{header:Name}} → 请求头；{{cookie:Name}} → Cookie 值
    let mut body = rest.replace("{{body}}", req_body);
    if body.contains("{{header:") {
        let mut out = String::with_capacity(body.len());
        let mut s = body.as_str();
        while let Some(i) = s.find("{{header:") {
            out.push_str(&s[..i]);
            s = &s[i + 9..];
            if let Some(j) = s.find("}}") {
                let name = s[..j].trim();
                let val = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).unwrap_or("");
                out.push_str(val);
                s = &s[j + 2..];
            } else { out.push_str(s); s = ""; }
        }
        out.push_str(s);
        body = out;
    }
    if body.contains("{{cookie:") {
        let cookie_hdr = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("cookie")).map(|(_, v)| v.as_str()).unwrap_or("");
        let mut out = String::with_capacity(body.len());
        let mut s = body.as_str();
        while let Some(i) = s.find("{{cookie:") {
            out.push_str(&s[..i]);
            s = &s[i + 9..];
            if let Some(j) = s.find("}}") {
                let name = s[..j].trim();
                let val = cookie_hdr.split(';').map(|p| p.trim()).find_map(|p| p.split_once('=').filter(|(k, _)| *k == name).map(|(_, v)| v)).unwrap_or("");
                out.push_str(val);
                s = &s[j + 2..];
            } else { out.push_str(s); s = ""; }
        }
        out.push_str(s);
        body = out;
    }
    if body.contains("{{param:") {
        let mut out = String::with_capacity(body.len());
        let mut s = body.as_str();
        while let Some(i) = s.find("{{param:") {
            out.push_str(&s[..i]);
            s = &s[i + 8..];
            if let Some(j) = s.find("}}") {
                let name = s[..j].trim();
                let val = params.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str()).unwrap_or("");
                out.push_str(&url_dec(val));
                s = &s[j + 2..];
            } else { out.push_str(s); s = ""; }
        }
        out.push_str(s);
        body = out;
    }
    if body.contains("{{query:") {
        let mut out = String::with_capacity(body.len());
        let mut s = body.as_str();
        while let Some(i) = s.find("{{query:") {
            out.push_str(&s[..i]);
            s = &s[i + 8..];
            if let Some(j) = s.find("}}") {
                let name = s[..j].trim();
                let val = query.split('&').filter_map(|p| p.split_once('=')).find(|(k, _)| *k == name).map(|(_, v)| v).unwrap_or("");
                out.push_str(&url_dec(val));
                s = &s[j + 2..];
            } else { out.push_str(s); s = ""; }
        }
        out.push_str(s);
        body = out;
    }
    RouteHit { status, content_type, extra_headers, body }
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

/// 处理单个连接：支持 keep-alive（同一连接处理多个请求）
fn handle_conn(mut s: TcpStream, routes: String) {
    let mut reader = match s.try_clone() { Ok(r) => BufReader::new(r), Err(_) => return };
    loop {
        let req = read_line(&mut reader);
        if req.is_empty() { break; }  // 客户端关闭
        let parts: Vec<&str> = req.split(' ').collect();
        let method = parts.get(0).copied().unwrap_or("GET").to_string();
        let path = parts.get(1).copied().unwrap_or("/").to_string();
        let mut content_len = 0usize;
        let mut headers: Vec<(String, String)> = Vec::new();
        let mut keep_alive = true;
        loop {
            let l = read_line(&mut reader);
            if l.is_empty() { break; }
            if let Some((k, v)) = l.split_once(':') {
                let k = k.trim().to_string();
                let v = v.trim().to_string();
                if k.eq_ignore_ascii_case("content-length") {
                    content_len = v.parse::<usize>().unwrap_or(0);
                }
                if k.eq_ignore_ascii_case("connection") && v.eq_ignore_ascii_case("close") {
                    keep_alive = false;
                }
                headers.push((k, v));
            }
        }
        let mut body_buf = vec![0u8; content_len];
        if content_len > 0 {
            let _ = reader.read_exact(&mut body_buf);
        }
        let req_body = String::from_utf8_lossy(&body_buf).into_owned();

        let hit = match resolve(&routes, &method, &path, &req_body, &headers) {
            Some(h) => h,
            None => RouteHit { status: 404, content_type: "text/html; charset=utf-8".into(), extra_headers: Vec::new(), body: "404 Not Found".into() },
        };
        let reason = match hit.status {
            200 => "OK", 201 => "Created", 204 => "No Content", 301 => "Moved Permanently",
            302 => "Found", 400 => "Bad Request", 403 => "Forbidden", 404 => "Not Found",
            405 => "Method Not Allowed", 500 => "Internal Server Error", _ => "OK",
        };
        let conn = if keep_alive { "keep-alive" } else { "close" };
        let mut extra = String::new();
        for (k, v) in &hit.extra_headers {
            extra.push_str(&format!("{}: {}\r\n", k, v));
        }
        let resp = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n{}\r\n{}",
            hit.status, reason, hit.content_type, hit.body.as_bytes().len(), conn, extra, hit.body
        );
        if s.write_all(resp.as_bytes()).is_err() { break; }
        if !keep_alive { break; }
    }
}

/// web.serve(port, routes) -> int（阻塞；并发处理每个连接）
#[no_mangle]
pub extern "C" fn py_serve(port: i64, routes: *const c_char) -> i64 {
    let r = unsafe { to_string(routes) };
    let listener = match TcpListener::bind(("127.0.0.1", port as u16)) {
        Ok(l) => l,
        Err(e) => { eprintln!("[web] 无法监听端口 {}：{}", port, e); return -1; }
    };
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
    ret_string(resolve(&r, &m, &p, "", &[]).map(|h| h.body).unwrap_or_default())
}
/// web.serve_fn(port, handler_addr) -> int
/// handler_addr 是 GTLang 处理函数的地址（i64）；签名约定：
///   extern "C" fn(req: *const c_char) -> *mut c_char
/// req 形如 "METHOD /path\n请求体"；返回完整响应体（可用 {status}{Header} 前缀）。
/// 仅解释器（--run）支持（函数地址在 JIT 内已知）。
#[no_mangle]
pub extern "C" fn py_serve_fn(port: i64, handler_addr: i64) -> i64 {
    if handler_addr == 0 { eprintln!("[web] serve_fn: handler 地址为空"); return -1; }
    let handler: extern "C" fn(*const c_char) -> *mut c_char = unsafe { std::mem::transmute(handler_addr as usize) };
    let listener = match TcpListener::bind(("127.0.0.1", port as u16)) {
        Ok(l) => l,
        Err(e) => { eprintln!("[web] 无法监听端口 {}：{}", port, e); return -1; }
    };
    for stream in listener.incoming() {
        let s = match stream { Ok(s) => s, Err(_) => continue };
        let h = handler;
        std::thread::spawn(move || handle_conn_fn(s, h));
    }
    0
}

/// 用回调函数处理单个连接（支持 keep-alive）
fn handle_conn_fn(mut s: TcpStream, handler: extern "C" fn(*const c_char) -> *mut c_char) {
    let mut reader = match s.try_clone() { Ok(r) => BufReader::new(r), Err(_) => return };
    loop {
        let req = read_line(&mut reader);
        if req.is_empty() { break; }
        let parts: Vec<&str> = req.split(' ').collect();
        let method = parts.get(0).copied().unwrap_or("GET");
        let path = parts.get(1).copied().unwrap_or("/");
        let mut content_len = 0usize;
        let mut keep_alive = true;
        let mut headers: Vec<String> = Vec::new();
        loop {
            let l = read_line(&mut reader);
            if l.is_empty() { break; }
            if let Some((k, v)) = l.split_once(':') {
                if k.trim().eq_ignore_ascii_case("content-length") { content_len = v.trim().parse::<usize>().unwrap_or(0); }
                if k.trim().eq_ignore_ascii_case("connection") && v.trim().eq_ignore_ascii_case("close") { keep_alive = false; }
            }
            headers.push(l.clone());
        }
        let mut body_buf = vec![0u8; content_len];
        if content_len > 0 { let _ = reader.read_exact(&mut body_buf); }
        let req_body = String::from_utf8_lossy(&body_buf).into_owned();
        // 构造传给 GTLang 处理函数的请求串（保持 "METHOD /path\nbody" 兼容）：
        //   "METHOD /path\nbody\n---HEADERS---\nHeader: v\n..."
        let req_str = format!("{} {}\n{}\n---HEADERS---\n{}", method, path, req_body, headers.join("\n"));
        let creq = match std::ffi::CString::new(req_str) { Ok(c) => c, Err(_) => break };
        let resp_ptr = handler(creq.as_ptr());
        let resp = if resp_ptr.is_null() { String::from("500 Internal Server Error") } else {
            // 注意：resp_ptr 由 GTLang 运行时（不同 CRT）分配，此处不释放以免跨 CRT 崩溃；
            // 服务端长期运行会有少量泄漏，可后续由 GTLang 侧提供释放函数解决。
            unsafe { std::ffi::CStr::from_ptr(resp_ptr).to_string_lossy().into_owned() }
        };
        // 解析响应：可带 {status}{Header: v} 前缀 + 正文
        let (status, content_type, extra, body) = parse_resp(&resp);
        let reason = match status {
            200 => "OK", 201 => "Created", 204 => "No Content", 301 => "Moved Permanently",
            302 => "Found", 400 => "Bad Request", 403 => "Forbidden", 404 => "Not Found",
            500 => "Internal Server Error", _ => "OK",
        };
        let conn = if keep_alive { "keep-alive" } else { "close" };
        let mut hdrs = String::new();
        for (k, v) in &extra { hdrs.push_str(&format!("{}: {}\r\n", k, v)); }
        let out = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n{}\r\n{}",
            status, reason, content_type, body.as_bytes().len(), conn, hdrs, body
        );
        if s.write_all(out.as_bytes()).is_err() { break; }
        if !keep_alive { break; }
    }
}

/// 解析处理函数返回的响应串：{status}/{Header: v} 前缀 + 正文
fn parse_resp(resp: &str) -> (u16, String, Vec<(String, String)>, String) {
    let mut rest = resp.trim_start();
    let mut status = 200u16;
    let mut content_type = String::from("text/html; charset=utf-8");
    let mut extra: Vec<(String, String)> = Vec::new();
    loop {
        if !rest.starts_with('{') { break; }
        let close = match rest.find('}') { Some(i) => i, None => break };
        let inner = &rest[1..close];
        if let Ok(code) = inner.trim().parse::<u16>() { status = code; }
        else if let Some(c) = inner.find(':') {
            let h = inner[..c].trim().to_string();
            let v = inner[c+1..].trim().to_string();
            if h.eq_ignore_ascii_case("content-type") { content_type = v; } else { extra.push((h, v)); }
        } else { break; }
        rest = rest[close+1..].trim_start();
    }
    (status, content_type, extra, rest.to_string())
}

extern "C" { fn free(p: *mut core::ffi::c_void); }
unsafe fn libc_free(p: *mut core::ffi::c_void) { free(p); }
