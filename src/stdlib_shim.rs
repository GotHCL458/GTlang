//! 生成"标准库 dll 运行期加载 + 转发 thunk"的 C 源码。
//!
//! 目标：AOT 产物**不链接任何标准库 .lib**，运行期用 LoadLibrary 加载 res 里的
//! 共享 dll（math.dll / string.dll / …），按名解析 py_* 符号。
//! 做法：编译期扫描 IR 得到用到的符号，为每个符号生成一个**同名、同签名**的转发函数，
//! 函数体经 GetProcAddress 得到的指针按正确签名调用；再用一个构造函数自动初始化。
//!
//! 这样 LLVM IR 里的 `declare double @py_sqrt(double)` 无需改动——
//! 链接时由本文件提供该符号，运行期再转到真实 dll。

use std::collections::BTreeSet;
use std::fmt::Write as _;
use crate::types::{gtlib_fn, Ty};

/// Ty → C 类型（与 LLVM IR 里的参数/返回类型逐字对应）。
fn c_ty(t: &Ty) -> &'static str {
    match t {
        Ty::F64 => "double",
        Ty::Void => "void",
        Ty::Str => "char *",
        // I64 / Bool / Unknown / 容器 / 结构体 / 枚举：均为 i64 槽或指针，用 long long 承接
        _ => "long long",
    }
}

