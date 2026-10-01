//! GT 标准库 —— core 模块（编译为 core.dll + core.lib）
//! 基础工具：内存释放、版本信息等。

#![allow(clippy::missing_safety_doc)]

use std::ffi::CStr;
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

#[link(name = "kernel32")]
extern "system" {
    fn HeapFree(h: *mut core::ffi::c_void, flags: u32, p: *mut core::ffi::c_void) -> i32;
}

/// core.free(ptr)：释放由标准库分配的字符串（进程堆 HeapAlloc）。
/// 由于标准库统一用 Windows 进程堆，任何 CRT 都可用 HeapFree 安全释放。
#[no_mangle]
pub extern "C" fn py_free(ptr: i64) {
    if ptr == 0 { return; }
    unsafe { HeapFree(GetProcessHeap(), 0, ptr as *mut core::ffi::c_void); }
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