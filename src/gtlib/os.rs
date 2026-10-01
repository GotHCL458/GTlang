//! GT 标准库 —— os 模块（编译为 os.dll + os.lib）
//! 环境与进程：getcwd / getenv / setenv / system
//! 路径与文件操作见 file 模块。

#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

unsafe fn cstr_to_string(p: *const c_char) -> String {
    if p.is_null() { return String::new(); }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}
fn ret_string(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()).into_raw()
}

/// os.getcwd() -> str
#[no_mangle]
pub extern "C" fn py_getcwd() -> *mut c_char {
    ret_string(std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())
}

/// os.getenv(name) -> str（不存在返回空串）
#[no_mangle]
pub extern "C" fn py_getenv(name: *const c_char) -> *mut c_char {
    let k = unsafe { cstr_to_string(name) };
    ret_string(std::env::var(&k).unwrap_or_default())
}

/// os.setenv(name, value) -> bool
#[no_mangle]
pub extern "C" fn py_setenv(name: *const c_char, value: *const c_char) -> std::os::raw::c_int {
    let k = unsafe { cstr_to_string(name) };
    let v = unsafe { cstr_to_string(value) };
    std::env::set_var(&k, &v);
    1
}

/// os.system(cmd) -> int（退出码）
#[no_mangle]
pub extern "C" fn py_system(cmd: *const c_char) -> i64 {
    let c = unsafe { cstr_to_string(cmd) };
    if c.is_empty() { return -1; }
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd").arg("/C").arg(&c).status();
        status.map(|s| s.code().unwrap_or(-1) as i64).unwrap_or(-1)
    }
    #[cfg(not(windows))]
    {
        let status = std::process::Command::new("sh").arg("-c").arg(&c).status();
        status.map(|s| s.code().unwrap_or(-1) as i64).unwrap_or(-1)
    }
}

/// os.args() -> str（命令行参数，"\n" 分隔）
#[no_mangle]
pub extern "C" fn py_args() -> *mut c_char {
    let args: Vec<String> = std::env::args().collect();
    ret_string(args.join("\n"))
}

/// os.exit(code)
#[no_mangle]
pub extern "C" fn py_exit(code: i64) {
    std::process::exit(code as i32);
}
