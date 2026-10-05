//! 模块系统（方案 B：真模块）。
//!
//! 每个 `.gt` 文件是一个独立模块，拥有自己的顶层符号表；只有 `pub` 标记的
//! 顶层项对外可见。`import a.b as x` 把模块绑定到本地别名 `x`。
//!
//! # 实现策略：链接期展平
//!
//! 后端（sema / jit / codegen）只见一份扁平的 `Program`，无需感知模块。
//! 链接器负责：
//!   1. 从入口文件递归解析 `import`（相对当前文件目录，再退回 `lib/`）；
//!   2. 环路检测 + 去重（同一文件只加载一次）；
//!   3. 为每个模块分配唯一前缀（`别名__`），重命名其导出符号；
//!   4. 把各模块里形如 `别名.名字` 的限定引用改写成 `别名__名字`；
//!   5. 合并为一个 `Program`。
//!
//! 命名空间隔离由前缀完成；可见性由「只导出 pub」完成；冲突由唯一前缀避免。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::*;

/// 模块链接失败：一条带文件名与源码位置的诊断
#[derive(Debug, Clone)]
pub struct ModError {
    pub file: String,
    pub message: String,
    /// 源码字节区间（解析错误时可用；路径/环路基错误时为默认值）
    pub span: Span,
}

impl ModError {
    fn new(file: impl Into<String>, message: impl Into<String>) -> ModError {
        ModError { file: file.into(), message: message.into(), span: Span::default() }
    }

    fn at(file: impl Into<String>, message: impl Into<String>, span: Span) -> ModError {
        ModError { file: file.into(), message: message.into(), span }
    }
}

/// 已加载的一个模块
struct Loaded {
    /// 规范化后的绝对路径（去重用）
    canon: PathBuf,
    /// 该模块在本编译单元里的别名（顶层前缀）
    alias: String,
    /// 解析后的程序（尚未重命名）
    prog: Program,
}

/// 模块链接器
pub struct Linker {
    /// 已加载模块，按规范路径索引
    modules: HashMap<PathBuf, usize>,
    /// 加载顺序（拓扑：被依赖者在前）
    order: Vec<Loaded>,
    /// 正在加载中的模块（环路检测）
    visiting: Vec<PathBuf>,
    /// 已用过的别名（保证前缀唯一）
    used_aliases: HashSet<String>,
    /// 每个模块的「本地别名」：同一模块可被多个名字导入
    aliases: Vec<(PathBuf, String)>,
    /// `lib/` 模块搜索根（存放 GTLang 标准库模块）
    std_roots: Vec<PathBuf>,
    /// `$GT_PATH` 附加搜索路径
    extra_roots: Vec<PathBuf>,
    /// C 头导入的函数：`(别名, C 函数名, 参数类型, 返回类型)`
    c_header_alias: Vec<(String, String, Vec<Ty>, Ty)>,
    /// 非致命错误暂存（供多错误报告合并）
    pending: Vec<ModError>,
    /// 已导入的内置标准库模块名（math/string/json/...）
    imported_gtlib: Vec<String>,
}

