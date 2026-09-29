//! GT 标准库 —— file 模块（编译为 file.dll + file.lib）
//! 路径与文件操作。Python 风格。
//!
//! 路径：path_exists / is_file / is_dir / getsize / listdir / basename / dirname
//!       path_join / abspath / mkdir / rmdir / remove
//! 读写：read_text / write_text / append_text / read_lines / write_lines
//!       exists / copy / size / touch
//! 二进制：read_bytes / write_bytes（返回/接收 i64 句柄的 list？此处用十六进制字符串）

#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

unsafe fn to_string(p: *const c_char) -> String {
    if p.is_null() { return String::new(); }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}
fn ret_string(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()).into_raw()
}

// ---------- 路径 ----------

/// file.path_exists(path) -> bool
#[no_mangle]
pub extern "C" fn py_path_exists(path: *const c_char) -> c_int {
    let p = unsafe { to_string(path) };
    if std::path::Path::new(&p).exists() { 1 } else { 0 }
}

/// file.exists(path) -> bool（是否为文件）
#[no_mangle]
pub extern "C" fn py_file_exists(path: *const c_char) -> c_int {
    let p = unsafe { to_string(path) };
    if std::path::Path::new(&p).is_file() { 1 } else { 0 }
}

/// file.is_file(path) -> bool
#[no_mangle]
pub extern "C" fn py_is_file(path: *const c_char) -> c_int {
    let p = unsafe { to_string(path) };
    if std::path::Path::new(&p).is_file() { 1 } else { 0 }
}

/// file.is_dir(path) -> bool
#[no_mangle]
pub extern "C" fn py_is_dir(path: *const c_char) -> c_int {
    let p = unsafe { to_string(path) };
    if std::path::Path::new(&p).is_dir() { 1 } else { 0 }
}

/// file.size(path) -> int（字节数，失败 -1）
#[no_mangle]
pub extern "C" fn py_file_size(path: *const c_char) -> i64 {
    let p = unsafe { to_string(path) };
    std::fs::metadata(&p).map(|m| m.len() as i64).unwrap_or(-1)
}

/// file.getsize(path) -> int（同 size）
#[no_mangle]
pub extern "C" fn py_getsize(path: *const c_char) -> i64 {
    py_file_size(path)
}

/// file.listdir(path) -> str（"\n" 分隔，排序）
#[no_mangle]
pub extern "C" fn py_listdir(path: *const c_char) -> *mut c_char {
    let p = unsafe { to_string(path) };
    let mut names: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&p) {
        for e in rd.flatten() { names.push(e.file_name().to_string_lossy().into_owned()); }
    }
    names.sort();
    ret_string(names.join("\n"))
}

