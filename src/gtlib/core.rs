//! GT 标准库 —— core 模块（编译为 core.dll + core.lib）
//! 基础工具：内存释放、版本信息等。

#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

unsafe fn to_string(p: *const c_char) -> String {
    if p.is_null() { return String::new(); }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}
fn ret_string(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()).into_raw()
}

/// core.free(ptr)：释放由标准库（CString::into_raw）分配的字符串。
/// 必须用本函数释放标准库返回的串；用 GTLang 的 mem_free 会因跨 CRT 崩溃。
#[no_mangle]
pub extern "C" fn py_free(ptr: i64) {
    if ptr == 0 { return; }
    unsafe { drop(CString::from_raw(ptr as *mut c_char)); }
}

/// core.version() -> str
#[no_mangle]
pub extern "C" fn py_core_version() -> *mut c_char {
    ret_string("gtcore 0.0.1".to_string())
}

/// core.echo(s) -> str（原样返回，用于测试）
#[no_mangle]
pub extern "C" fn py_core_echo(s: *const c_char) -> *mut c_char {
    ret_string(unsafe { to_string(s) })
}