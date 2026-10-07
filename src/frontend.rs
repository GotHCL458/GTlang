//! 前端入口：读源 → 模块链接 → 提升/单态化 → 语义分析 → `Unit`。

use std::path::{Path, PathBuf};

use crate::ast::Program;
use crate::diag::{span_of_line, Diag, Stage};
use crate::unit::Unit;
use crate::{encoding, hoist, module, mono, parser, sema};

/// 读取并解码一个源文件：UTF-8 优先，其它编码按系统代码页转换。
///
/// 返回 `(文本, 编码说明)`，编码说明为 `None` 表示就是 UTF-8。
pub fn read_source(path: &Path) -> Result<(String, Option<String>), String> {
    encoding::read_source(path)
}

/// 把多个源文件按顺序拼接成一个编译单元（返回拼接文本与编码提示）
pub fn load_sources(paths: &[PathBuf]) -> Result<(String, Vec<String>), String> {
    let mut text = String::new();
    let mut notes = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        let (t, note) = encoding::read_source(p)?;
        if let Some(n) = note {
            notes.push(format!("{}：{}", p.display(), n));
        }
        if i > 0 {
            text.push('\n');
        }
        text.push_str(&t);
    }
    Ok((text, notes))
}

/// 把相对路径按当前工作目录补全为绝对路径（不做规范化，仅拼接）
pub fn abs_of(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}

/// 收集 impl / trait impl 的方法表：类型名 → 方法名列表。
/// 供方法调用降级（`obj.方法()` → `类型__方法(obj, ...)`）使用。
fn collect_methods(prog: &Program) -> std::collections::HashMap<String, Vec<String>> {
    let mut methods: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for item in &prog.items {
        match item {
            crate::ast::Item::Impl { ty, methods: ms, .. } => {
                methods.entry(ty.clone()).or_default().extend(ms.iter().map(|m| m.name.clone()));
            }
            crate::ast::Item::TraitImpl { ty, methods: ms, .. } => {
                methods.entry(ty.clone()).or_default().extend(ms.iter().map(|m| m.name.clone()));
            }
            _ => {}
        }
    }
    // hoist 之后：trait 默认方法已展平为 `类型__方法` 形式的顶层 Fn，
    // 这里把这些也登记进方法表，使 `obj.方法()` 能降级。
    if methods.is_empty() {
        for item in &prog.items {
            if let crate::ast::Item::Fn(f) = item {
                if let Some((ty, m)) = f.name.split_once("__") {
                    methods.entry(ty.to_string()).or_default().push(m.to_string());
                }
            }
        }
    }
    methods
}

/// 对已解析的 `Program` 做完整的中间处理流水线：
/// 收集方法表 → hoist → 第一遍 sema → 方法调用降级 → 单态化。
fn lower(prog: &mut Program) -> Vec<String> {
    expand_derives(prog);
    let mut methods = collect_methods(prog);
    hoist::hoist(prog);
    // hoist 后：trait 默认方法已展平为 `类型__方法`，补登记方法表
    for item in &prog.items {
        if let crate::ast::Item::Fn(f) = item {
            if let Some((ty, m)) = f.name.split_once("__") {
                let e = methods.entry(ty.to_string()).or_default();
                if !e.contains(&m.to_string()) { e.push(m.to_string()); }
            }
        }
    }
    // 命名参数：CallNamed → Call（在 sema 前，使 arity 检查通过）
    crate::opt::resolve_named(prog);
    // 默认参数：调用点补全（在 sema 前，使 arity 检查通过）
    crate::opt::apply_defaults(prog);
    // 列表推导：ListComp -> 循环 + push
    crate::opt::expand_list_comp(prog);
    // 宏展开（在 sema 前）
    crate::opt::expand_macros(prog);
    // 编译期求值：comptime { ... } -> 字面量（在 sema 前）
    crate::opt::expand_comptime(prog);
    // 第一遍 sema：填充调用点实参类型（单态化 / 方法降级需要）
    let _ = sema::analyze(prog);
    mono::lower_method_calls(prog, &methods);
    let errs = mono::monomorphize(prog);
    // 优化第一段：内联 + 常量折叠（sema(2) 会重推断类型）
    crate::opt::inline_and_fold(prog);
    errs
}