impl Linker {
    pub fn new(entry: &Path) -> Linker {
        let mut std_roots = Vec::new();
        // 与 driver::find_in_toolchain 同策略：从 exe / cwd 逐级向上找 lib/std
        if let Ok(exe) = std::env::current_exe() {
            if let Some(d) = exe.parent() {
                push_lib_std(&mut std_roots, d);
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            push_lib_std(&mut std_roots, &cwd);
        }
        if let Some(root) = entry.parent() {
            push_lib_std(&mut std_roots, root);
        }
        let mut extra_roots = Vec::new();
        if let Ok(p) = std::env::var("GT_PATH") {
            for seg in p.split(';') {
                if !seg.is_empty() {
                    extra_roots.push(PathBuf::from(seg));
                }
            }
        }
        Linker {
            modules: HashMap::new(),
            order: Vec::new(),
            visiting: Vec::new(),
            used_aliases: HashSet::new(),
            aliases: Vec::new(),
            std_roots,
            extra_roots,
            c_header_alias: Vec::new(),
            pending: Vec::new(),
            imported_gtlib: Vec::new(),
        }
    }

    /// 解析入口文件，返回展平后的统一 `Program`（已做重命名与引用改写）。
    pub fn link(mut self, entry: &Path) -> Result<Program, Vec<ModError>> {
        let mut errors = Vec::new();
        match self.load(entry, None, true) {
            Ok(()) => {}
            Err(e) => errors.push(e),
        }
        // 合并暂存的多错误（parse_program_multi 产生的额外诊断）
        errors.extend(std::mem::take(&mut self.pending));
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(self.flatten())
    }

    /// 递归加载一个文件。`alias_hint` 是 import 语句给的本地名（入口为 None）。
    fn load(
        &mut self,
        path: &Path,
        alias_hint: Option<String>,
        is_entry: bool,
    ) -> Result<(), ModError> {
        let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(&idx) = self.modules.get(&canon) {
            // 已加载：若这次带了别名，登记别名到该模块（后处理在 flatten 前完成）
            let _ = idx;
            self.register_alias(&canon, alias_hint);
            return Ok(());
        }
        // 环路检测
        if self.visiting.contains(&canon) {
            let chain: Vec<String> = self
                .visiting
                .iter()
                .map(|p| p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())
                .collect();
            return Err(ModError::new(
                path.display().to_string(),
                format!("检测到 import 环路：{} -> {}", chain.join(" -> "), file_name(&canon)),
            ));
        }

        let (text, _note) = crate::encoding::read_source(path)
            .map_err(|e| ModError::new(path.display().to_string(), e))?;
        let prog = match crate::parser::parse_program_multi(&text) {
            Ok(p) => p,
            Err(errs) => {
                let mut iter = errs.into_iter();
                let first = iter.next().unwrap();
                for e in iter {
                    self.pending.push(ModError::at(path.display().to_string(), e.msg, e.span));
                }
                return Err(ModError::at(path.display().to_string(), first.msg, first.span));
            }
        };

        // 该模块的别名：入口固定 "__main__"，其余用 import 给的名字
        let alias = if is_entry {
            "__main__".to_string()
        } else {
            self.pick_alias(alias_hint.unwrap_or_else(|| stem(&canon)))
        };

        self.visiting.push(canon.clone());
        let dir = canon.parent().map(|p| p.to_path_buf()).unwrap_or_default();

        // 收集并解析 import（不改动 items），记录本地别名 → 目标文件
        let imports = prog.imports.clone();
        let mut local_to_target: Vec<(String, PathBuf)> = Vec::new();
        // C 头导入：解析 .h 的函数签名，登记为 `别名.函数` 可调用；并把 #include 加入 C 块
        let mut c_header_includes: Vec<String> = Vec::new();
        let mut c_header_impls: Vec<String> = Vec::new();
        let mut c_header_fns: Vec<(String, Vec<crate::cblock::CFn>)> = Vec::new();
        for imp in &imports {
            if imp.is_c_header {
                let hdr = imp.path[0].clone();
                let hp = PathBuf::from(&hdr);
                let hpath = if hp.is_absolute() { hp } else { dir.join(&hp) };
                // 读 .h（找不到也继续——让 C 编译器去报 #include 错误）
                let raw = std::fs::read_to_string(&hpath).unwrap_or_default();
                let funcs = crate::cblock::parse_c_funcs_ex(&raw, true);
                // 若存在同名 .c 实现文件，一并纳入 C 块（供 tcc/clang 编译实现）
                let c_impl = hpath.with_extension("c");
                if c_impl.is_file() {
                    if let Ok(src) = std::fs::read_to_string(&c_impl) {
                        c_header_impls.push(src);
                    }
                }
                let alias = imp.local_name();
                // alias 缺省用文件基名（去掉 .h）
                let alias = if imp.alias.is_some() {
                    alias
                } else {
                    Path::new(&hdr)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or(alias)
                };
                c_header_includes.push(hdr.clone());
                c_header_fns.push((alias, funcs));
            }
        }
        for imp in &imports {
            if imp.is_c_header {
                continue;
            }
            let target = match self.resolve_import(&dir, imp) {
                Some(t) => t,
                None => {
                    // 找不到文件时，若为内置标准库（math/string/os/file/json/...）则跳过（运行时提供）
                    // 内置模块：`import math`（单段）或 `import boot.fs`（多段，boot 命名空间）
                    if imp.path[0] == "boot" && crate::gtlib::boot::all_funcs().iter().any(|f| !f.is_empty()) {
                        self.imported_gtlib.push(imp.path.join("."));
                        continue;
                    }
                    if imp.path.len() == 1 && crate::gtlib::MODULES.iter().any(|m| m.dll == imp.path[0].as_str()) {
                        self.imported_gtlib.push(imp.path[0].clone());
                        continue;
                    }
                    return Err(ModError::new(
                        path.display().to_string(),
                        crate::lb!(
                            imp.line,
                            "module not found: '{}'",
                            "找不到模块 '{}'",
                            imp.path.join(".")
                        ),
                    ));
                }
            };
            local_to_target.push((imp.local_name(), target.clone()));
            self.load(&target, Some(imp.local_name()), false)
                .map_err(|e| ModError::new(path.display().to_string(), e.message))?;
        }

        self.visiting.pop();
        // 应用 C 头导入：把 #include 加入 C 块，并把 .h 里的函数登记为可调用
        let mut prog = prog;
        // 先并入 .c 实现（在 #include 之前，避免重复声明问题）
        for src in &c_header_impls {
            if !prog.cblock.is_empty() {
                prog.cblock.push('\n');
            }
            prog.cblock.push_str(src);
        }
        for hdr in &c_header_includes {
            if !prog.cblock.is_empty() {
                prog.cblock.push('\n');
            }
            // 用绝对路径 include，免去给 tcc/clang 传 -I
            let abs = if std::path::Path::new(hdr).is_absolute() {
                std::path::PathBuf::from(hdr)
            } else {
                dir.join(hdr)
            };
            let inc = abs.to_string_lossy().replace('\\', "/");
            prog.cblock.push_str(&format!("#include \"{}\"", inc));
        }
        for (alias, funcs) in &c_header_fns {
            for f in funcs {
                if !prog.cfuncs.iter().any(|c| c.name == f.name) {
                    prog.cfuncs.push(f.clone());
                }
                self.c_header_alias
                    .push((alias.clone(), f.name.clone(), f.params.clone(), f.ret.clone()));
            }
        }
        let idx = self.order.len();
        self.modules.insert(canon.clone(), idx);
        self.order.push(Loaded { canon, alias, prog });
        // 记录别名（在 flatten 前统一改写）
        for (local, target) in local_to_target {
            let tcanon = std::fs::canonicalize(&target).unwrap_or(target);
            self.register_alias(&tcanon, Some(local));
        }
        Ok(())
    }

    /// 登记「模块规范路径 → 一个本地别名」。同一模块可被多次以不同名导入。
    fn register_alias(&mut self, canon: &Path, alias: Option<String>) {
        if let Some(a) = alias {
            self.aliases.push((canon.to_path_buf(), a));
        }
    }

    /// 选择唯一别名（避免与已有前缀冲突）
    fn pick_alias(&mut self, base: String) -> String {
        let base = sanitize(&base);
        let mut name = base.clone();
        let mut n = 2;
        while self.used_aliases.contains(&name) {
            name = format!("{}_{}", base, n);
            n += 1;
        }
        self.used_aliases.insert(name.clone());
        name
    }

    /// 把 `import` 解析为具体文件路径。
    fn resolve_import(&self, cur_dir: &Path, imp: &Import) -> Option<PathBuf> {
        if imp.is_file {
            let raw = &imp.path[0];
            let p = PathBuf::from(raw);
            let cand = if p.is_absolute() { p } else { cur_dir.join(p) };
            return cand.is_file().then_some(cand);
        }
        // 路径段：a.b.c → a/b/c.gt 或 a/b/c/mod.gt
        let rel: PathBuf = imp
            .path
            .iter()
            .fold(PathBuf::new(), |acc, seg| acc.join(seg));
        let mut roots: Vec<PathBuf> = vec![cur_dir.to_path_buf()];
        roots.extend(self.extra_roots.iter().cloned());
        roots.extend(self.std_roots.iter().cloned());
        for root in roots {
            let g1 = root.join(&rel).with_extension("gt");
            if g1.is_file() {
                return Some(g1);
            }
            let g2 = root.join(&rel).join("mod.gt");
            if g2.is_file() {
                return Some(g2);
            }
        }
        None
    }

    /// 展平：重命名每个模块的符号，改写限定引用，合并成一个 `Program`。
    fn flatten(&self) -> Program {
        // 1) 建立 「模块索引 → (导出名 → 全局名)」 与 别名 → 模块索引 两张表
        let mut alias_to_idx: HashMap<String, usize> = HashMap::new();
        let mut exports: Vec<HashMap<String, String>> = Vec::new();

        // 规范路径 → 模块索引（用于把 import 的本地别名绑到目标模块）
        let mut canon_to_idx: HashMap<PathBuf, usize> = HashMap::new();
        for (i, m) in self.order.iter().enumerate() {
            canon_to_idx.insert(m.canon.clone(), i);
        }
        for (canon, local) in &self.aliases {
            if let Some(&idx) = canon_to_idx.get(canon) {
                alias_to_idx.insert(local.clone(), idx);
            }
        }

        for (i, m) in self.order.iter().enumerate() {
            alias_to_idx.entry(m.alias.clone()).or_insert(i);
            let mut map = HashMap::new();
            let prefix = format!("{}__", m.alias);
            let is_entry = m.alias == "__main__";
            for item in &m.prog.items {
                match item {
                    Item::Fn(f) if f.is_pub || is_entry => {
                        // 入口模块符号保留原名；被导入模块加前缀
                        let g = if is_entry {
                            f.name.clone()
                        } else {
                            format!("{}{}", prefix, f.name)
                        };
                        map.insert(f.name.clone(), g);
                    }
                    Item::Const { name, is_pub, .. } if *is_pub || is_entry => {
                        let g = if is_entry {
                            name.clone()
                        } else {
                            format!("{}{}", prefix, name)
                        };
                        map.insert(name.clone(), g);
                    }
                    Item::Struct(s) if s.is_pub || is_entry => {
                        let g = if is_entry {
                            s.name.clone()
                        } else {
                            format!("{}{}", prefix, s.name)
                        };
                        map.insert(s.name.clone(), g);
                    }
                    _ => {}
                }
            }
            exports.push(map);
        }

        // 2) 合并 items，逐模块重命名 + 改写引用
        let mut out = Program::default();
        for (i, m) in self.order.iter().enumerate() {
            let mut prog = m.prog.clone();
            let prefix = format!("{}__", m.alias);
            let self_exports = exports[i].clone();
            // 本模块可见的「别名 → 目标模块导出表」
            let mut visible: HashMap<String, HashMap<String, String>> = HashMap::new();
            for imp in &prog.imports {
                let local = imp.local_name();
                if let Some(&tidx) = alias_to_idx.get(&local) {
                    visible.insert(local, exports[tidx].clone());
                }
            }
            // 入口模块的符号全部保留原名：这样内联 C 块里对 GTLang 函数的
            // 引用（如 `累加(...)`）、以及 `main` 入口都无需改写。
            // 只有被 import 的模块才加前缀隔离。
            let is_entry = m.alias == "__main__";
            if is_entry {
                rewrite_refs_only(&mut prog, &visible);
            } else {
                rewrite_program(&mut prog, &prefix, &self_exports, &visible, false);
            }
            out.items.extend(prog.items);
            if !prog.cblock.trim().is_empty() {
                if !out.cblock.is_empty() {
                    out.cblock.push('\n');
                }
                out.cblock.push_str(&prog.cblock);
            }
            // cfuncs 始终并入（含 extern "C" 声明，即使没有 C 块）
            out.cfuncs.extend(prog.cfuncs);
        }
        // C 头导入：把 `别名.函数(...)` 改写为 `函数(...)`（C 函数已并入 cfuncs）
        if !self.c_header_alias.is_empty() {
            let map: HashMap<String, String> = self
                .c_header_alias
                .iter()
                .map(|(a, f, _, _)| (format!("{}.{}", a, f), f.clone()))
                .collect();
            for item in out.items.iter_mut() {
                if let Item::Fn(f) = item {
                    rewrite_c_calls(&mut f.body, &map);
                }
            }
        }
        out.imported_gtlib = self.imported_gtlib.clone();
        out
    }
}

/// 把函数体内 `别名.函数(...)` 调用改写为裸函数名
fn rewrite_c_calls(b: &mut Block, map: &HashMap<String, String>) {
    for s in b.iter_mut() {
        match s {
            Stmt::Let { value, .. } | Stmt::Const { value, .. } => rewrite_c_expr(value, map),
            Stmt::Assign { value, .. } | Stmt::FieldAssign { value, .. } => rewrite_c_expr(value, map),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => rewrite_c_expr(e, map),
            Stmt::If { cond, then, els, .. } => {
                rewrite_c_expr(cond, map);
                rewrite_c_calls(then, map);
                if let Some(e) = els { rewrite_c_calls(e, map); }
            }
            Stmt::While { cond, body, .. } => { rewrite_c_expr(cond, map); rewrite_c_calls(body, map); }
            Stmt::ForRange { from, to, body, .. } => { rewrite_c_expr(from, map); rewrite_c_expr(to, map); rewrite_c_calls(body, map); }
            Stmt::ForEach { iter, body, .. } => { rewrite_c_expr(iter, map); rewrite_c_calls(body, map); }
            Stmt::Block(inner) => rewrite_c_calls(inner, map),
            _ => {}
        }
    }
}

fn rewrite_c_expr(e: &mut Expr, map: &HashMap<String, String>) {
    if let ExprKind::Call(name, args) = &mut e.kind {
        if let Some(real) = map.get(name) {
            *name = real.clone();
        }
        for a in args.iter_mut() { rewrite_c_expr(a, map); }
        return;
    }
    match &mut e.kind {
        ExprKind::Call(_, args) | ExprKind::ArrayLit(args) => for a in args.iter_mut() { rewrite_c_expr(a, map); },
        ExprKind::CallValue { callee, args } => { rewrite_c_expr(callee, map); for a in args.iter_mut() { rewrite_c_expr(a, map); } }
        ExprKind::Unary(_, a) => rewrite_c_expr(a, map),
        ExprKind::Binary(_, a, b) => { rewrite_c_expr(a, map); rewrite_c_expr(b, map); }
        ExprKind::Index(a, b) => { rewrite_c_expr(a, map); rewrite_c_expr(b, map); }
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { rewrite_c_expr(i, map); } },
        ExprKind::If { cond, then, els } => { rewrite_c_expr(cond, map); rewrite_c_calls(then, map); if let Some(x) = els { rewrite_c_calls(x, map); } }
        ExprKind::Field(base, _) => rewrite_c_expr(base, map),
        ExprKind::StructLit(_, fields) => for (_, v) in fields.iter_mut() { rewrite_c_expr(v, map); },
        ExprKind::ClosureNew { captures, .. } => for c in captures.iter_mut() { rewrite_c_expr(c, map); },
        ExprKind::Borrow { inner, .. } => rewrite_c_expr(inner, map),
        _ => {}
    }
}

