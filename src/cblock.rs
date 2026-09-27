//! 内联 C 块：`C { ... }` 的提取与函数签名解析。
//!
//! # 为什么需要预扫描
//!
//! C 代码里充满 GTLang 词法器无法处理的字符：`#include <stdio.h>` 的 `#` 在 GTLang 里
//! 是行注释起始、宏续行 `\` 与 `'` 会触发"无法识别的字符"、裸引号会让字符串扫描跑飞。
//! 因此**不能让 GTLang 词法器看到 C 块内部**。
//!
//! 做法是在词法分析之前先做一次字符级扫描：
//!   1. 找到行首（前面只有空白）的 `C` + `{`；
//!   2. 按 C 的规则做括号配平（跳过 C 字符串 / 字符字面量 / 注释）；
//!   3. 把整块内容取出，并在**原位置上用等长空白替换**（换行保留）。
//!
//! 等长替换是关键：所有 token 的字节 span 与行号完全不变，诊断依旧精确。
//!
//! # 自动注册
//!
//! 从 C 块里解析出所有函数定义（名字 / 返回类型 / 参数类型），
//! 使 GTLang 可以直接调用它们，无需手写 `extern` 声明。

use crate::types::Ty;

/// 从 C 块里解析出的一个函数签名
#[derive(Debug, Clone)]
pub struct CFn {
    pub name: String,
    pub params: Vec<Ty>,
    pub ret: Ty,
    /// 函数头在该段 C 源码里的字符区间 `(起点, '(' 位置)`。
    /// 仅用于 `externize` 定位 `static`/`inline`，不参与类型系统。
    pub head: (usize, usize),
}

/// 把顶层函数定义前的 `static` / `inline` 去掉（等长空白替换），使其具备**外部链接**。
///
/// 这是必要的：GTLang 调用 C 函数时链接器需要看到该符号，而 `static` 是内部链接，
/// 会直接导致 `undefined symbol`。用户出于习惯写 `static`（避免符号污染）是合理的，
/// 所以由编译器自动剥离，而不是要求用户别写。
///
/// 只处理**函数头**范围，函数体内的 `static` 局部变量不受影响。
pub fn externize(code: &str) -> String {
    let funcs = parse_c_funcs(code);
    if funcs.is_empty() {
        return code.to_string();
    }
    let mut cs: Vec<char> = code.chars().collect();
    for f in &funcs {
        let (s, e) = f.head;
        blank_word(&mut cs, s, e, "static");
        blank_word(&mut cs, s, e, "inline");
    }
    cs.into_iter().collect()
}

/// 在 `cs[s..e]` 范围内把整词 `word` 替换为等长空格
fn blank_word(cs: &mut [char], s: usize, e: usize, word: &str) {
    let w: Vec<char> = word.chars().collect();
    if w.is_empty() {
        return;
    }
    let mut i = s;
    while i + w.len() <= e && i + w.len() <= cs.len() {
        if cs[i..i + w.len()] == w[..] {
            let before_ok = i == 0 || !is_word_char(cs[i - 1]);
            let after_ok = i + w.len() >= cs.len() || !is_word_char(cs[i + w.len()]);
            if before_ok && after_ok {
                for k in i..i + w.len() {
                    cs[k] = ' ';
                }
                i += w.len();
                continue;
            }
        }
        i += 1;
    }
}

/// 一个可被 C 调用的 GTLang 函数签名（用于生成桥接）
#[derive(Debug, Clone)]
pub struct GtFn {
    pub name: String,
    pub params: Vec<Ty>,
    pub ret: Ty,
}

/// GTLang 类型 → C 类型名
pub fn ty_to_c(t: &Ty) -> &'static str {
    match t {
        Ty::Void => "void",
        Ty::Bool => "long long",
        Ty::F64 => "double",
        Ty::Str => "const char *",
        // 数组在 C 侧按指针传递（元素为 8 字节槽）
        Ty::Array(..) => "void *",
        _ => "long long",
    }
}

