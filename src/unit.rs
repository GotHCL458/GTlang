//! 统一前端产物 `Unit`：一次前端处理的完整结果。
//!
//! 拿到 `Unit` 就说明词法/语法/类型检查全部通过；之后无论走编译后端还是
//! 解释后端，看到的都是同一份 AST。

use std::path::{Path, PathBuf};

use crate::ast::Program;
use crate::frontend::abs_of;
use crate::sema::Analysis;
use crate::tmp::TempDir;
use crate::{cblock, codegen, driver, jit};

/// 一次前端处理的完整结果：统一 AST + 语义分析 + 源码文本。
pub struct Unit {
    /// 统一抽象语法树（已回填类型）
    pub ast: Program,
    /// 语义分析结果（常量表等）
    pub analysis: Analysis,
    /// 参与本次处理的源码原文（多文件已按顺序拼接）
    pub text: String,
    /// 逻辑文件名（用于诊断）
    pub file: String,
}

impl Unit {
    /// 用解释器执行（Cranelift 即时编译，不产出文件）
    pub fn interpret(&self) -> Result<(), String> {
        jit::run(&self.ast, &self.analysis, &self.file)
    }

    /// 生成 LLVM IR 文本（编译器后端的中间产物）
    pub fn emit_llvm(&self) -> Result<String, String> {
        codegen::generate(&self.ast, &self.analysis, &self.file)
    }

    /// 交给 C 编译器的完整源码：**桥接头 + 用户内联 C 块**。
    ///
    /// 桥接头让 C 代码可以直接调用 GTLang 函数（自动生成，无需手写声明）。
    pub fn c_source(&self) -> String {
        let mut s = String::new();
        if self.ast.cblock.trim().is_empty() {
            return s;
        }
        let fns = cblock::gt_functions(&self.ast.items);
        let (bridge, _slots) = cblock::bridge_header(&fns);
        s.push_str(&bridge);
        s.push_str(&self.ast.cblock);
        s
    }

    /// 编译为可执行文件：写出 `.ll` 并调用 clang 链接，返回 exe 路径。
    ///
    /// 中间产物（运行时、内联 C 块、`.ll`）全部放在 `%TEMP%\gtc\<标签>_<pid>\`，
    /// 结束后自动清理；源码目录只出现最终 `.exe`。
    pub fn compile_to(&self, out: &Path, opt: u8) -> Result<PathBuf, String> {
        self.compile_to_ex(out, opt, false)
    }

    /// 同 `compile_to`，`keep_tmp` 为真时保留临时目录（排查问题用）
    /// 扫描 LLVM IR，返回用到的 gtlib dll 名（去重排序）。
    pub fn used_gtlib_dlls_of(ir: &str) -> Vec<&'static str> {
        let ir = ir;
        let mut set: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
        for m in crate::gtlib::MODULES {
            // 该模块的符号前缀 py_ 不统一（py_/gto_/gt_），改为按函数名查
            for f in m.funcs {
                if let Some(sf) = crate::types::gtlib_fn(f) {
                    if ir.contains(sf.symbol) { set.insert(m.dll); }
                }
            }
        }
        let mut v: Vec<&'static str> = set.into_iter().collect();
        v.sort();
        v
    }

    pub fn compile_to_ex(&self, out: &Path, opt: u8, keep_tmp: bool) -> Result<PathBuf, String> {
        let ir = self.emit_llvm()?;
        let exe = abs_of(out);
        let mut tmp = TempDir::new("compile")?;
        if keep_tmp {
            tmp.keep();
        }

        let ll = tmp.file("prog.ll");
        std::fs::write(&ll, &ir).map_err(|e| format!("无法写入 {}：{}", ll.display(), e))?;

        if let Some(dir) = exe.parent() {
            if !dir.as_os_str().is_empty() && !dir.exists() {
                std::fs::create_dir_all(dir)
                    .map_err(|e| format!("无法创建输出目录 {}：{}", dir.display(), e))?;
            }
        }

        let clang = driver::find_clang().ok_or_else(|| {
            "未找到 clang。请安装 LLVM，或用环境变量 GTC_CLANG 指定 clang 可执行文件路径。\n  \
             例如：set GTC_CLANG=D:\\LLVM\\bin\\clang.exe"
                .to_string()
        })?;
        let needed = Self::used_gtlib_dlls_of(&ir);
        driver::compile_ll(&clang, &tmp, &ll, &exe, opt, &self.c_source(), &needed)?;
        Ok(exe)
    }

    /// 仅写出 LLVM IR 到指定文件
    pub fn write_llvm(&self, out: &Path) -> Result<PathBuf, String> {
        let ir = self.emit_llvm()?;
        let ll = abs_of(out);
        std::fs::write(&ll, &ir).map_err(|e| format!("无法写入 {}：{}", ll.display(), e))?;
        Ok(ll)
    }

    /// 写出"优化后"的 LLVM IR（经 clang -O<n> 跑一遍优化 pass）。
    /// 产物可读性差（SSA/内联/向量化后），但性能就是最终代码的样子。
    pub fn write_llvm_opt(&self, out: &Path, opt: u8) -> Result<PathBuf, String> {
        let ir = self.emit_llvm()?;
        let ll = abs_of(out);
        // 先写原始 IR，再让 clang 优化并回写。
        let raw = ll.with_extension("raw.ll");
        std::fs::write(&raw, &ir).map_err(|e| format!("无法写入 {}：{}", raw.display(), e))?;
        let clang = driver::find_clang().ok_or_else(|| "找不到 clang（设置 GTC_CLANG 或加入 PATH）".to_string())?;
        let o = std::process::Command::new(&clang)
            .arg("-S").arg("-emit-llvm")
            .arg(format!("-O{}", opt))
            .arg("-o").arg(&ll)
            .arg(&raw)
            .output()
            .map_err(|e| format!("无法执行 clang：{}", e))?;
        let _ = std::fs::remove_file(&raw);
        if !o.status.success() {
            return Err(format!("clang 优化 IR 失败：\n{}", String::from_utf8_lossy(&o.stderr).trim()));
        }
        Ok(ll)
    }
}