/// 入口模块：不改名任何顶层符号，只把 `别名.名字` 限定引用解析成全局名。
fn rewrite_refs_only(
    prog: &mut Program,
    visible: &HashMap<String, HashMap<String, String>>,
) {
    let empty = HashMap::new();
    for item in &mut prog.items {
        match item {
            Item::Fn(f) => rewrite_block(&mut f.body, &empty, visible),
            Item::Const { value, .. } => rewrite_expr(value, &empty, visible),
            Item::Impl { methods, .. } => {
                for m in methods.iter_mut() {
                    rewrite_block(&mut m.body, &empty, visible);
                }
            }
            Item::Struct(_) | Item::Enum(_) | Item::Macro { .. } | Item::ExternC(_) | Item::Trait(_) => {}
            Item::TraitImpl { methods, .. } => {
                for m in methods.iter_mut() {
                    rewrite_block(&mut m.body, &empty, visible);
                }
            }
        }
    }
    prog.imports.clear();
}

/// 把模块内所有顶层符号加前缀；把 `别名.名字` 限定引用解析成全局名。
fn rewrite_program(
    prog: &mut Program,
    prefix: &str,
    own: &HashMap<String, String>,
    visible: &HashMap<String, HashMap<String, String>>,
    is_entry: bool,
) {
    // 顶层项重命名（入口模块的 `main` 保留原名，作为 JIT/链接入口）
    for item in &mut prog.items {
        match item {
            Item::Fn(f) => {
                if !(is_entry && f.name == "main") {
                    f.name = format!("{}{}", prefix, f.name);
                }
                // 结构体类型名加前缀（与本模块 struct 重命名一致）
                for p in f.params.iter_mut() {
                    if let Some(t) = &mut p.ty { prefix_struct_ty(t, prefix); }
                }
                if let Some(t) = &mut f.ret { prefix_struct_ty(t, prefix); }
            }
            Item::Const { name, .. } => *name = format!("{}{}", prefix, name),
            Item::Struct(s) => {
                let old = s.name.clone();
                s.name = format!("{}{}", prefix, old);
                for (_, t, _) in s.fields.iter_mut() {
                    if let Some(ty) = t {
                        rewrite_struct_ty(ty, &old, &s.name);
                    }
                }
            }
            Item::Impl { ty, methods, .. } => {
                *ty = format!("{}{}", prefix, ty);
                for m in methods.iter_mut() {
                    for p in m.params.iter_mut() {
                        if let Some(t) = &mut p.ty { prefix_struct_ty(t, prefix); }
                    }
                    if let Some(t) = &mut m.ret { prefix_struct_ty(t, prefix); }
                }
            }
            Item::ExternC(_) => {}
            Item::Macro { .. } => {}
            Item::Enum(en) => {
                en.name = format!("{}{}", prefix, en.name);
            }
            Item::Trait(t) => {
                t.name = format!("{}{}", prefix, t.name);
                for (_, params, ret) in t.methods.iter_mut() {
                    for p in params.iter_mut() { prefix_struct_ty(p, prefix); }
                    prefix_struct_ty(ret, prefix);
                }
            }
            Item::TraitImpl { trait_name, ty, methods, .. } => {
                *trait_name = format!("{}{}", prefix, trait_name);
                *ty = format!("{}{}", prefix, ty);
                for m in methods.iter_mut() {
                    for p in m.params.iter_mut() {
                        if let Some(t) = &mut p.ty { prefix_struct_ty(t, prefix); }
                    }
                    if let Some(t) = &mut m.ret { prefix_struct_ty(t, prefix); }
                }
            }
        }
    }
    // 改写函数体与常量表达式
    for item in &mut prog.items {
        match item {
            Item::Fn(f) => rewrite_block(&mut f.body, own, visible),
            Item::Const { value, .. } => rewrite_expr(value, own, visible),
            Item::Impl { methods, .. } => {
                for m in methods.iter_mut() {
                    rewrite_block(&mut m.body, own, visible);
                }
            }
            Item::Struct(_) | Item::Enum(_) | Item::Macro { .. } | Item::ExternC(_) | Item::Trait(_) => {}
            Item::TraitImpl { methods, .. } => {
                for m in methods.iter_mut() {
                    rewrite_block(&mut m.body, own, visible);
                }
            }
        }
    }
    prog.imports.clear();
}