/// 统一前端：源码文本 → 统一 AST（含词法、语法、类型检查）
///
/// 失败时返回**全部**诊断（语法阶段最多一条，类型阶段可多条）。
pub fn build(file: &str, text: &str) -> Result<Unit, Vec<Diag>> {
    let mut prog = match parser::parse_program_multi(text) {
        Ok(p) => p,
        Err(errs) => {
            return Err(errs
                .into_iter()
                .map(|e| Diag {
                    stage: Stage::Parse,
                    file: file.to_string(),
                    message: e.msg,
                    span: e.span,
                    notes: Vec::new(),
                })
                .collect());
        }
    };
    let lower_errs = lower(&mut prog);
    if !lower_errs.is_empty() {
        return Err(lower_errs
            .into_iter()
            .map(|message| Diag {
                stage: Stage::Type,
                file: file.to_string(),
                span: span_of_line(text, &message),
                message,
                notes: Vec::new(),
            })
            .collect());
    }
    match sema::analyze(&mut prog) {
        Ok(analysis) => {
            // 所有权检查（移动语义 / 借用冲突）
            let own_errs = crate::own::check(&prog);
            if !own_errs.is_empty() {
                return Err(own_errs
                    .into_iter()
                    .map(|message| Diag {
                        stage: Stage::Type,
                        file: file.to_string(),
                        span: span_of_line(text, &message),
                        message,
                        notes: Vec::new(),
                    })
                    .collect());
            }
            // 优化第二段：死代码消除（类型检查已过）
            crate::opt::dead_code(&mut prog);
            Ok(Unit {
                ast: prog,
                analysis,
                text: text.to_string(),
                file: file.to_string(),
            })
        }
        Err(errs) => Err(errs
            .into_iter()
            .map(|message| Diag {
                stage: Stage::Type,
                file: file.to_string(),
                span: span_of_line(text, &message),
                message,
                notes: Vec::new(),
            })
            .collect()),
    }
}

/// 直接从文件读取并做前端处理（**含 import 模块解析**）。
///
/// 入口文件的所有 `import` 会被递归解析、去重、环路检测，随后展平为
/// 一份统一 AST 再做语义分析。没有 import 的文件等价于单模块编译。
pub fn build_file(path: &Path) -> Result<Unit, Vec<Diag>> {
    let name = path.to_string_lossy().to_string();
    // 模块链接：递归解析 import，展平成统一 Program
    let mut prog = match module::Linker::new(path).link(path) {
        Ok(p) => p,
        Err(errs) => {
            return Err(errs
                .into_iter()
                .map(|e| Diag {
                    stage: Stage::Lex,
                    file: e.file,
                    message: e.message,
                    span: e.span,
                    notes: Vec::new(),
                })
                .collect())
        }
    };
    let text = encoding::read_source(path).map(|(t, _)| t).unwrap_or_default();
    let lower_errs = lower(&mut prog);
    if !lower_errs.is_empty() {
        return Err(lower_errs
            .into_iter()
            .map(|message| Diag {
                stage: Stage::Type,
                file: name.clone(),
                span: span_of_line(&text, &message),
                message,
                notes: Vec::new(),
            })
            .collect());
    }
    match sema::analyze(&mut prog) {
        Ok(analysis) => {
            let own_errs = crate::own::check(&prog);
            if !own_errs.is_empty() {
                return Err(own_errs
                    .into_iter()
                    .map(|message| Diag {
                        stage: Stage::Type,
                        file: name.clone(),
                        span: span_of_line(&text, &message),
                        message,
                        notes: Vec::new(),
                    })
                    .collect());
            }
            crate::opt::dead_code(&mut prog);
            Ok(Unit {
                ast: prog,
                analysis,
                text: text.clone(),
                file: name,
            })
        }
        Err(errs) => Err(errs
            .into_iter()
            .map(|message| Diag {
                stage: Stage::Type,
                file: name.clone(),
                span: span_of_line(&text, &message),
                message,
                notes: Vec::new(),
            })
            .collect()),
    }
}

