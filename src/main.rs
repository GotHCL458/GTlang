//! gtc —— GTLang 编译器与解释器（命令行前端）
//!
//! 统一前端（词法 → 语法 → 语义/类型检查 → 统一 AST），两条后端：
//!   - `--c`   编译：统一 AST → LLVM IR → clang → 本机可执行文件
//!   - `--run` 解释：统一 AST → Cranelift 即时编译并在内存中执行
//!
//! 两条后端共用同一份 AST 与同一套类型规则，因此对同一输入结果一致。
//! 库接口见 `lib.rs`（`build` / `Unit::interpret` / `Unit::compile_to`）。
//!
//! 语言：默认英文；在命令行**末尾**加 `zh` 参数则所有信息与诊断切换为中文。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gtc_rust::{build, lang, Diag, Span, Unit};

const USAGE_EN: &str = "\
gtc -- GTLang compiler / interpreter

Usage:
  gtc --c   <file.gt> [more.gt ...] [-o out.exe] [-O 0..3]   compile to executable
  gtc --run <file.gt> [more.gt ...]                         run with interpreter
  gtc --check <file.gt> [...]                               lex/parse/type-check only
  gtc --lint <file.gt> [...]                                static check (unused fn / unreachable / empty if / const cond / self-compare)
  gtc --lint --strict <file.gt>                             treat warnings as errors
  gtc --lint --json <file.gt>                               JSON output (for CI)
  gtc --emit-llvm <file.gt> [...]                           emit LLVM IR (.ll) only

Options:
  -o <path>      output path (default: same dir/name as first source)
  -O <n>         optimization level 0..3 (default 2)
  --watch, -w    watch sources and rebuild on change (Ctrl-C to quit)
  --keep-tmp     keep temp dir (for debugging; default deleted on success)
  --no-color     disable colored diagnostics
  --no-overflow-check   disable integer overflow checks (faster, wraps)
  -h, --help     show this help
  zh             append as the last argument to use Chinese messages

Notes:
  Multiple sources are treated as one compilation unit (concatenated in order).
  With no mode flag, --c is assumed.
  Temp artifacts live in %TEMP%\\gtc\\<tag>_<pid>\\ and are removed afterwards.
";

const USAGE_ZH: &str = "\
gtc —— GTLang 编译器 / 解释器

用法：
  gtc --c   <文件.gt> [更多文件.gt ...] [-o 输出.exe] [-O 0..3]   编译为可执行文件
  gtc --run <文件.gt> [更多文件.gt ...]                          用解释器直接运行
  gtc --check <文件.gt> [...]                                    只做词法/语法/类型检查
  gtc --lint <文件.gt> [...]                                     静态检查（未用函数/不可达代码/空 if/常量条件/自比较）
  gtc --lint --strict <文件.gt>                                  把提示当作错误（有提示即退出码 1）
  gtc --lint --json <文件.gt>                                    JSON 输出（便于 CI）
  gtc --emit-llvm <文件.gt> [...]                                只生成 LLVM IR(.ll)

选项：
  -o <path>      指定输出文件（默认与首个源文件同目录同名）
  -O <n>         优化级别 0..3（默认 2）
  --watch, -w    监控源文件，变更即重构建（Ctrl-C 退出）
  --keep-tmp     保留临时目录（排查问题用，默认成功即删）
  --no-color     关闭诊断彩色输出
  --no-overflow-check  关闭整数溢出检查（更快，溢出回绕）
  -h, --help     显示本帮助
  zh             作为最后一个参数，输出中文消息

说明：
  多个源文件会被视为同一个编译单元（按顺序拼接），便于拆分模块。
  不加模式参数时默认按 --c 处理。
  中间产物统一放在 %TEMP%\\gtc\\<标签>_<pid>\\，结束后自动删除。
";

fn usage() -> &'static str {
    if lang::is_zh() { USAGE_ZH } else { USAGE_EN }
}