fn rewrite_block(b: &mut Block, own: &HashMap<String, String>, visible: &HashMap<String, HashMap<String, String>>) {
    for s in b.iter_mut() {
        match s {
            Stmt::Let { value, .. } => rewrite_expr(value, own, visible),
            Stmt::Const { value, .. } => rewrite_expr(value, own, visible),
            Stmt::Throw(e, _) => rewrite_expr(e, own, visible),
            Stmt::Asm { .. } => {}
            Stmt::Go { args, .. } => for a in args.iter_mut() { rewrite_expr(a, own, visible); },
            Stmt::Labeled { inner, .. } => { let mut blk: Block = vec![(**inner).clone()]; rewrite_block(&mut blk, own, visible); *inner = Box::new(blk.into_iter().next().unwrap()); }
            Stmt::Try { body, catches, fin, .. } => {
                rewrite_block(body, own, visible);
                for ca in catches {
                    if let Some(g) = &mut ca.guard { rewrite_expr(g, own, visible); }
                    rewrite_block(&mut ca.body, own, visible);
                }
                if let Some(f) = fin { rewrite_block(f, own, visible); }
            }
            Stmt::Assign { value, index, .. } => {
                rewrite_expr(value, own, visible);
                if let Some(ix) = index {
                    rewrite_expr(ix, own, visible);
                }
            }
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => rewrite_expr(e, own, visible),
            Stmt::If { cond, then, els, .. } => {
                rewrite_expr(cond, own, visible);
                rewrite_block(then, own, visible);
                if let Some(e) = els {
                    rewrite_block(e, own, visible);
                }
            }
            Stmt::While { cond, body, .. } => {
                rewrite_expr(cond, own, visible);
                rewrite_block(body, own, visible);
            }
            Stmt::DoWhile { body, cond, .. } => {
                rewrite_block(body, own, visible);
                rewrite_expr(cond, own, visible);
            }
            Stmt::ForRange { from, to, body, .. } => {
                rewrite_expr(from, own, visible);
                rewrite_expr(to, own, visible);
                rewrite_block(body, own, visible);
            }
            Stmt::ForEach { iter, body, .. } => {
                rewrite_expr(iter, own, visible);
                rewrite_block(body, own, visible);
            }
            Stmt::Block(inner) => rewrite_block(inner, own, visible),
            Stmt::LocalFn(f) => rewrite_block(&mut f.body, own, visible),
            Stmt::FieldAssign { value, .. } => rewrite_expr(value, own, visible),
            Stmt::Return(None, _) | Stmt::Break(..) | Stmt::Continue(..) => {}
        }
    }
}