/// 展开 `@derive(Eq)`：为结构体生成 `impl 类型 { fn eq(self, o: 类型) -> bool {...} }`。
fn expand_derives(prog: &mut Program) {
    let mut extra: Vec<crate::ast::Item> = Vec::new();
    for item in &prog.items {
        // enum 的 @derive(Debug)：生成 to_str（按变体名输出 "Suit::Hearts"）
        if let crate::ast::Item::Enum(en) = item {
            let ty = en.name.clone();
            if en.derives.iter().any(|d| d == "Debug") && !en.variants.is_empty() {
                let mut arms: Vec<String> = Vec::new();
                for (v, payload) in en.variants.iter() {
                    if payload.is_empty() {
                        arms.push(format!("{}::{} => {{ return \"{}::{}\" }}", ty, v, ty, v));
                    } else {
                        // 有载荷：只输出变体名（简化，不展开载荷）
                        let wild: Vec<String> = payload.iter().map(|_| "_".to_string()).collect();
                        arms.push(format!("{}::{}({}) => {{ return \"{}::{}\" }}", ty, v, wild.join(", "), ty, v));
                    }
                }
                arms.push("_ => { return \"\" }".to_string());
                let wrapped = format!("impl {} {{\n    fn to_str(self) -> str {{ match self {{ {} }} }}\n}}", ty, arms.join(" "));
                if let Ok(p) = crate::parser::parse_program(&wrapped) {
                    for it in p.items { extra.push(it); }
                }
            }
            continue;
        }
        if let crate::ast::Item::Struct(s) = item {
            let ty = s.name.clone();
            let fields: Vec<String> = s.fields.iter().map(|(n, _, _)| n.clone()).collect();
            if fields.is_empty() { continue; }
            let mut methods: Vec<String> = Vec::new();
            // @derive(Eq) -> eq / ne
            if s.derives.iter().any(|d| d == "Eq") {
                let mut parts: Vec<String> = Vec::new();
                for f in &fields { parts.push(format!("self.{} == o.{}", f, f)); }
                let cond = parts.join(" && ");
                let mut parts_ne: Vec<String> = Vec::new();
                for f in &fields { parts_ne.push(format!("self.{} != o.{}", f, f)); }
                let cond_ne = parts_ne.join(" || ");
                methods.push(format!("fn eq(self, o: {}) -> bool {{ {} }}", ty, cond));
                methods.push(format!("fn ne(self, o: {}) -> bool {{ {} }}", ty, cond_ne));
            }
            // @derive(Clone) -> clone：逐字段复制构造新实例
            if s.derives.iter().any(|d| d == "Clone") {
                let mut fs: Vec<String> = Vec::new();
                for f in &fields { fs.push(format!("{}: self.{}", f, f)); }
                methods.push(format!("fn clone(self) -> {} {{ {} {{ {} }} }}", ty, ty, fs.join(", ")));
            }
            // @derive(Debug) -> to_str：用字符串插值生成 "类型名 { f1: v1, f2: v2 }"
            if s.derives.iter().any(|d| d == "Debug") {
                let mut inner: Vec<String> = Vec::new();
                for f in &fields { inner.push(format!("{}: ${{self.{}}}", f, f)); }
                let body = format!("\"{} {{ {} }}\"", ty, inner.join(", "));
                methods.push(format!("fn to_str(self) -> str {{ {} }}", body));
            }
            // @derive(Default) -> default：各字段 0 / 0.0 / false / \"\"（按类型）
            if s.derives.iter().any(|d| d == "Default") {
                let mut fs: Vec<String> = Vec::new();
                for (fname, fty, _) in &s.fields {
                    let dflt = match fty {
                        Some(crate::ast::Ty::F64) => "0.0".to_string(),
                        Some(crate::ast::Ty::Bool) => "false".to_string(),
                        Some(crate::ast::Ty::Str) => "\"\"".to_string(),
                        // 具名类型（struct/enum）字段：递归用其 default()
                        Some(crate::ast::Ty::Struct(n)) => format!("{}.default()", n),
                        _ => "0".to_string(),
                    };
                    fs.push(format!("{}: {}", fname, dflt));
                }
                methods.push(format!("fn default() -> {} {{ {} {{ {} }} }}", ty, ty, fs.join(", ")));
            }
            // @derive(Hash) -> hash：逐字段混合（FNV 风格）。每步与掩码相与，
            // 避免 int 乘法溢出（溢出检测默认开启）。
            if s.derives.iter().any(|d| d == "Hash") {
                // FNV-1a 32 位风格：每步掩码 0xFFFFFFFF（< 2^32），乘 2^24 的常数不溢出 i64。
                let mut expr = "2166136261".to_string();
                for (i, f) in fields.iter().enumerate() {
                    expr = format!("((({} ^ (int(self.{}) + {})) * 16777619) & 4294967295)", expr, f, i);
                }
                methods.push(format!("fn hash(self) -> int {{ return {} & 4294967295 }}", expr));
            }
            // @derive(Ord) -> cmp/lt/le/gt/ge：逐字段字典序 + 比较运算符重载
            if s.derives.iter().any(|d| d == "Ord") {
                let mut body = String::new();
                for f in &fields {
                    body.push_str(&format!("if self.{} < o.{} {{ return -1 }}\n        if self.{} > o.{} {{ return 1 }}\n        ", f, f, f, f));
                }
                body.push_str("return 0");
                methods.push(format!("fn cmp(self, o: {}) -> int {{\n        {}\n    }}", ty, body));
                // lt/le/gt/ge：内联逐字段比较（不能调 self.cmp——hoist 不认 self.方法）
                let mut lt_body = String::new();
                let mut ge_body = String::new();
                for f in &fields {
                    lt_body.push_str(&format!("if self.{} < o.{} {{ return true }}\n        if self.{} > o.{} {{ return false }}\n        ", f, f, f, f));
                    ge_body.push_str(&format!("if self.{} < o.{} {{ return false }}\n        if self.{} > o.{} {{ return true }}\n        ", f, f, f, f));
                }
                let mut le_body = String::new();
                let mut ge2_body = String::new();
                for f in &fields {
                    le_body.push_str(&format!("if self.{} < o.{} {{ return true }}\n        if self.{} > o.{} {{ return false }}\n        ", f, f, f, f));
                    ge2_body.push_str(&format!("if self.{} < o.{} {{ return false }}\n        if self.{} > o.{} {{ return true }}\n        ", f, f, f, f));
                }
                methods.push(format!("fn lt(self, o: {}) -> bool {{\n        {}return false\n    }}", ty, lt_body));
                methods.push(format!("fn gt(self, o: {}) -> bool {{\n        {}return false\n    }}", ty, ge_body));
                methods.push(format!("fn le(self, o: {}) -> bool {{\n        {}return true\n    }}", ty, le_body));
                methods.push(format!("fn ge(self, o: {}) -> bool {{\n        {}return true\n    }}", ty, ge2_body));
            }
            // @derive(PartialEq) -> eq：仅 eq（不带 ne）
            if s.derives.iter().any(|d| d == "PartialEq") {
                let mut parts: Vec<String> = Vec::new();
                for f in &fields { parts.push(format!("self.{} == o.{}", f, f)); }
                methods.push(format!("fn eq(self, o: {}) -> bool {{ {} }}", ty, parts.join(" && ")));
            }
            // @derive(Display) -> to_str：仅字段值（无类型名），如 "1, 2"
            if s.derives.iter().any(|d| d == "Display") {
                let mut inner: Vec<String> = Vec::new();
                for f in &fields { inner.push(format!("${{self.{}}}", f)); }
                let body = format!("\"{}\"", inner.join(", "));
                methods.push(format!("fn to_str(self) -> str {{ {} }}", body));
            }
            if methods.is_empty() { continue; }
            let wrapped = format!("impl {} {{\n    {}\n}}", ty, methods.join("\n    "));
            if crate::verbose() {
                eprintln!("[derive] wrapped:\n{}", wrapped);
            }
            match crate::parser::parse_program(&wrapped) {
                Ok(p) => for it in p.items { extra.push(it); },
                Err(e) => {
                    if crate::verbose() { eprintln!("[derive] parse err: {}", e.msg); }
                }
            }
        }
    }
    prog.items.extend(extra);
}