/// 生成"标准库加载 + 转发"的 C 源码。
/// `dlls`：需要加载的 dll 名（不含扩展名）；`syms`：需要的 C 符号名。
pub fn generate_shim(dlls: &[&'static str], syms: &BTreeSet<&'static str>) -> String {
    let mut s = String::new();
    s.push_str("/* 自动生成：标准库运行期加载与转发。请勿手改。 */\n");
    s.push_str("#include <windows.h>\n#include <string.h>\n\n");

    // 每个符号：静态函数指针 + 转发函数
    // 先收集"符号 → StdFn"（按符号名反查）。
    let mut entries: Vec<(&'static str, crate::types::StdFn)> = Vec::new();
    for name in syms {
        if let Some(sf) = gtlib_fn_by_symbol(name) {
            entries.push((name, sf));
        }
    }
    entries.sort_by_key(|(n, _)| *n);

    // 函数指针 typedef + 变量
    for (sym, sf) in &entries {
        let ret = c_ty(&sf.ret);
        let params: Vec<&str> = sf.params.iter().map(|p| c_ty(p)).collect();
        let pty = if params.is_empty() { "void".to_string() } else { params.join(", ") };
        let _ = writeln!(s, "typedef {} (*gt_fp_{})({});", ret, sym, pty);
        let _ = writeln!(s, "static gt_fp_{} gt_p_{} = 0;", sym, sym);
    }
    s.push('\n');
    s.push_str("static void gt_stdlib_init(void);\n\n");

    // 转发函数（与 IR 的 declare 同名同签名）
    for (sym, sf) in &entries {
        let ret = c_ty(&sf.ret);
        let params: Vec<String> = sf.params.iter().enumerate().map(|(i, p)| format!("{} a{}", c_ty(p), i)).collect();
        let pty = if params.is_empty() { "void".to_string() } else { params.join(", ") };
        let _ = write!(s, "{} {}({}) {{ ", ret, sym, pty);
        // 兜底：构造函数若未被调用（某些工具链），首次调用时惰性初始化。
        let _ = write!(s, "if (!gt_p_{}) gt_stdlib_init(); ", sym);
        let arglist: Vec<String> = (0..sf.params.len()).map(|i| format!("a{}", i)).collect();
        if matches!(sf.ret, Ty::Void) {
            let _ = write!(s, "gt_p_{}({});", sym, arglist.join(", "));
        } else {
            let _ = write!(s, "return gt_p_{}({});", sym, arglist.join(", "));
        }
        s.push_str(" }\n");
    }
    s.push('\n');

    // 初始化：LoadLibrary + GetProcAddress
    s.push_str("static void gt_stdlib_init(void) {\n");
    let _ = writeln!(s, "    const char *dlls[] = {{ {} }};", dlls.iter().map(|d| format!("\"{}.dll\"", d)).collect::<Vec<_>>().join(", "));
    s.push_str("    int ndll = (int)(sizeof(dlls)/sizeof(dlls[0]));\n");
    s.push_str("    HMODULE hs[64]; int nh = 0;\n");
    // 以 exe 所在目录为起点，逐级向上最多 8 层，在每层尝试 <d>\dll、<d>\res\lib\dll。
    s.push_str("    char base[MAX_PATH]; DWORD n = GetModuleFileNameA(NULL, base, MAX_PATH);\n");
    s.push_str("    for (DWORD i = n; i > 0; --i) { if (base[i-1] == '\\\\' || base[i-1] == '/') { base[i] = 0; break; } }\n");
    // 优先：环境变量 GTC_STDLIB_DIR 指定的标准库目录（供测试/自定义分发）
    s.push_str("    char envdir[MAX_PATH]; DWORD en = GetEnvironmentVariableA(\"GTC_STDLIB_DIR\", envdir, MAX_PATH);\n");
    s.push_str("    for (int i = 0; i < ndll && nh < 64; ++i) {\n");
    s.push_str("        HMODULE h = 0;\n");
    s.push_str("        if (en > 0 && en < MAX_PATH) { char p[MAX_PATH]; strcpy(p, envdir); if (p[strlen(p)-1] != '\\\\') strcat(p, \"\\\\\"); strcat(p, dlls[i]); h = LoadLibraryA(p); }\n");
    s.push_str("        char root[MAX_PATH]; strcpy(root, base);\n");
    s.push_str("        for (int up = 0; up < 8 && !h; ++up) {\n");
    s.push_str("            char p[MAX_PATH]; strcpy(p, root); strcat(p, dlls[i]); h = LoadLibraryA(p);\n");
    s.push_str("            if (!h) { strcpy(p, root); strcat(p, \"res\\\\lib\\\\\"); strcat(p, dlls[i]); h = LoadLibraryA(p); }\n");
    s.push_str("            if (!h) { int L = (int)strlen(root); if (L > 0) { root[L-1] = 0; char *sl = strrchr(root, '\\\\'); if (sl) sl[1] = 0; else root[0] = 0; } }\n");
    s.push_str("        }\n");
    s.push_str("        if (!h) h = LoadLibraryA(dlls[i]);\n");
    s.push_str("        if (h) hs[nh++] = h;\n");
    s.push_str("    }\n");
    for (sym, _) in &entries {
        let _ = writeln!(s, "    for (int i = 0; i < nh; ++i) {{ FARPROC p = GetProcAddress(hs[i], \"{}\"); if (p) {{ gt_p_{} = (gt_fp_{})p; break; }} }}", sym, sym, sym);
    }
    s.push_str("}\n");
    // 构造函数（MSVC/clang 支持）
    s.push_str("#if defined(_MSC_VER)\n");
    s.push_str("#pragma section(\".CRT$XCU\", read)\n");
    s.push_str("__declspec(allocate(\".CRT$XCU\")) static void (*gt_stdlib_ctor)(void) = gt_stdlib_init;\n");
    s.push_str("#else\n");
    s.push_str("__attribute__((constructor)) static void gt_stdlib_ctor(void) { gt_stdlib_init(); }\n");
    s.push_str("#endif\n");
    s
}

/// 按 C 符号名反查签名（遍历整张 gtlib 表）。
fn gtlib_fn_by_symbol(symbol: &str) -> Option<crate::types::StdFn> {
    for m in crate::gtlib::MODULES {
        for f in m.funcs {
            if let Some(sf) = gtlib_fn(f) {
                if sf.symbol == symbol {
                    return Some(sf);
                }
            }
        }
    }
    None
}