/// 运行模式
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// 编译为可执行文件（LLVM 全后端）
    Compile,
    /// 解释执行（Cranelift 即时编译）
    Run,
    /// 只做检查（词法/语法/类型）
    Check,
    /// 运行测试（`test_` 前缀函数）
    Test,
    /// 只产出 LLVM IR
    EmitLlvm,
    /// 静态检查（未用函数、不可达代码、空 if、常量条件、自比较）
    Lint,
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();

    // 语言开关：命令行末尾出现 `zh` 即切换为中文（先于其它解析）
    if args.last().map(|s| s == "zh").unwrap_or(false) {
        lang::set_zh();
        args.pop();
    }

    if args.is_empty() {
        eprintln!("{}", usage());
        return ExitCode::from(1);
    }

    let mut mode = Mode::Compile;
    let mut out: Option<PathBuf> = None;
    let mut opt: u8 = 2;
    let mut no_color = false;
    let mut keep_tmp = false;
    let mut watch = false;
    let mut lint_strict = false;
    let mut lint_json = false;
    let mut sources: Vec<PathBuf> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-h" | "--help" => {
                println!("{}", usage());
                return ExitCode::SUCCESS;
            }
            "-v" | "-V" | "--version" => {
                println!("gtc 0.0.1c");
                return ExitCode::SUCCESS;
            }
            "--verbose" => gtc_rust::set_verbose(true),
            "--c" | "--compile" => mode = Mode::Compile,
            "--run" | "--interp" => mode = Mode::Run,
            "--check" => mode = Mode::Check,
            "--lint" => mode = Mode::Lint,
            "--strict" => lint_strict = true,
            "--json" => lint_json = true,
            "--test" | "test" => mode = Mode::Test,
            "--emit-llvm" => mode = Mode::EmitLlvm,
            "--no-color" => no_color = true,
            "--keep-tmp" => keep_tmp = true,
            "--watch" | "-w" => watch = true,
            "--no-overflow-check" | "-fno-overflow" => gtc_rust::disable_overflow_check(),
            "-o" => {
                i += 1;
                match args.get(i) {
                    Some(p) => out = Some(PathBuf::from(p)),
                    None => {
                        eprintln!("{}", lang::tr("error: -o requires a path", "错误：-o 后面需要路径"));
                        return ExitCode::from(1);
                    }
                }
            }
            "-O" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse::<u8>().ok()) {
                    Some(n) if n <= 3 => opt = n,
                    _ => {
                        eprintln!("{}", lang::tr("error: -O requires 0..3", "错误：-O 后面需要 0..3"));
                        return ExitCode::from(1);
                    }
                }
            }
            s if s.starts_with('-') => {
                if lang::is_zh() {
                    eprintln!("错误：未知选项 '{}'\n\n{}", s, usage());
                } else {
                    eprintln!("error: unknown option '{}'\n\n{}", s, usage());
                }
                return ExitCode::from(1);
            }
            _ => sources.push(PathBuf::from(a)),
        }
        i += 1;
    }

    if sources.is_empty() {
        if lang::is_zh() {
            eprintln!("错误：缺少源文件\n\n{}", usage());
        } else {
            eprintln!("error: missing source file\n\n{}", usage());
        }
        return ExitCode::from(1);
    }

    if watch {
        // --watch：轮询 mtime，变更即重跑（Ctrl-C 退出）
        if lang::is_zh() {
            println!("[watch] 监控中（Ctrl-C 退出）");
        } else {
            println!("[watch] watching for changes (Ctrl-C to quit)");
        }
        let mut last = file_stamps(&sources);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(400));
            let cur = file_stamps(&sources);
            if cur != last {
                last = cur;
                if lang::is_zh() { println!("\n[watch] 变更，重新构建…"); } else { println!("\n[watch] change detected, rebuilding..."); }
                if let Err(e) = drive(mode, &sources, out.as_deref(), opt, no_color, keep_tmp, lint_strict, lint_json) {
                    eprintln!("{}", e);
                }
            }
        }
    }

    match drive(mode, &sources, out.as_deref(), opt, no_color, keep_tmp, lint_strict, lint_json) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", e);
            ExitCode::from(1)
        }
    }
}

/// 每个源文件的 (路径, mtime)，用于 --watch 变更检测。
fn file_stamps(files: &[PathBuf]) -> Vec<(PathBuf, Option<std::time::SystemTime>)> {
    files
        .iter()
        .map(|p| (p.clone(), std::fs::metadata(p).and_then(|m| m.modified()).ok()))
        .collect()
}

