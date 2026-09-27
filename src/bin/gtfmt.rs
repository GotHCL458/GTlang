//! gtfmt —— GTLang 代码格式化器（高质量：缩进 + 空格规范化 + 空行策略）。
//!
//! 用法：
//!   gtfmt <文件.gt>           就地格式化
//!   gtfmt --check <文件.gt>   只检查是否需要格式化（退出码 1 表示需要）
//!
//! 格式化前会先经 gtc 完整检查（词法/语法/类型），有错则拒绝并报出全部错误。

use std::path::PathBuf;
use std::process::ExitCode;

const HELP_ZH: &str = "gtfmt —— GTLang 代码格式化器\n\n用法：\n  gtfmt <文件.gt> [...]           就地格式化\n  gtfmt --check <文件.gt> [...]   只检查（退出码 1 表示需要格式化）\n  gtfmt -h | --help               显示本帮助\n  gtfmt ... zh                    使用中文";
const HELP_EN: &str = "gtfmt — GTLang code formatter\n\nUsage:\n  gtfmt <file.gt> [...]           format in place\n  gtfmt --check <file.gt> [...]   check only (exit 1 if formatting needed)\n  gtfmt -h | --help               show this help\n  gtfmt ... zh                    use Chinese";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let zh = args.last().map(|s| s == "zh").unwrap_or(false);
    let mut check = false;
    let mut files: Vec<PathBuf> = Vec::new();
    for a in &args {
        match a.as_str() {
            "--check" => check = true,
            "-h" | "--help" => {
                println!("{}", if zh { HELP_ZH } else { HELP_EN });
                return ExitCode::SUCCESS;
            }
            s if s.starts_with('-') => {
                eprintln!("{}", if zh { format!("未知选项：{}", s) } else { format!("unknown option: {}", s) });
                return ExitCode::from(1);
            }
            _ => { if a.as_str() != "zh" { files.push(PathBuf::from(a)); } }
        }
    }
    if files.is_empty() {
        println!("{}", if zh { HELP_ZH } else { HELP_EN });
        return ExitCode::from(1);
    }
    let mut changed = 0;
    for f in &files {
        match format_file(f, check) {
            Ok(diff) => { if diff { changed += 1; } }
            Err(e) => {
                eprintln!("[gtfmt] {}：{}", f.display(), e);
                return ExitCode::from(1);
            }
        }
    }
    if check && changed > 0 {
        eprintln!("[gtfmt] {} 个文件需要格式化", changed);
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn format_file(path: &std::path::Path, check: bool) -> Result<bool, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("无法读取：{}", e))?;
    // 先做完整检查（词法/语法/类型），有错则拒绝格式化
    gtc_rust::build_file(path).map_err(|diags| {
        let mut msg = String::new();
        for d in &diags {
            msg.push_str(&format!("  {}\n", d.message));
        }
        msg.push_str(&format!("共 {} 个错误（请先修复再格式化）", diags.len()));
        msg
    })?;
    let formatted = format_source(&text);
    if formatted == text {
        return Ok(false);
    }
    if check {
        return Ok(true);
    }
    std::fs::write(path, &formatted).map_err(|e| format!("无法写入：{}", e))?;
    Ok(true)
}
/// 高质量格式化：缩进规范化 + 运算符/逗号/冒号空格 + 空行策略。
///
/// 兼容性：跨行字符串、字符串插值花括号平衡、r"..." 原始串、# 与 // 注释。
fn format_source(src: &str) -> String {
    let lines = split_lines_aware(src);
    let mut out = String::with_capacity(src.len() + 64);
    let mut depth: i32 = 0;
    let mut blank = false;
    for li in &lines {
        let trimmed = li.text.trim();
        if trimmed.is_empty() {
            if !blank { out.push('\n'); blank = true; }
            continue;
        }
        blank = false;
        let starts_close = trimmed.starts_with('}');
        let eff = if starts_close { (depth - 1).max(0) } else { depth.max(0) };
        for _ in 0..eff { out.push_str("    "); }
        if li.in_string || trimmed.starts_with("//") || trimmed.starts_with('#') {
            out.push_str(trimmed);
        } else {
            out.push_str(&normalize_line(trimmed));
        }
        out.push('\n');
        if !li.trailing_string {
            let (opens, closes) = count_braces_aware(trimmed);
            depth += opens - closes;
            if depth < 0 { depth = 0; }
        }
    }
    while out.ends_with("\n\n") { out.pop(); }
    if !out.ends_with('\n') { out.push('\n'); }
    out
}

struct FmtLine {
    text: String,
    in_string: bool,
    trailing_string: bool,
}

/// 按行切分，跟踪跨行字符串状态。
fn split_lines_aware(src: &str) -> Vec<FmtLine> {
    let mut out = Vec::new();
    let mut in_string = false;
    for raw in src.lines() {
        let text = raw.to_string();
        // 扫描本行，更新 in_string（处理转义与 """ 不可见）
        let mut chars = text.chars().peekable();
        let start_in = in_string;
        let mut esc = false;
        while let Some(c) = chars.next() {
            if esc { esc = false; continue; }
            if in_string {
                if c == '\\' { esc = true; continue; }
                if c == '"' { in_string = false; }
            } else {
                if c == '"' { in_string = true; }
                else if c == '#' || (c == '/' && chars.peek() == Some(&'/')) {
                    // 行注释：其后不再有字符串开始
                    break;
                }
            }
        }
        out.push(FmtLine { text, in_string: start_in, trailing_string: in_string });
    }
    out
}