fn rewrite_expr(e: &mut Expr, own: &HashMap<String, String>, visible: &HashMap<String, HashMap<String, String>>) {
    match &mut e.kind {
        ExprKind::Ident(name) => resolve_name(name, own, visible),
        ExprKind::CallNamed(_, named) => { for (_, v) in named.iter_mut() { rewrite_expr(v, own, visible); } }
        ExprKind::TupleLit(items) => for v in items.iter_mut() { rewrite_expr(v, own, visible); },
        ExprKind::Slice(b, lo, hi) => { rewrite_expr(b, own, visible); rewrite_expr(lo, own, visible); rewrite_expr(hi, own, visible); },
        ExprKind::ListComp { expr, iter, cond, .. } => { rewrite_expr(expr, own, visible); rewrite_expr(iter, own, visible); if let Some(c) = cond { rewrite_expr(c, own, visible); } },
        ExprKind::EnumLit(_, _, args) => for a in args.iter_mut() { rewrite_expr(a, own, visible); },
        ExprKind::DynBox { value, .. } => rewrite_expr(value, own, visible),
        ExprKind::MethodOn { recv, args, .. } => { rewrite_expr(recv, own, visible); for a in args.iter_mut() { rewrite_expr(a, own, visible); } }
        ExprKind::Call(name, args) => {
            for a in args.iter_mut() {
                rewrite_expr(a, own, visible);
            }
            resolve_name(name, own, visible);
        }
        ExprKind::Unary(_, a) => rewrite_expr(a, own, visible),
        ExprKind::Binary(_, a, b) => {
            rewrite_expr(a, own, visible);
            rewrite_expr(b, own, visible);
        }
        ExprKind::Index(a, b) => {
            rewrite_expr(a, own, visible);
            rewrite_expr(b, own, visible);
        }
        ExprKind::ArrayLit(items) => {
            for it in items.iter_mut() {
                rewrite_expr(it, own, visible);
            }
        }
        ExprKind::Interp(parts) => {
            for p in parts.iter_mut() {
                if let StrPart::Expr(inner) = p {
                    rewrite_expr(inner, own, visible);
                }
            }
        }
        ExprKind::If { cond, then, els } => {
            rewrite_expr(cond, own, visible);
            rewrite_block(then, own, visible);
            if let Some(e) = els {
                rewrite_block(e, own, visible);
            }
        }
        // 模块限定名 mod.fn 被解析为 Field(Ident(mod), fn)：改写为全局名
        ExprKind::Field(base, field) => {
            if let ExprKind::Ident(head) = &base.kind {
                if let Some(exports) = visible.get(head) {
                    if let Some(g) = exports.get(field) {
                        let name = g.clone();
                        e.kind = ExprKind::Ident(name);
                        return;
                    }
                }
            }
            rewrite_expr(base, own, visible);
        }
        ExprKind::StructLit(name, fields) => {
            resolve_name(name, own, visible);
            for (_, v) in fields.iter_mut() {
                rewrite_expr(v, own, visible);
            }
        }
        ExprKind::TryBlock { body, catches, fin } => {
            rewrite_block(body, own, visible);
            for ca in catches {
                if let Some(g) = &mut ca.guard { rewrite_expr(g, own, visible); }
                rewrite_block(&mut ca.body, own, visible);
            }
            if let Some(f) = fin { rewrite_block(f, own, visible); }
        }
        ExprKind::Match { subject, arms } => {
            rewrite_expr(subject, own, visible);
            for arm in arms.iter_mut() {
                if let Some(p) = arm.pat.as_mut() {
                    rewrite_expr(p, own, visible);
                }
                if let Some(g) = arm.guard.as_mut() {
                    rewrite_expr(g, own, visible);
                }
                rewrite_block(&mut arm.body, own, visible);
            }
        }
        ExprKind::Closure { body, .. } => rewrite_expr(body, own, visible),
        ExprKind::CallValue { callee, args } => {
            rewrite_expr(callee, own, visible);
            for a in args.iter_mut() {
                rewrite_expr(a, own, visible);
            }
        }
        ExprKind::ClosureNew { captures, .. } => {
            for c in captures.iter_mut() {
                rewrite_expr(c, own, visible);
            }
        }
        ExprKind::Borrow { inner, .. } => rewrite_expr(inner, own, visible),
        ExprKind::Ok(inner) | ExprKind::Err(inner) | ExprKind::Some(inner) | ExprKind::Try(inner) => rewrite_expr(inner, own, visible),
        ExprKind::None => {}
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) => {}
    }
}

