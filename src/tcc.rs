//! libtcc 动态绑定：解释器后端用它**在内存里**编译内联 C 块。
//!
//! 为什么动态加载而不是构建期链接：
//!   - `gtc.exe` 不引入任何非系统 DLL 依赖，没有 TCC 时仍能编译/解释纯 GTLang；
//!   - TCC 工具链位置可配置（`GTC_TCC` 环境变量或项目内 `toolchain/tcc`）。
//!
//! 编译产物留在 TCC 自己管理的内存里，**不落盘**，符合"中间文件只进系统 temp"的约定。

use std::ffi::{c_char, c_int, c_void, CString};
use std::path::Path;

// ============================================================
// Win32 动态加载
// ============================================================

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(h: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(h: *mut c_void) -> c_int;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 一个已加载的 libtcc 及其函数指针表
pub struct LibTcc {
    handle: *mut c_void,
    new: unsafe extern "C" fn() -> *mut c_void,
    delete: unsafe extern "C" fn(*mut c_void),
    set_lib_path: unsafe extern "C" fn(*mut c_void, *const c_char),
    set_error_func: unsafe extern "C" fn(*mut c_void, *mut c_void, TccErrorFn),
    set_output_type: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    compile_string: unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
    add_include_path: unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
    add_library_path: unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
    relocate: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int,
    get_symbol: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    add_symbol: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_void) -> c_int,
}

type TccErrorFn = extern "C" fn(*mut c_void, *const c_char);

/// TCC 的 `TCC_OUTPUT_MEMORY`：产物留在内存，可直接取符号执行
const TCC_OUTPUT_MEMORY: c_int = 1;
/// `TCC_RELOCATE_AUTO`：由 TCC 自行分配并管理代码内存
const TCC_RELOCATE_AUTO: *mut c_void = 1 as *mut c_void;