/// 生成 C → GTLang 的调用桥。
///
/// 思路：C 侧看到的是**普通函数**，函数体里通过一个函数指针槽间接调用真正的
/// GTLang 代码；槽的值由编译器/解释器在启动时填入。这样：
///   - 签名转换完全交给 C 编译器，不需要运行时生成代码（无栈/ABI 兼容问题）；
///   - C 代码直接写 GTLang 函数名即可调用，无需手写 `extern` 声明。
///
/// 返回 `(桥接源码, 槽名列表)`，槽名按顺序对应 `funcs`。
pub fn bridge_header(funcs: &[GtFn]) -> (String, Vec<String>) {
    let mut out = String::new();
    let mut slots = Vec::new();
    if funcs.is_empty() {
        return (out, slots);
    }
    out.push_str("/* ===== 自动生成：C 调用 GTLang 的桥接（请勿修改） ===== */\n");
    // 禁用 stdout 缓冲：解释器端 C 代码里的 printf 要立刻落控制台，
    // 否则会因 CRT 全缓冲而与 GTLang 的直接输出混序。
    // 注意不能加 static——解释器需要用 tcc_get_symbol 取到它并主动调用。
    out.push_str(
        "#include <stdio.h>\nvoid gt_flush_iob(void) { setvbuf(stdout, NULL, _IONBF, 0); }\n",
    );
    for (i, f) in funcs.iter().enumerate() {
        let _ = f;
        let slot = format!("gt_slot_{}", i);
        // 槽必须是**定义**（非 static，好让编译器侧用 external global 引用）
        out.push_str(&format!("void *{} = 0;\n", slot));
        slots.push(slot);
    }
    out.push('\n');
    for (i, f) in funcs.iter().enumerate() {
        let ret = ty_to_c(&f.ret);
        let params: Vec<String> = f
            .params
            .iter()
            .enumerate()
            .map(|(k, t)| format!("{} a{}", ty_to_c(t), k + 1))
            .collect();
        let sig = params.join(", ");
        // 函数指针类型：参数类型列表
        let ptypes = f
            .params
            .iter()
            .map(ty_to_c)
            .collect::<Vec<_>>()
            .join(", ");
        let call_args = (0..f.params.len())
            .map(|k| format!("a{}", k + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let slot = format!("gt_slot_{}", i);
        // 注意：不能用 `static`——桥接头与用户 C 块是**两个编译单元**
        // （分片编译才能让 C 块报错行号从 1 开始），static 函数跨单元不可见。
        out.push_str(&format!(
            "{ret} {name}({sig}) {{\n    \
             {prefix}(({ret} (*)({ptypes})){slot})({call_args});\n}}\n",
            ret = ret,
            name = f.name,
            sig = sig,
            prefix = if f.ret == Ty::Void { "" } else { "return " },
            ptypes = ptypes,
            slot = slot,
            call_args = call_args,
        ));
    }
    out.push('\n');
    (out, slots)
}

/// 从已解析的程序里取出所有可被 C 调用的 GTLang 函数。
///
/// 排除 `main`：桥接会为每个函数生成同名 C 包装，而 `main` 会与 C 的入口重名。
pub fn gt_functions(items: &[crate::ast::Item]) -> Vec<GtFn> {
    let mut out = Vec::new();
    for it in items {
        if let crate::ast::Item::Fn(f) = it {
            if f.name == "main" {
                continue;
            }
            out.push(GtFn {
                name: f.name.clone(),
                params: f
                    .params
                    .iter()
                    .map(|p| p.ty.clone().unwrap_or(Ty::I64))
                    .collect(),
                ret: f.ret_ty.clone(),
            });
        }
    }
    out
}

/// 内联 C 块的提取结果
#[derive(Debug, Default, Clone)]
pub struct CBlock {
    /// 所有 C 块拼接后的源码（按出现顺序，已剥离 static/inline）
    pub code: String,
    /// 解析出的可调用函数
    pub funcs: Vec<CFn>,
}

impl CBlock {
    pub fn is_empty(&self) -> bool {
        self.code.trim().is_empty()
    }
}

/// 扫描源码，抽出所有 `C { ... }` 块，并把它们从源码中"挖空"。
///
/// 返回 `(挖空后的源码, C 块信息)`；挖空后的源码可以直接交给词法/语法分析。
pub fn extract(src: &str) -> Result<(String, CBlock), String> {
    let chars: Vec<char> = src.chars().collect();
    // 逐字节偏移表：chars[i] 的起始字节
    let mut off = Vec::with_capacity(chars.len() + 1);
    let mut b = 0usize;
    for ch in &chars {
        off.push(b);
        b += ch.len_utf8();
    }
    off.push(b);

    let mut blank = chars.clone(); // 用于挖空
    let mut out = CBlock::default();
    let mut i = 0usize;

    while i < chars.len() {
        // 只在「行首（前面只有空白）」识别，避免误伤字符串/表达式里的 `C`
        if !at_line_start(&chars, i) {
            i += 1;
            continue;
        }
        // 需要 `C` 后跟（可跨空白）`{`，且 `C` 是独立单词
        if chars[i] != 'C' {
            i += 1;
            continue;
        }
        if i > 0 && is_word_char(chars[i - 1]) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
            j += 1;
        }
        if j >= chars.len() || chars[j] != '{' {
            i += 1;
            continue;
        }

        // 从 `{` 开始按 C 规则配平
        let body_start = j + 1;
        let close = match match_c_brace(&chars, j) {
            Some(c) => c,
            None => {
                return Err(crate::lb!(
                    line_of(&chars, i),
                    "unterminated inline C block 'C {{'",
                    "内联 C 块 'C {{' 未闭合"
                ))
            }
        };

        // 取原文（body_start..close）
        let raw: String = chars[body_start..close].iter().collect();
        let funcs = parse_c_funcs(&raw);
        // 剥离 static/inline，使这些函数具备外部链接，GTLang 才能链接到
        let code = externize(&raw);
        out.funcs.extend(funcs);
        if !out.code.is_empty() {
            out.code.push('\n');
        }
        out.code.push_str(&code);
        out.code.push('\n');

        // 等长挖空：非换行字符替换为空格，换行保留（行号与字节 span 都不变）
        for k in i..=close {
            if blank[k] != '\n' {
                blank[k] = ' ';
            }
        }
        i = close + 1;
    }

    let _ = off; // 偏移表仅用于诊断，暂不需要
    Ok((blank.into_iter().collect(), out))
}

/// `chars[i]` 是否位于行首（前面只有空白）
fn at_line_start(chars: &[char], i: usize) -> bool {
    let mut k = i;
    while k > 0 {
        let c = chars[k - 1];
        if c == '\n' || c == '\r' {
            return true;
        }
        if c == ' ' || c == '\t' {
            k -= 1;
            continue;
        }
        return false;
    }
    true
}

fn line_of(chars: &[char], i: usize) -> usize {
    1 + chars[..i].iter().filter(|c| **c == '\n').count()
}

fn is_word_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric() || (c as u32) > 0x7F
}