/// 主流程：读入 → 统一前端 → 按模式分派后端
fn drive(
    mode: Mode,
    sources: &[PathBuf],
    out: Option<&Path>,
    opt: u8,
    no_color: bool,
    keep_tmp: bool,
    lint_strict: bool,
    lint_json: bool,
) -> Result<(), String> {
    let first = &sources[0];

    // 1. 读入并解码（UTF-8 优先，其它编码按系统代码页转换）
    let (text, notes) = gtc_rust::load_sources(sources)?;
    for n in &notes {
        if lang::is_zh() {
            println!("源文件编码：{}", n);
        } else {
            println!("source encoding: {}", n);
        }
    }

    // 2. 统一前端：import 模块解析 + 词法 + 语法 + 语义/类型检查 → 统一 AST
    let t_front = std::time::Instant::now();
    let unit: Unit = {
        let built = if sources.len() == 1 {
            gtc_rust::build_file(first)
        } else {
            build(&first.to_string_lossy(), &text)
        };
        match built {
            Ok(u) => u,
            Err(diags) => {
                render_diags(first, &text, &diags, no_color);
                let stage = diags.first().map(|d| d.stage.label()).unwrap_or(lang::tr("error", "错误"));
                return Err(if lang::is_zh() {
                    format!("[{}] 发现 {} 个错误（见上方诊断）\n  文件：{}", stage, diags.len(), first.display())
                } else {
                    format!("[{}] {} error(s) found (see diagnostics above)\n  file: {}", stage, diags.len(), first.display())
                });
            }
        }
    };

    if gtc_rust::verbose() {
        eprintln!("[verbose] frontend (lex+parse+check): {:?}", t_front.elapsed());
    }

    if mode == Mode::Lint {
        return run_lint(&first, &text, no_color, lint_strict, lint_json);
    }

    if mode == Mode::Check {
        if lang::is_zh() {
            println!("检查通过：{}", first.display());
        } else {
            println!("check passed: {}", first.display());
        }
        return Ok(());
    }

    // 3. 解释执行（Cranelift 即时编译，不产出文件）
    if mode == Mode::Run {
        return unit.interpret();
    }

    // 3b. 测试：运行所有 `test_` 前缀函数
    if mode == Mode::Test {
        return run_tests(&unit);
    }

    // 4. 编译：统一 AST → LLVM IR → clang
    let target = match out {
        Some(p) => p.to_path_buf(),
        None => first.with_extension(if mode == Mode::EmitLlvm { "ll" } else { "exe" }),
    };

    if mode == Mode::EmitLlvm {
        let ll = unit.write_llvm(&target)?;
        if lang::is_zh() {
            println!("已生成 LLVM IR：{}", ll.display());
        } else {
            println!("LLVM IR written: {}", ll.display());
        }
        return Ok(());
    }

    let exe = unit.compile_to_ex(&target, opt, keep_tmp)?;
    // 若 exe 旁缺少 stdlib dll，从 res/lib 复制（便携发布）
    copy_stdlib_dlls(&exe);
    if lang::is_zh() {
        println!("编译成功：{}", exe.display());
        if keep_tmp {
            println!("  （临时目录已保留在 %TEMP%\\gtc\\ 下）");
        }
    } else {
        println!("compiled: {}", exe.display());
        if keep_tmp {
            println!("  (temp dir kept under %TEMP%\\gtc\\)");
        }
    }
    Ok(())
}

// ============================================================
// 彩色诊断（ariadne）
// ============================================================