/// file.basename(path) -> str
#[no_mangle]
pub extern "C" fn py_basename(path: *const c_char) -> *mut c_char {
    let p = unsafe { to_string(path) };
    ret_string(std::path::Path::new(&p).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
}

/// file.dirname(path) -> str
#[no_mangle]
pub extern "C" fn py_dirname(path: *const c_char) -> *mut c_char {
    let p = unsafe { to_string(path) };
    ret_string(std::path::Path::new(&p).parent().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
}

/// file.path_join(a, b) -> str
#[no_mangle]
pub extern "C" fn py_path_join(a: *const c_char, b: *const c_char) -> *mut c_char {
    let x = unsafe { to_string(a) };
    let y = unsafe { to_string(b) };
    ret_string(std::path::Path::new(&x).join(&y).to_string_lossy().into_owned())
}

/// file.abspath(path) -> str
#[no_mangle]
pub extern "C" fn py_abspath(path: *const c_char) -> *mut c_char {
    let p = unsafe { to_string(path) };
    let ap = std::fs::canonicalize(&p).map(|q| q.to_string_lossy().into_owned()).unwrap_or(p);
    ret_string(ap)
}

/// file.mkdir(path) -> bool
#[no_mangle]
pub extern "C" fn py_mkdir(path: *const c_char) -> c_int {
    let p = unsafe { to_string(path) };
    match std::fs::create_dir_all(&p) { Ok(_) => 1, Err(_) => 0 }
}

/// file.rmdir(path) -> bool（递归删除目录）
#[no_mangle]
pub extern "C" fn py_rmdir(path: *const c_char) -> c_int {
    let p = unsafe { to_string(path) };
    match std::fs::remove_dir_all(&p) { Ok(_) => 1, Err(_) => 0 }
}

/// file.remove(path) -> bool（删除文件）
#[no_mangle]
pub extern "C" fn py_remove(path: *const c_char) -> c_int {
    let p = unsafe { to_string(path) };
    match std::fs::remove_file(&p) { Ok(_) => 1, Err(_) => 0 }
}

// ---------- 文本读写 ----------

/// file.read_text(path) -> str（失败空串）
#[no_mangle]
pub extern "C" fn py_read_text(path: *const c_char) -> *mut c_char {
    let p = unsafe { to_string(path) };
    match std::fs::read_to_string(&p) { Ok(t) => ret_string(t), Err(_) => ret_string(String::new()) }
}

/// file.write_text(path, s) -> bool（覆盖）
#[no_mangle]
pub extern "C" fn py_write_text(path: *const c_char, s: *const c_char) -> c_int {
    let p = unsafe { to_string(path) };
    let v = unsafe { to_string(s) };
    match std::fs::write(&p, v.as_bytes()) { Ok(_) => 1, Err(_) => 0 }
}

/// file.append_text(path, s) -> bool
#[no_mangle]
pub extern "C" fn py_append_text(path: *const c_char, s: *const c_char) -> c_int {
    use std::io::Write;
    let p = unsafe { to_string(path) };
    let v = unsafe { to_string(s) };
    match std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        Ok(mut f) => match f.write_all(v.as_bytes()) { Ok(_) => 1, Err(_) => 0 },
        Err(_) => 0,
    }
}

/// file.read_lines(path) -> str（"\n" 分隔）
#[no_mangle]
pub extern "C" fn py_read_lines(path: *const c_char) -> *mut c_char {
    let p = unsafe { to_string(path) };
    match std::fs::read_to_string(&p) {
        Ok(t) => ret_string(t.lines().collect::<Vec<_>>().join("\n")),
        Err(_) => ret_string(String::new()),
    }
}

/// file.write_lines(path, s) -> bool（s 以 "\n" 分隔，每行写入）
#[no_mangle]
pub extern "C" fn py_write_lines(path: *const c_char, s: *const c_char) -> c_int {
    let p = unsafe { to_string(path) };
    let v = unsafe { to_string(s) };
    let mut out = String::with_capacity(v.len() + 1);
    for line in v.split('\n') { out.push_str(line); out.push('\n'); }
    match std::fs::write(&p, out.as_bytes()) { Ok(_) => 1, Err(_) => 0 }
}

/// file.copy(src, dst) -> bool
#[no_mangle]
pub extern "C" fn py_file_copy(src: *const c_char, dst: *const c_char) -> c_int {
    let a = unsafe { to_string(src) };
    let b = unsafe { to_string(dst) };
    match std::fs::copy(&a, &b) { Ok(_) => 1, Err(_) => 0 }
}

/// file.rename(src, dst) -> bool
#[no_mangle]
pub extern "C" fn py_file_rename(src: *const c_char, dst: *const c_char) -> c_int {
    let a = unsafe { to_string(src) };
    let b = unsafe { to_string(dst) };
    match std::fs::rename(&a, &b) { Ok(_) => 1, Err(_) => 0 }
}

/// file.touch(path) -> bool（创建空文件，已存在则不动）
#[no_mangle]
pub extern "C" fn py_touch(path: *const c_char) -> c_int {
    use std::io::Write;
    let p = unsafe { to_string(path) };
    if std::path::Path::new(&p).exists() { return 1; }
    match std::fs::File::create(&p) { Ok(mut f) => { let _ = f.write_all(b""); 1 } Err(_) => 0 }
}