thread_local! {
    /// TCC 的错误回调没有用户数据可用，用线程局部变量收集诊断
    static TCC_MSG: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

extern "C" fn on_tcc_error(_opaque: *mut c_void, msg: *const c_char) {
    if msg.is_null() {
        return;
    }
    let s = unsafe { std::ffi::CStr::from_ptr(msg) }.to_string_lossy();
    TCC_MSG.with(|m| m.borrow_mut().push_str(&s));
}

impl LibTcc {
    /// 从 TCC 工具链目录加载 `libtcc.dll`
    pub fn load(tcc_dir: &Path) -> Result<LibTcc, String> {
        let dll = tcc_dir.join("libtcc.dll");
        if !dll.is_file() {
            return Err(format!("未找到 {}", dll.display()));
        }
        let w = wide(&dll.to_string_lossy());
        let handle = unsafe { LoadLibraryW(w.as_ptr()) };
        if handle.is_null() {
            return Err(format!("无法加载 {}", dll.display()));
        }

        // 逐个取函数指针；任一缺失都说明 DLL 不匹配
        unsafe fn sym<T>(h: *mut c_void, name: &str) -> Result<T, String> {
            let c = CString::new(name).unwrap();
            let p = unsafe { GetProcAddress(h, c.as_ptr()) };
            if p.is_null() {
                return Err(format!("libtcc.dll 缺少导出符号 {}", name));
            }
            Ok(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&p) })
        }
        unsafe {
            Ok(LibTcc {
                handle,
                new: sym(handle, "tcc_new")?,
                delete: sym(handle, "tcc_delete")?,
                set_lib_path: sym(handle, "tcc_set_lib_path")?,
                set_error_func: sym(handle, "tcc_set_error_func")?,
                set_output_type: sym(handle, "tcc_set_output_type")?,
                compile_string: sym(handle, "tcc_compile_string")?,
                add_include_path: sym(handle, "tcc_add_include_path")?,
                add_library_path: sym(handle, "tcc_add_library_path")?,
                relocate: sym(handle, "tcc_relocate")?,
                get_symbol: sym(handle, "tcc_get_symbol")?,
                add_symbol: sym(handle, "tcc_add_symbol")?,
            })
        }
    }

    /// 编译一段 C 源码并完成重定位，返回可取符号的会话。
    ///
    /// 会话通过 `Rc` 持有库引用，因此可以独立于加载处返回与保存。
    pub fn compile(
        self: &std::rc::Rc<LibTcc>,
        code: &str,
        tcc_dir: &Path,
    ) -> Result<TccSession, String> {
        self.compile_parts(&[code], tcc_dir)
    }

    /// 分片编译：每个片段**各自的行号从 1 开始**，便于报错对应用户代码。
    ///
    /// 典型用法 `[自动生成的桥接头, 用户 C 块]`：这样 C 块里的错误行号就是
    /// 用户在 `C { ... }` 内看到的行号，不会因为前面插了桥接头而整体偏移。
    pub fn compile_parts(
        self: &std::rc::Rc<LibTcc>,
        parts: &[&str],
        tcc_dir: &Path,
    ) -> Result<TccSession, String> {
        let dir = CString::new(tcc_dir.to_string_lossy().as_bytes())
            .map_err(|_| "TCC 路径含 NUL 字符".to_string())?;

        TCC_MSG.with(|m| m.borrow_mut().clear());
        let st = unsafe { (self.new)() };
        if st.is_null() {
            return Err("tcc_new() 失败".into());
        }

        unsafe {
            (self.set_error_func)(st, std::ptr::null_mut(), on_tcc_error);
            // lib 路径决定 TCC 去哪里找 include/ 与 lib/
            (self.set_lib_path)(st, dir.as_ptr());
            (self.add_include_path)(st, dir.as_ptr());
            (self.add_library_path)(st, dir.as_ptr());
            if (self.set_output_type)(st, TCC_OUTPUT_MEMORY) < 0 {
                (self.delete)(st);
                return Err("tcc_set_output_type 失败".into());
            }
            for p in parts {
                if p.trim().is_empty() {
                    continue;
                }
                let src = CString::new(*p).map_err(|_| "内联 C 代码含 NUL 字符".to_string())?;
                if (self.compile_string)(st, src.as_ptr()) < 0 {
                    (self.delete)(st);
                    let msg = TCC_MSG.with(|m| m.borrow().clone());
                    return Err(format!(
                        "内联 C 编译失败（报错行号对应 C 块内的行）：\n{}",
                        msg.trim()
                    ));
                }
            }
            if (self.relocate)(st, TCC_RELOCATE_AUTO) < 0 {
                (self.delete)(st);
                let msg = TCC_MSG.with(|m| m.borrow().clone());
                return Err(format!("内联 C 链接失败：\n{}", msg.trim()));
            }
        }
        Ok(TccSession { lib: std::rc::Rc::clone(self), st })
    }
}

impl Drop for LibTcc {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.handle);
        }
    }
}

/// 一次编译会话：可查询符号地址。
/// 通过 `Rc` 持有 `LibTcc`，保证会话存活期间 DLL 与代码内存都有效。
pub struct TccSession {
    lib: std::rc::Rc<LibTcc>,
    st: *mut c_void,
}

impl TccSession {
    /// 取符号地址；找不到返回 `None`
    pub fn symbol(&self, name: &str) -> Option<usize> {
        let c = CString::new(name).ok()?;
        let p = unsafe { (self.lib.get_symbol)(self.st, c.as_ptr()) };
        if p.is_null() {
            None
        } else {
            Some(p as usize)
        }
    }

    /// 把一个宿主函数地址注册进 TCC，使 C 代码可以调用它（C → GTLang 方向）
    pub fn add_symbol(&self, name: &str, addr: usize) -> Result<(), String> {
        let c = CString::new(name).map_err(|_| format!("符号名 {} 含 NUL", name))?;
        let r = unsafe { (self.lib.add_symbol)(self.st, c.as_ptr(), addr as *const c_void) };
        if r < 0 {
            return Err(format!("tcc_add_symbol({}) 失败", name));
        }
        Ok(())
    }
}

impl Drop for TccSession {
    fn drop(&mut self) {
        unsafe {
            (self.lib.delete)(self.st);
        }
    }
}

/// 该环境是否可用内联 C（有 libtcc.dll 即可）
pub fn available(tcc_dir: &Path) -> bool {
    tcc_dir.join("libtcc.dll").is_file()
}