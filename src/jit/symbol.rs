//! 符号解析：libGT.dll 动态加载 + 宿主 C 符号查找。

// ============================================================
// 标准库 libGT.dll 动态加载
// ============================================================

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut core::ffi::c_void;
    fn GetProcAddress(h: *mut core::ffi::c_void, name: *const i8) -> *mut core::ffi::c_void;
}

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 从宿主进程 / CRT 解析一个 C 符号（用于 extern "C" 声明的 libc 函数）
pub(crate) fn resolve_host_symbol(name: &str) -> Option<usize> {
    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut core::ffi::c_void;
        fn GetProcAddress(h: *mut core::ffi::c_void, name: *const i8) -> *mut core::ffi::c_void;
        fn GetModuleHandleW(name: *const u16) -> *mut core::ffi::c_void;
    }
    let c = std::ffi::CString::new(name).ok()?;
    unsafe {
        // 先查当前进程（含静态链接的 CRT 符号）
        let cur = GetModuleHandleW(std::ptr::null());
        if !cur.is_null() {
            let p = GetProcAddress(cur, c.as_ptr());
            if !p.is_null() {
                return Some(p as usize);
            }
        }
        for dll in ["ucrtbase.dll", "msvcrt.dll", "api-ms-win-crt-string-l1-1-0.dll"] {
            let w = wide(dll);
            let h = LoadLibraryW(w.as_ptr());
            if !h.is_null() {
                let p = GetProcAddress(h, c.as_ptr());
                if !p.is_null() {
                    return Some(p as usize);
                }
            }
        }
    }
    None
}


/// 加载标准库 dll（math.dll / string.dll），返回全部 py_* 符号的 (名字, 地址)。
/// 失败时返回空表（纯 GTLang 程序不依赖标准库也能跑）。
pub(crate) fn load_stdlib() -> Vec<(&'static str, usize)> {
    use crate::types::stdlib_fn;
    let dirs = std_dll_dirs();
    let mut handles: Vec<*mut core::ffi::c_void> = Vec::new();
    for lib in crate::stdlib::MODULES {
        let name = format!("{}.dll", lib.dll);
        let mut loaded = false;
        for d in &dirs {
            let p = d.join(&name);
            if p.is_file() {
                let w = wide(&p.to_string_lossy());
                let h = unsafe { LoadLibraryW(w.as_ptr()) };
                if !h.is_null() { handles.push(h); loaded = true; break; }
            }
        }
        let _ = loaded;
    }
    let mut out = Vec::new();
    for name in STDLIB_NAMES {
        if let Some(sf) = stdlib_fn(name) {
            let c = std::ffi::CString::new(sf.symbol).unwrap();
            for h in &handles {
                let p = unsafe { GetProcAddress(*h, c.as_ptr()) };
                if !p.is_null() { out.push((sf.symbol, p as usize)); break; }
            }
        }
    }
    out
}

/// 标准库 dll 的候选目录（exe 同目录 / res/lib / cwd 逐级向上）。
pub(crate) fn std_dll_dirs() -> Vec<std::path::PathBuf> {
    let mut starts: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() { starts.push(d.to_path_buf()); }
    }
    if let Ok(cwd) = std::env::current_dir() { starts.push(cwd); }
    let mut out = Vec::new();
    for start in starts {
        let mut dir = Some(start);
        for _ in 0..8 {
            let d = match dir { Some(d) => d, None => break };
            out.push(d.clone());
            out.push(d.join("res").join("lib"));
            out.push(d.join("lib"));
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }
    out
}

/// 标准库函数名的规范列表（与 type.rs::stdlib_fn 的键一致）
pub(crate) const STDLIB_NAMES: &[&str] = &[
    // os
    // os（环境 / 进程）
    "getcwd", "getenv", "setenv", "system", "args", "exit",
    // file（路径 + 文件）
    "path_exists", "file_exists", "is_file", "is_dir", "getsize", "file_size",
    "listdir", "basename", "dirname", "path_join", "abspath", "mkdir", "rmdir", "os_remove", "remove_file",
    "read_text", "write_text", "append_text", "read_lines", "write_lines", "file_copy", "file_rename", "touch",
    "json_dump", "json_load", "json_pretty", "json_minify", "json_valid",
    // json
    "json_dumps", "json_loads",
    // toml
    "toml_loads", "toml_load", "toml_dumps",
    "sqrt", "pow", "floor", "ceil", "round", "sin", "cos", "tan", "asin", "acos", "atan",
    "atan2", "exp", "log", "log2", "log10", "fmod", "hypot", "cbrt", "gcd", "lcm", "pi", "e",
    "ipow", "isnumeric", "isdigit", "capitalize", "reverse", "count", "startswith", "endswith",
    "center", "fabs", "isqrt", "powmod", "factorial", "fib", "isprime", "comb",
    "zfill", "ljust", "rjust", "title", "swapcase", "isalpha", "isspace",
];

/// JIT 运行时也要开启控制台 VT，否则 Windows 控制台不认 ANSI 转义
pub(crate) fn enable_vt() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(n: u32) -> *mut core::ffi::c_void;
        fn GetConsoleMode(h: *mut core::ffi::c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(h: *mut core::ffi::c_void, mode: u32) -> i32;
        fn SetConsoleOutputCP(cp: u32) -> i32;
    }
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const ENABLE_VT: u32 = 0x0004;
    unsafe {
        SetConsoleOutputCP(65001); // CP_UTF8：让内联 C 的 printf 正确显示中文
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut mode = 0u32;
        if GetConsoleMode(h, &mut mode) != 0 {
            SetConsoleMode(h, mode | ENABLE_VT);
        }
    }
}