/// 把一个引用名解析为全局名：
///   - `alias.name`（含 `alias.name.field`）→ 查 visible
///   - 裸名 `name` → 本模块 own 表
fn resolve_name(name: &mut String, own: &HashMap<String, String>, visible: &HashMap<String, HashMap<String, String>>) {
    if let Some(dot) = name.find('.') {
        let (head, rest) = name.split_at(dot);
        let rest = &rest[1..];
        if let Some(exports) = visible.get(head) {
            if let Some(g) = exports.get(rest) {
                *name = g.clone();
            }
        }
        return;
    }
    if let Some(g) = own.get(name.as_str()) {
        *name = g.clone();
    }
}

/// 从 start 逐级向上收集 `lib/` 目录作为模块搜索根（不再有 `lib/std` 特殊层级）。
fn push_lib_std(roots: &mut Vec<PathBuf>, start: &Path) {
    let mut dir = Some(start.to_path_buf());
    for _ in 0..8 {
        let d = match dir {
            Some(d) => d,
            None => break,
        };
        let lib = d.join("lib");
        if lib.is_dir() {
            roots.push(lib);
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "mod".into())
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// 别名清洗：只留标识符安全字符，其余换成 `_`
fn sanitize(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        let ok = ch == '_' || ch.is_alphanumeric() || (ch as u32) > 0x7F;
        if ok && !(i == 0 && ch.is_ascii_digit()) {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("mod");
    }
    out
}





fn prefix_struct_ty(t: &mut Ty, prefix: &str) {
    match t {
        Ty::Struct(n) | Ty::Enum(n) | Ty::Dyn(n) => *n = format!("{}{}", prefix, n),
        Ty::List(e) | Ty::Set(e) | Ty::Option(e) | Ty::Ref(e) | Ty::RefMut(e) => prefix_struct_ty(e, prefix),
        Ty::Map(k, v) => { prefix_struct_ty(k, prefix); prefix_struct_ty(v, prefix); }
        Ty::Array(e, _) => prefix_struct_ty(e, prefix),
        Ty::Result(a, b) => { prefix_struct_ty(a, prefix); prefix_struct_ty(b, prefix); }
        Ty::Tuple(ts) => for x in ts { prefix_struct_ty(x, prefix); },
        Ty::Closure(ps, r) => { for p in ps { prefix_struct_ty(p, prefix); } prefix_struct_ty(r, prefix); }
        _ => {}
    }
}

fn rewrite_struct_ty(t: &mut Ty, old: &str, new: &str) {
    match t {
        Ty::Struct(n) | Ty::Enum(n) | Ty::Dyn(n) => { if n == old { *n = new.to_string(); } }
        Ty::List(e) | Ty::Set(e) | Ty::Option(e) | Ty::Ref(e) | Ty::RefMut(e) => rewrite_struct_ty(e, old, new),
        Ty::Map(k, v) => { rewrite_struct_ty(k, old, new); rewrite_struct_ty(v, old, new); }
        Ty::Array(e, _) => rewrite_struct_ty(e, old, new),
        Ty::Result(a, b) => { rewrite_struct_ty(a, old, new); rewrite_struct_ty(b, old, new); }
        Ty::Tuple(ts) => for x in ts { rewrite_struct_ty(x, old, new); },
        Ty::Closure(ps, r) => { for p in ps { rewrite_struct_ty(p, old, new); } rewrite_struct_ty(r, old, new); }
        _ => {}
    }
}