/// `gtc --lint`：静态检查（未用函数、不可达代码、空 if、常量条件、自比较）。
fn run_lint(file: &std::path::Path, text: &str, no_color: bool, strict: bool, json: bool) -> Result<(), String> {
    let _ = no_color;
    let prog = match gtc_rust::parser::parse_program(text) {
        Ok(p) => p,
        Err(e) => return Err(format!("[lint] {}：{}", file.display(), e.msg)),
    };
    let warns = gtc_rust::lint::lint(&prog);
    if json {
        let mut out = String::from("[");
        for (i, (line, msg)) in warns.iter().enumerate() {
            if i > 0 { out.push(','); }
            let m = msg.replace('\\', "\\\\").replace('"', "\\\"");
            let f = file.display().to_string().replace('\\', "\\\\").replace('"', "\\\"");
            out.push_str(&format!("{{\"file\":\"{}\",\"line\":{},\"message\":\"{}\"}}", f, line, m));
        }
        out.push(']');
        println!("{}", out);
    } else {
        for (line, msg) in &warns {
            println!("{}:{}  [lint] {}", file.display(), line, msg);
        }
        if warns.is_empty() {
            if lang::is_zh() {
                println!("检查通过（无提示）：{}", file.display());
            } else {
                println!("lint clean: {}", file.display());
            }
        } else if lang::is_zh() {
            eprintln!("[lint] 共 {} 条提示", warns.len());
        } else {
            eprintln!("[lint] {} warning(s)", warns.len());
        }
    }
    if !warns.is_empty() && strict {
        return Err(if lang::is_zh() { format!("[lint] --strict：有 {} 条提示", warns.len()) } else { format!("[lint] --strict: {} warning(s)", warns.len()) });
    }
    Ok(())
}