/// 从 `chars[open]`（必须是 `{`）出发找到匹配的 `}`，跳过 C 字符串/字符/注释。
fn match_c_brace(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = open;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' | '\'' => {
                let quote = c;
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\\' {
                        i += 2;
                        continue;
                    }
                    if chars[i] == quote {
                        break;
                    }
                    i += 1;
                }
            }
            '/' if i + 1 < chars.len() && chars[i + 1] == '/' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '/' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 1;
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

// ============================================================
// C 函数签名解析
// ============================================================

/// 解析 C 源码里所有**函数定义**（带 `{` 体的），提取名字/返回类型/参数类型。
///
/// 只认顶层（大括号深度 0）的定义；跳过预处理行、注释、字符串。
/// `struct Foo { ... }` 这类没有 `(` 的声明会被自然跳过。
///
/// 关键点：函数头文本必须在**剔除注释与字符串之后**再解析。
/// 否则 `/* 调用 累加() */ static long long 用累加(...)` 里的注释会污染
/// `find('(')`，把函数名解析成注释里提到的那个名字。
pub fn parse_c_funcs(code: &str) -> Vec<CFn> {
    parse_c_funcs_ex(code, false)
}

/// `parse_c_funcs` 的扩展版：`want_decls` 为真时也解析 `;` 结尾的**函数声明**。
/// C 块用 `false`（只要定义），`.h` 导入用 `true`（声明也是接口）。
pub fn parse_c_funcs_ex(code: &str, want_decls: bool) -> Vec<CFn> {
    let chars: Vec<char> = code.chars().collect();
    let mut out = Vec::new();
    let mut depth = 0i32;
    // 语句起点（原文索引，用于 externize 定位 static/inline）
    let mut stmt_start = 0usize;
    // 已剔除注释/字符串的头部文本
    let mut head = String::new();
    let mut i = 0usize;

    while i < chars.len() {
        let c = chars[i];
        match c {
            '#' if at_line_start(&chars, i) => {
                // 预处理指令整行跳过
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                stmt_start = i;
                head.clear();
                continue;
            }
            '"' | '\'' => {
                // 字符串/字符字面量：内容不进 head（函数头里不该有它们）
                let quote = c;
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\\' {
                        i += 2;
                        continue;
                    }
                    if chars[i] == quote {
                        break;
                    }
                    i += 1;
                }
            }
            '/' if i + 1 < chars.len() && chars[i + 1] == '/' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '/' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 1;
            }
            '{' => {
                if depth == 0 {
                    // 深度 0 的 `{` 之前是函数头候选
                    if let Some(f) = parse_c_head(&head, (stmt_start, i)) {
                        out.push(f);
                    }
                    // 函数体整体跳过（里面的 `{` 不参与计数）
                    let close = match match_c_brace(&chars, i) {
                        Some(c) => c,
                        None => break,
                    };
                    i = close;
                    stmt_start = i + 1;
                    head.clear();
                } else {
                    depth += 1;
                }
            }
            ';' if depth == 0 => {
                // 函数**声明**（头文件里以 ';' 结尾）：仅 `.h` 导入时解析
                if want_decls {
                    if let Some(f) = parse_c_head(&head, (stmt_start, i)) {
                        out.push(f);
                    }
                }
                stmt_start = i + 1;
                head.clear();
            }
            _ => head.push(c),
        }
        i += 1;
    }
    out
}

