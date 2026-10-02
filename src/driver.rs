//! 驱动：定位内置工具链（clang / tcc），把 `.ll` 与内联 C 块编译链接为可执行文件。
//!
//! 所有中间产物都写进 `%TEMP%\gtc\<标签>_<pid>\`（见 `tmp.rs`），
//! 结束后整体删除，源码目录只留下最终产物。

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::tmp::TempDir;

// ============================================================
// 工具链定位
// ============================================================

/// 从若干起点逐级向上查找 `toolchain/<sub>/<rel>`，使项目自带工具链、无需系统安装。
///
/// 同时支持**独立分发包**布局：`res/<sub>/<rel>`（如 `res/llvm/clang.exe`、
/// `res/tcc/libtcc.dll`），便于把依赖与产物一起分发。
fn find_in_toolchain(sub: &str, rel: &str) -> Option<PathBuf> {
    let mut starts: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            starts.push(d.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }

    for start in starts {
        let mut dir = Some(start);
        for _ in 0..8 {
            let d = match dir {
                Some(d) => d,
                None => break,
            };
            // 优先独立分发包布局 res/<sub>，其次项目内 toolchain/<sub>
            for cand in [
                d.join("res").join(sub).join(rel),
                d.join("toolchain").join(sub).join(rel),
            ] {
                if cand.is_file() {
                    return Some(cand);
                }
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }
    None
}

/// 找到可用的 clang（编译后端用）
pub fn find_clang() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(p) = std::env::var("GTC_CLANG") {
        if !p.is_empty() {
            candidates.push(PathBuf::from(p));
        }
    }
    if let Some(p) = find_in_toolchain("llvm", "bin/clang.exe") {
        candidates.push(p);
    }
    for dir in [
        "D:/LLVM/bin",
        "C:/Program Files/LLVM/bin",
        "C:/Program Files (x86)/LLVM/bin",
        "C:/LLVM/bin",
    ] {
        candidates.push(PathBuf::from(dir).join("clang.exe"));
        candidates.push(PathBuf::from(dir).join("clang"));
    }
    candidates.push(PathBuf::from("clang.exe"));
    candidates.push(PathBuf::from("clang"));

    for c in candidates {
        let ok = Command::new(&c)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Some(c);
        }
    }
    None
}

/// TCC 工具链目录（内含 `libtcc.dll`、`include/`、`lib/`）。
///
/// 解释器用它**在内存里动态编译**内联 C 块，因此需要 lib 路径来解析
/// `#include <stdio.h>` 之类的头文件。
pub fn find_tcc_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("GTC_TCC") {
        if !p.is_empty() {
            let d = PathBuf::from(p);
            if d.is_dir() {
                return Some(d);
            }
        }
    }
    // 优先用 libtcc.dll 定位（解释器只需要它 + include/lib）
    if let Some(dll) = find_in_toolchain("tcc", "libtcc.dll") {
        return dll.parent().map(|p| p.to_path_buf());
    }
    None
}

// ============================================================
// 编译后端：.ll + 运行时 + 内联 C 块 → 可执行文件
// ============================================================

/// 内置 C 运行时源码：随编译器二进制内嵌，避免运行时再去找文件。
/// GC 实现（gc.c/gc.h）与 gt_rt.c 分文件保存，编译时写入同一目录。
const GT_RT_C: &str = include_str!("runtime/gt_rt.c");
const GT_GC_C: &str = include_str!("runtime/gc.c");
const GT_GC_H: &str = include_str!("runtime/gc.h");

/// 把内置运行时 + 内联 C 块 + `.ll` 交给 clang 编译链接成可执行文件。
///
/// **运行时对象缓存**：内置 `gt_rt.c` 内容不变时无需每次重新编译——按内容哈希
/// 缓存编译好的 `.obj`（在 `%TEMP%\gtc\_rtcache\`），命中则直接参与链接，
/// 把 `--c` 的耗时从"编译 C 运行时 + 链接"降到"仅链接"。
///
/// 内联 C 块写成**独立文件** `inline_c.c` 而不是拼进运行时尾部：
/// 这样 clang 的报错是 `inline_c.c:2: ...`，行号直接对应 C 块内的行。
/// PGO 模式（`--pgo`）：
/// - `Generate(dir)`：编译带 `-fprofile-generate=dir`
/// - `Use(dir)`：编译带 `-fprofile-use=dir -fprofile-correction`
#[derive(Clone, Debug)]
pub enum PgoMode {
    Generate(std::path::PathBuf),
    Use(std::path::PathBuf),
}