/// 行内空白规范化：运算符两侧 1 空格，逗号后 1 空格，冒号后 1 空格（:: 除外）。
/// 不改字符串/注释内部（调用方已保证只在代码行上调用，且本函数会跳过字符串段）。
fn normalize_line(line: &str) -> String {
    // 把行按"字符串内/外"分段，只在外部做空白规范化
    let mut out = String::with_capacity(line.len() + 8);
    let mut seg = String::new();
    let mut in_str = false;
    let mut esc = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_str {
            seg.push(c);
            if esc { esc = false; continue; }
            if c == '\\' { esc = true; continue; }
            if c == '"' {
                in_str = false;
                out.push_str(&normalize_code_segment(&seg));
                seg.clear();
            }
            continue;
        }
        if c == '"' {
            if !seg.is_empty() {
                out.push_str(&normalize_code_segment(&seg));
                seg.clear();
            }
            in_str = true;
            seg.push(c);
            continue;
        }
        seg.push(c);
    }
    if !seg.is_empty() {
        out.push_str(&normalize_code_segment(&seg));
    }
    out
}

/// 对一段"代码"（不含字符串）做空格规范化。
fn normalize_code_segment(seg: &str) -> String {
    if seg.is_empty() { return String::new(); }
    let s = seg.trim();
    // 注释：原样保留
    if s.starts_with("//") || s.starts_with('#') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        // 多字符运算符
        if i + 1 < n {
            let two: String = chars[i..i+2].iter().collect();
            // `..` 范围：两侧不加空格
            if two == ".." {
                trim_trailing_spaces(&mut out);
                out.push_str("..");
                i += 2;
                while i < n && chars[i] == ' ' { i += 1; }
                continue;
            }
            if matches!(two.as_str(), "==" | "!=" | "<=" | ">=" | "&&" | "||" | "+=" | "-=" | "*=" | "/=" | "%=" | "->" | "=>" | ":=") {
                trim_trailing_spaces(&mut out);
                if !out.is_empty() && !out.ends_with(' ') { out.push(' '); }
                out.push_str(&two);
                out.push(' ');
                i += 2;
                while i < n && chars[i] == ' ' { i += 1; }
                continue;
            }
        }
        if c == ':' && !(i + 1 < n && chars[i+1] == ':') && !(i > 0 && chars[i-1] == ':') {
            trim_trailing_spaces(&mut out);
            out.push(':');
            out.push(' ');
            i += 1;
            while i < n && chars[i] == ' ' { i += 1; }
            continue;
        }
        if c == ',' {
            trim_trailing_spaces(&mut out);
            out.push(',');
            out.push(' ');
            i += 1;
            while i < n && chars[i] == ' ' { i += 1; }
            continue;
        }
        if c == '{' {
            // 代码块的 { 前加空格（fn main() { / if x { / else { / struct X {）
            trim_trailing_spaces(&mut out);
            if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('{') { out.push(' '); }
            out.push('{');
            i += 1;
            while i < n && chars[i] == ' ' { i += 1; }
            continue;
        }
        if matches!(c, '+' | '-' | '*' | '/' | '%' | '<' | '>' | '=' | '&' | '|') {
            // 单字符运算符（排除 :: / -> 已在上面处理）
            // 一元 +/-/* （前一个非值 token）：不两侧加空格
            let prev = last_non_space(&out);
            let unary = matches!(c, '+' | '-' | '*' | '&') && prev.map(|p| "([{,=:;+-*/%<>!&|".contains(p)).unwrap_or(true);
            if unary {
                out.push(c);
            } else {
                trim_trailing_spaces(&mut out);
                if !out.is_empty() && !out.ends_with(' ') { out.push(' '); }
                out.push(c);
                out.push(' ');
            }
            i += 1;
            while i < n && chars[i] == ' ' { i += 1; }
            continue;
        }
        out.push(c);
        i += 1;
    }
    // 收尾：去尾空格，规范 ". " 之类
    let t = out.trim_end().to_string();
    t
}

fn trim_trailing_spaces(s: &mut String) {
    while s.ends_with(' ') { s.pop(); }
}

fn last_non_space(s: &str) -> Option<char> {
    s.chars().rev().find(|c| !c.is_whitespace())
}

/// 统计裸花括号（跳过字符串与注释）。
fn count_braces_aware(line: &str) -> (i32, i32) {
    let mut opens = 0;
    let mut closes = 0;
    let mut chars = line.chars().peekable();
    let mut in_str = false;
    let mut esc = false;
    while let Some(c) = chars.next() {
        if in_str {
            if esc { esc = false; continue; }
            if c == '\\' { esc = true; continue; }
            if c == '"' { in_str = false; }
            continue;
        }
        match c {
            '"' => in_str = true,
            '#' => break,
            '/' => { if chars.peek() == Some(&'/') { break; } }
            '{' => opens += 1,
            '}' => closes += 1,
            _ => {}
        }
    }
    (opens, closes)
}
