//! GT 标准库 —— sql 模块（SQLite，编译为 sql.dll + sql.lib）
//! 基于 Windows 自带的 winsqlite3.dll（Win10+）；无需外部依赖。
//! 句柄用 i64（指向内部连接结构的指针）。
//! 查询结果以 "列名\n...\n---\n行1\n行2..." 形式返回（制表符分隔列）。

#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

unsafe fn to_string(p: *const c_char) -> String {
    if p.is_null() { return String::new(); }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}
fn ret_string(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()).into_raw()
}

#[link(name = "winsqlite3")]
extern "C" {
    fn sqlite3_open(filename: *const c_char, ppDb: *mut *mut c_void) -> c_int;
    fn sqlite3_close(db: *mut c_void) -> c_int;
    fn sqlite3_exec(db: *mut c_void, sql: *const c_char, cb: *mut c_void, arg: *mut c_void, errmsg: *mut *mut c_char) -> c_int;
    fn sqlite3_prepare_v2(db: *mut c_void, sql: *const c_char, n: c_int, stmt: *mut *mut c_void, tail: *mut *const c_char) -> c_int;
    fn sqlite3_step(stmt: *mut c_void) -> c_int;
    fn sqlite3_finalize(stmt: *mut c_void) -> c_int;
    fn sqlite3_column_count(stmt: *mut c_void) -> c_int;
    fn sqlite3_column_text(stmt: *mut c_void, i: c_int) -> *const c_char;
    fn sqlite3_column_name(stmt: *mut c_void, i: c_int) -> *const c_char;
    fn sqlite3_free(p: *mut c_void);
    fn sqlite3_errmsg(db: *mut c_void) -> *const c_char;
}

/// 打开数据库文件（":memory:" 为内存库），返回句柄（0 失败）
#[no_mangle]
pub extern "C" fn py_sql_open(path: *const c_char) -> i64 {
    let p = unsafe { to_string(path) };
    let cp = CString::new(p).unwrap_or_default();
    let mut db: *mut c_void = std::ptr::null_mut();
    let rc = unsafe { sqlite3_open(cp.as_ptr(), &mut db) };
    if rc != 0 { return 0; }
    db as i64
}

/// 关闭数据库
#[no_mangle]
pub extern "C" fn py_sql_close(db: i64) {
    if db == 0 { return; }
    unsafe { sqlite3_close(db as *mut c_void); }
}

/// 执行写语句（CREATE/INSERT/UPDATE/DELETE），返回 0 成功 / 非 0 错误码
#[no_mangle]
pub extern "C" fn py_sql_exec(db: i64, sql: *const c_char) -> i64 {
    if db == 0 { return -1; }
    let s = unsafe { to_string(sql) };
    let cs = CString::new(s).unwrap_or_default();
    let mut err: *mut c_char = std::ptr::null_mut();
    let rc = unsafe { sqlite3_exec(db as *mut c_void, cs.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), &mut err) };
    if !err.is_null() { unsafe { sqlite3_free(err as *mut c_void); } }
    rc as i64
}

/// 最近一次错误信息
#[no_mangle]
pub extern "C" fn py_sql_error(db: i64) -> *mut c_char {
    if db == 0 { return ret_string(String::new()); }
    let p = unsafe { sqlite3_errmsg(db as *mut c_void) };
    ret_string(unsafe { to_string(p) })
}

/// 查询：返回 "列1\t列2\n行...\n"（首行是列名）；出错返回空串
#[no_mangle]
pub extern "C" fn py_sql_query(db: i64, sql: *const c_char) -> *mut c_char {
    if db == 0 { return ret_string(String::new()); }
    let s = unsafe { to_string(sql) };
    let cs = CString::new(s).unwrap_or_default();
    let mut stmt: *mut c_void = std::ptr::null_mut();
    let rc = unsafe { sqlite3_prepare_v2(db as *mut c_void, cs.as_ptr(), -1, &mut stmt, std::ptr::null_mut()) };
    if rc != 0 || stmt.is_null() { return ret_string(String::new()); }
    let ncol = unsafe { sqlite3_column_count(stmt) };
    let mut out = String::new();
    // 列名
    for i in 0..ncol {
        if i > 0 { out.push('\t'); }
        let name = unsafe { sqlite3_column_name(stmt, i) };
        out.push_str(&unsafe { to_string(name) });
    }
    out.push('\n');
    loop {
        let step = unsafe { sqlite3_step(stmt) };
        if step != 100 /* SQLITE_ROW */ { break; }
        for i in 0..ncol {
            if i > 0 { out.push('\t'); }
            let p = unsafe { sqlite3_column_text(stmt, i) };
            if !p.is_null() { out.push_str(&unsafe { to_string(p) }); }
        }
        out.push('\n');
    }
    unsafe { sqlite3_finalize(stmt); }
    ret_string(out)
}

/// 便捷：执行 INSERT/UPDATE/DELETE 并返回受影响行数（用 sqlite3_changes 需再声明；
/// 这里简化为执行成功返回 1，失败 0）
#[no_mangle]
pub extern "C" fn py_sql_run(db: i64, sql: *const c_char) -> i64 {
    if py_sql_exec(db, sql) == 0 { 1 } else { 0 }
}