pub fn compile_ll(
    clang: &Path,
    tmp: &TempDir,
    ll: &Path,
    out: &Path,
    opt: u8,
    cblock: &str,
    needed: &[&'static str],
    pgo: Option<&PgoMode>,
) -> Result<(), String> {
    // 1) 运行时：优先用**预编译静态库** gt_rt.lib（build.bat 产出，放 res/lib 或 toolchain）；
    //    找不到时回退到按内容哈希缓存的对象（首次编译后复用）。
    let rt = match find_runtime_lib() {
        Some(lib) => RtInput::Lib(lib),
        None => RtInput::Obj(runtime_object(clang, opt)?),
    };

    // 2) 内联 C 块（每次不同，放临时目录）
    let mut srcs: Vec<PathBuf> = Vec::new();
    if !cblock.trim().is_empty() {
        let cfile = tmp.file("inline_c.c");
        std::fs::write(&cfile, cblock)
            .map_err(|e| format!("无法写入内联 C 文件 {}：{}", cfile.display(), e))?;
        srcs.push(cfile);
    }

    link(clang, &rt, &srcs, ll, out, opt, needed, pgo)
}

/// 运行时的两种输入形式：预编译静态库（优先）或按需编译的对象文件。
enum RtInput {
    /// 预编译静态库 gt_rt.lib
    Lib(PathBuf),
    /// 按需编译并缓存的对象 gt_rt_<hash>.obj
    Obj(PathBuf),
}

impl RtInput {
    fn path(&self) -> &Path {
        match self {
            RtInput::Lib(p) | RtInput::Obj(p) => p,
        }
    }
}

/// 查找预编译的运行时静态库 `gt_rt.lib`：
/// 依次在 `res/lib`、`res`、`toolchain/rt`、exe 同级及其上级目录中查找。
pub fn find_runtime_lib() -> Option<PathBuf> {
    let mut starts: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            starts.push(d.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    for start in starts {
        let mut dir = Some(start);
        for _ in 0..8 {
            let d = match dir { Some(d) => d, None => break };
            for cand in [
                d.join("res").join("lib").join("gt_rt.lib"),
                d.join("res").join("gt_rt.lib"),
                d.join("toolchain").join("rt").join("gt_rt.lib"),
                d.join("gt_rt.lib"),
            ] {
                if cand.is_file() {
                    return Some(cand);
                }
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }
    None
}

/// 返回编译好的运行时对象路径；首次编译后缓存，后续按内容哈希直接复用。
fn runtime_object(clang: &Path, opt: u8) -> Result<PathBuf, String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    GT_RT_C.hash(&mut h);
    opt.hash(&mut h);
    let tag = format!("{:016x}", h.finish());

    let mut dir = std::env::temp_dir();
    dir.push("gtc");
    dir.push("_rtcache");
    let obj = dir.join(format!("gt_rt_{}.obj", tag));
    if obj.is_file() {
        return Ok(obj);
    }
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("无法创建运行时缓存目录 {}：{}", dir.display(), e))?;

    // 写源码并编译为对象（不链接）；gc.c/gc.h 与 gt_rt.c 同目录（gt_rt.c 里 #include "gc.c"）。
    let src = dir.join(format!("gt_rt_{}.c", tag));
    std::fs::write(&src, GT_RT_C)
        .map_err(|e| format!("无法写入运行时源码 {}：{}", src.display(), e))?;
    std::fs::write(dir.join("gc.c"), GT_GC_C)
        .map_err(|e| format!("无法写入 gc.c：{}", e))?;
    std::fs::write(dir.join("gc.h"), GT_GC_H)
        .map_err(|e| format!("无法写入 gc.h：{}", e))?;
    let o = Command::new(clang)
        .arg("-c")
        .arg(&src)
        .arg("-o")
        .arg(&obj)
        .arg(format!("-O{}", opt))
        .arg("-fms-runtime-lib=libcmt")
        .output()
        .map_err(|e| format!("无法执行 clang：{}", e))?;
    let _ = std::fs::remove_file(&src);
    if !o.status.success() {
        return Err(format!(
            "clang 编译内置运行时失败：\n{}",
            String::from_utf8_lossy(&o.stderr).trim()
        ));
    }
    Ok(obj)
}

/// 查找标准库静态库（math.lib / string.lib）：exe 同目录 → res/lib → cwd 逐级向上。
/// 返回找到的全部（编译器按需链接；纯 GTLang 程序不依赖也可）。
pub fn find_std_libs_for(needed: &[&'static str]) -> Vec<PathBuf> {
    let mut starts: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() { starts.push(d.to_path_buf()); }
    }
    if let Ok(cwd) = std::env::current_dir() { starts.push(cwd); }
    let mut out = Vec::new();
    for start in starts {
        let mut dir = Some(start);
        for _ in 0..8 {
            let d = match dir { Some(d) => d, None => break };
            for lib in crate::gtlib::MODULES {
                if !needed.is_empty() && !needed.contains(&lib.dll) { continue; }
                let name = format!("{}.lib", lib.dll);
                let name_dll = format!("{}.dll.lib", lib.dll);
                let name_static = format!("{}_static.lib", lib.dll);
                for cand in [
                    // 优先静态库（无运行时 dll 依赖）
                    d.join(&name_static), d.join("res").join("lib").join(&name_static),
                    d.join("res").join("lib").join(".lib").join(&name_static), d.join("lib").join(&name_static),
                    d.join(&name), d.join(&name_dll),
                    d.join("res").join("lib").join(&name), d.join("res").join("lib").join(&name_dll),
                    d.join("res").join("lib").join(".lib").join(&name), d.join("res").join("lib").join(".lib").join(&name_dll),
                    d.join("lib").join(&name), d.join("lib").join(&name_dll),
                ] {
                    if cand.is_file() {
                        if !out.contains(&cand) { out.push(cand); }
                        break; // 每个模块只用第一个命中的库
                    }
                }
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }
    out
}

fn link(clang: &Path, rt: &RtInput, srcs: &[PathBuf], ll: &Path, out: &Path, opt: u8, needed: &[&'static str], pgo: Option<&PgoMode>) -> Result<(), String> {
    // 首次带上 lld（LLVM 自带，通常位于 clang 同目录）；失败则退回默认链接器
    let attempts: Vec<Vec<String>> = vec![vec!["-fuse-ld=lld".into()], vec![]];
    // 标准库静态库：只链接"用到的"模块
    let std_libs = find_std_libs_for(needed);

    let mut last = String::new();
    for extra in attempts {
        let mut cmd = Command::new(clang);
        cmd.arg(rt.path());
        for s in srcs {
            cmd.arg(s);
        }
        cmd.arg(ll);
        cmd.arg("-o").arg(out);
        cmd.arg(format!("-O{}", opt));
        cmd.arg("-fms-runtime-lib=libcmt");
        // PGO：生成/使用 profile
        match pgo {
            Some(PgoMode::Generate(dir)) => { cmd.arg(format!("-fprofile-generate={}", dir.display())); }
            Some(PgoMode::Use(dir)) => { cmd.arg(format!("-fprofile-use={}", dir.display())); cmd.arg("-fprofile-correction"); }
            None => {}
        }
        // 极致性能（可被 GTC_NO_FAST 关闭）
        if std::env::var("GTC_NO_FAST").is_err() {
            cmd.arg("-march=native");
            cmd.arg("-funroll-loops");
            cmd.arg("-fno-stack-protector");
            cmd.arg("-fno-asynchronous-unwind-tables");
        }
        let mut syslibs: Vec<&str> = vec!["kernel32", "ws2_32", "ntdll", "userenv", "advapi32", "bcrypt", "synchronization"];
        // 用到 sql 模块时链接 Windows 自带的 SQLite
        if needed.contains(&"sql") { syslibs.push("winsqlite3"); }
        if needed.contains(&"session") { syslibs.push("winsqlite3"); }
        for syslib in &syslibs {
            cmd.arg(format!("-l{}", syslib));
        }
        for lib in &std_libs {
            cmd.arg(lib);
        }
        for e in &extra {
            cmd.arg(e);
        }
        let o = cmd.output().map_err(|e| format!("无法执行 clang：{}", e))?;
        if o.status.success() {
            return Ok(());
        }
        last = String::from_utf8_lossy(&o.stderr).to_string();
    }
    Err(format!("clang 编译失败：\n{}", last.trim()))
}