/// 解析形如 `static const char *gt_tag(long long x, const char *s)` 的函数头。
/// 解析不出来（不是函数）时返回 `None`。
///
/// `head_span` 是该函数头在**原文**里的字符区间（用于 `externize` 定位
/// `static`/`inline`），与传入的 `head` 文本可能因剔除注释而不同。
fn parse_c_head(head: &str, head_span: (usize, usize)) -> Option<CFn> {
    let open = head.find('(')?;
    let close = head.rfind(')')?;
    if close < open {
        return None;
    }
    let before = head[..open].trim();
    let params_src = &head[open + 1..close];

    // `before` 形如 `static const char *打招呼`：末尾的标识符才是函数名。
    // 注意 `*` 常与函数名粘连（`char *f` / `char *f`），所以不能简单按空白切词。
    let (name, ret_src) = split_c_name(before)?;
    // 关键字不能当函数名（说明是 `if (...)` / `while (...)` 之类）
    if matches!(name.as_str(), "if" | "while" | "for" | "switch" | "return" | "sizeof") {
        return None;
    }
    if ret_src.trim().is_empty() {
        return None;
    }

    let ret = c_type_to_ty(&ret_src);
    let params = split_c_params(params_src)
        .iter()
        .filter_map(|p| {
            let t = p.trim();
            if t.is_empty() || t == "void" {
                return None;
            }
            Some(c_type_to_ty(t))
        })
        .collect();

    Some(CFn { name, params, ret, head: head_span })
}

/// 把 `static const char *打招呼` 拆成 `("打招呼", "static const char *")`。
/// 从末尾跳过非标识符字符（空白、`*`），再往前吃标识符字符作为名字。
fn split_c_name(before: &str) -> Option<(String, String)> {
    let cs: Vec<char> = before.chars().collect();
    let mut e = cs.len();
    while e > 0 && !is_word_char(cs[e - 1]) {
        e -= 1;
    }
    if e == 0 {
        return None;
    }
    let name_end = e;
    while e > 0 && is_word_char(cs[e - 1]) {
        e -= 1;
    }
    let name: String = cs[e..name_end].iter().collect();
    if name.is_empty() {
        return None;
    }
    let ret: String = cs[..e].iter().collect();
    Some((name, ret))
}