/// stderr 是否为控制台（可上色）：被重定向时自动关闭，并顺带开启 Windows VT
fn use_color(force_off: bool) -> bool {
    if force_off {
        return false;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(n: u32) -> *mut core::ffi::c_void;
        fn GetConsoleMode(h: *mut core::ffi::c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(h: *mut core::ffi::c_void, mode: u32) -> i32;
    }
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    const ENABLE_VT: u32 = 0x0004;
    unsafe {
        let h = GetStdHandle(STD_ERROR_HANDLE);
        let mut mode = 0u32;
        if GetConsoleMode(h, &mut mode) == 0 {
            return false;
        }
        SetConsoleMode(h, mode | ENABLE_VT);
        true
    }
}

fn render_diags(path: &Path, src_text: &str, diags: &[Diag], no_color: bool) {
    let colored = use_color(no_color);
    for d in diags {
        report_spanned(path, src_text, d.stage.label(), &d.span, &d.message, colored, &d.notes);
    }
}

/// 以字节区间 + ariadne 渲染一条诊断
fn report_spanned(
    path: &Path,
    src_text: &str,
    kind: &str,
    span: &Span,
    msg: &str,
    colored: bool,
    notes: &[(String, Span)],
) {
    let len = src_text.len();
    if len == 0 {
        eprintln!("{}: {}", kind, msg);
        return;
    }

    let mut s = span.start.min(len - 1);
    let mut e = span.end.min(len);
    if e <= s {
        e = (s + 1).min(len);
    }
    if e <= s {
        s = len - 1;
        e = len;
    }

    let name = path.to_string_lossy().to_string();
    // 消息可能含 \x01 分隔的"提示"（如 closest 建议）
    let (main_msg, extra_hint) = match msg.find('\x01') {
        Some(i) => (&msg[..i], Some(msg[i+1..].to_string())),
        None => (msg, None),
    };
    let label_msg = gtc_rust::strip_line_prefix(main_msg).to_string();

    let cfg = ariadne::Config::default()
        .with_index_type(ariadne::IndexType::Byte)
        .with_color(colored);

    let mut report = ariadne::Report::build(ariadne::ReportKind::Error, (name.clone(), s..e))
        .with_code(kind)
        .with_config(cfg)
        .with_message(&label_msg)
        .with_label(
            ariadne::Label::new((name.clone(), s..e))
                .with_message(&label_msg)
                .with_color(ariadne::Color::Red),
        );
    // 按消息推断修复建议（hint），渲染为 help 行
    let hint_src = gtc_rust::strip_line_prefix(main_msg).to_string();
    if let Some(h) = extra_hint {
        report = report.with_help(h);
    } else if let Some(h) = gtc_rust::hint_for(&hint_src) {
        report = report.with_help(h);
    }
    // 相关位置备注（notes）
    for (label, nspan) in notes.iter() {
        let ns = nspan.start.min(len.saturating_sub(1));
        let ne = nspan.end.min(len).max(ns + 1);
        report = report.with_label(
            ariadne::Label::new((name.clone(), ns..ne))
                .with_message(label)
                .with_color(ariadne::Color::Blue),
        );
    }

    let src_owned = src_text.to_string();
    if let Err(err) = report
        .finish()
        .eprint((name.clone(), ariadne::Source::from(src_owned)))
    {
        eprintln!("{}: {}", kind, msg);
        if lang::is_zh() {
            eprintln!("（诊断渲染失败：{}）", err);
        } else {
            eprintln!("(diagnostic rendering failed: {})", err);
        }
    }
}

/// 运行测试：所有 test_ 前缀函数，逐个在子进程中执行。
fn run_tests(unit: &Unit) -> Result<(), String> {
    use gtc_rust::ast::Item;
    // 用优化前的 AST（lower 会删掉未被调用的函数）
    let raw = gtc_rust::parser::parse_program(&unit.text).map_err(|e| e.msg)?;
    let mut tests: Vec<String> = Vec::new();
    for item in &raw.items {
        if let Item::Fn(f) = item {
            if f.name.starts_with("test_") {
                tests.push(f.name.clone());
            }
        }
    }
    if tests.is_empty() {
        if lang::is_zh() {
            println!("未找到测试（约定：函数名以 test_ 开头）");
        } else {
            println!("no tests found (convention: fn name starts with test_)");
        }
        return Ok(());
    }
    let mut passed = 0;
    let mut failed = 0;
    for (idx, name) in tests.iter().enumerate() {
        // 生成临时入口：调用该测试函数
        let src = format!("fn main() {{ {}() }}", name);
        let tmp = std::env::temp_dir().join("gtc").join(format!("test_{}_{}.gt", std::process::id(), idx));
        if let Some(dir) = tmp.parent() { let _ = std::fs::create_dir_all(dir); }
        // 拼接原源（去掉原 main 定义）+ 临时 main
        let mut full = strip_main(&unit.text);
        full.push_str("\n");
        full.push_str(&src);
        let _ = std::fs::write(&tmp, &full);
        // 每个测试独立子进程（assert 失败会 exit，不污染其它测试）
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let st = std::process::Command::new(&exe).arg("--run").arg(&tmp).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
        match st {
            Ok(s) if s.success() => { passed += 1; println!("test {} ... ok", name); }
            _ => { failed += 1; println!("test {} ... FAILED", name); }
        }
        let _ = std::fs::remove_file(&tmp);
    }
    println!("\ntest result: {} passed; {} failed", passed, failed);
    if failed > 0 { Err("测试未全部通过".into()) } else { Ok(()) }
}


/// 从源码中删掉顶层 fn main 定义（用于测试时替换入口）。
fn strip_main(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut depth: i32 = 0;
    let mut skipping = false;
    for line in src.lines() {
        let t = line.trim_start();
        if !skipping && (t.starts_with("fn main(") || t.starts_with("pub fn main(")) {
            skipping = true;
            depth = 0;
        }
        if skipping {
            depth += line.matches('{').count() as i32;
            depth -= line.matches('}').count() as i32;
            if depth <= 0 && line.contains('}') {
                skipping = false;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}


/// 把 res/lib 下的 stdlib dll 复制到 exe 同目录（若缺失）。
fn copy_stdlib_dlls(exe: &Path) {
    let out_dir = match exe.parent() { Some(d) => d, None => return };
    // 找 res/lib
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        if let Some(d) = cur.parent() { candidates.push(d.join("lib")); candidates.push(d.join("res").join("lib")); }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("res").join("lib"));
        candidates.push(cwd.join("lib"));
    }
    for src_dir in candidates {
        if !src_dir.is_dir() { continue; }
        if let Ok(rd) = std::fs::read_dir(&src_dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x == "dll").unwrap_or(false) {
                    if let Some(name) = p.file_name() {
                        let dst = out_dir.join(name);
                        if !dst.exists() {
                            let _ = std::fs::copy(&p, &dst);
                        }
                    }
                }
            }
        }
        break;
    }
}