/// 按顶层逗号切分 C 参数列表（跳过括号与指针里的逗号）
fn split_c_params(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '(' | '[' => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// C 类型文本 → GTLang 类型。
///
/// 映射规则（与 `type.rs` 的整型归一策略一致，所有整数都是 I64）：
/// - `void` → `Void`
/// - `bool` / `_Bool` → `Bool`
/// - `float` / `double` → `F64`
/// - `char *` / `const char *` → `Str`（C 字符串与 GTLang 字符串同为 NUL 结尾字节串）
/// - 其余整数与指针 → `I64`
pub fn c_type_to_ty(s: &str) -> Ty {
    let t = s.trim();
    if t.is_empty() {
        return Ty::I64;
    }
    let is_ptr = t.contains('*');
    // 去掉指针符号与数组后缀后看基类型
    let base: String = t
        .replace('*', " ")
        .replace(['[', ']'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if is_ptr {
        // 字符指针按字符串处理，其余指针当整数句柄传递
        return if base.contains("char") { Ty::Str } else { Ty::I64 };
    }
    if base.contains("void") {
        return Ty::Void;
    }
    if base.contains("bool") {
        return Ty::Bool;
    }
    if base.contains("double") || base.contains("float") {
        return Ty::F64;
    }
    Ty::I64
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_c_block_and_blank_fills_it() {
        let src = "C {\n    int x = 1;\n}\nfn main() { put(\"hi\") }\n";
        let (blanked, cb) = extract(src).unwrap();
        // 原文长度不变（保证 span 与行号稳定）
        assert_eq!(blanked.len(), src.len());
        assert_eq!(blanked.lines().count(), src.lines().count());
        assert!(blanked.contains("fn main()"));
        assert!(!blanked.contains("int x"));
        assert!(cb.code.contains("int x = 1;"));
    }

    #[test]
    fn parses_function_signatures() {
        let code = r#"
#include <math.h>
static long long 双倍(long long x) { return x * 2; }
static double hypot2(double a, double b) { return sqrt(a*a + b*b); }
static const char *tag(long long x) { return "t"; }
static void noop(void) { }
"#;
        let fns = parse_c_funcs(code);
        let names: Vec<&str> = fns.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["双倍", "hypot2", "tag", "noop"]);
        assert_eq!(fns[0].ret, Ty::I64);
        assert_eq!(fns[0].params, vec![Ty::I64]);
        assert_eq!(fns[1].ret, Ty::F64);
        assert_eq!(fns[1].params, vec![Ty::F64, Ty::F64]);
        assert_eq!(fns[2].ret, Ty::Str);
        assert_eq!(fns[3].ret, Ty::Void);
        assert!(fns[3].params.is_empty());
    }

    #[test]
    fn ignores_structs_and_control_flow() {
        let code = r#"
struct P { int x; int y; };
int add(int a, int b) { if (a) { return a + b; } return b; }
"#;
        let fns = parse_c_funcs(code);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "add");
        assert_eq!(fns[0].params, vec![Ty::I64, Ty::I64]);
    }

    #[test]
    fn c_string_braces_do_not_confuse_matching() {
        let src = "C {\n    const char *s = \"} not the end {\";\n}\nfn main() { }\n";
        let (blanked, cb) = extract(src).unwrap();
        assert!(cb.code.contains("not the end"));
        assert!(blanked.contains("fn main()"));
    }

    #[test]
    fn pointer_params_map_correctly() {
        assert_eq!(c_type_to_ty("const char *"), Ty::Str);
        assert_eq!(c_type_to_ty("char*"), Ty::Str);
        assert_eq!(c_type_to_ty("int *"), Ty::I64);
        assert_eq!(c_type_to_ty("unsigned long long"), Ty::I64);
        assert_eq!(c_type_to_ty("double"), Ty::F64);
        assert_eq!(c_type_to_ty("void"), Ty::Void);
    }

    #[test]
    fn comment_and_call_inside_body_are_not_functions() {
        // C 块里"调用 GTLang 函数"的语句、以及提到函数名的注释，
        // 都不能被当成 C 函数定义（否则会误报重名）。
        let code = r#"
    /* 调用 GTLang 的 累加()：桥接头已自动生成同名 C 函数 */
    static long long 用累加(long long n) {
        return 累加(n) * 10;   // 这里的 累加() 是调用，不是定义
    }
    static void 触发报告(const char *name) {
        报告(name);
    }
"#;
        let fns = parse_c_funcs(code);
        let names: Vec<&str> = fns.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["用累加", "触发报告"], "误把调用/注释识别成了定义");
    }
}