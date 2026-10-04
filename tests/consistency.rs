//! 一致性测试体系：**同一份源码，解释器后端与编译器后端必须逐字节一致**。
//!
//! 这是"统一 AST + 统一类型规则"的直接验证：
//!   - 编译器后端：`gtc --c` → `.ll` → clang → exe → 运行产物
//!   - 解释器后端：`gtc --run` → Cranelift 即时编译后在内存中执行
//!
//! 测试全部通过真实命令行进程进行，因此同时覆盖了 CLI、运行时与两个后端。
//!
//! 三类断言：
//!   1. `examples/` 下所有 `.gt` 示例双后端输出一致
//!   2. 内联小样例（含类型转换、插值、浮点格式）双后端输出一致
//!   3. 非法程序两个后端都被**同一套编译期检查**拒绝

use std::path::{Path, PathBuf};
use std::process::Command;

// ============================================================
// 基础设施
// ============================================================

/// cargo 为本 crate 的集成测试提供的可执行文件路径
fn gtc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_gtc"))
}

/// 测试用临时目录：统一放在 `%TEMP%\gtc\consistency_<pid>\`，
/// 与编译器自身的中间目录同级，便于识别与清理。
fn tmp_dir() -> PathBuf {
    let d = std::env::temp_dir()
        .join("gtc")
        .join(format!("consistency_{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("无法创建临时目录");
    d
}

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

/// `examples/*.gt`，按文件名排序保证结果稳定。
/// 跳过长期阻塞的服务器示例（含 serve/serve_fn 调用）——它们不适合"跑完比对"。
fn example_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.filter_map(|e| e.ok()) {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().map(|x| x == "gt").unwrap_or(false) {
                    out.push(p);
                }
            }
        }
    }
    let mut v: Vec<PathBuf> = Vec::new();
    walk(&examples_dir(), &mut v);
    v.retain(|p| {
        let src = std::fs::read_to_string(p).unwrap_or_default();
        // 跳过服务器示例（阻塞）与无 main 的库文件
        !src.contains("serve(") && !src.contains("serve_fn(") && src.contains("fn main")
    });
    v.sort();
    assert!(!v.is_empty(), "examples/ 下没有任何 .gt");
    v
}

fn decode(bytes: &[u8]) -> String {
    // 不做换行归一化：JIT 与 AOT 必须输出完全相同的字节（含 \n vs \r\n）
    String::from_utf8_lossy(bytes).into_owned()
}

/// 双后端跑同一份源码，返回 `(解释器输出, 编译器输出)`
fn both_backends(tag: &str, src: &Path) -> (String, String) {
    let dir = tmp_dir();
    let gtc = gtc();

    // ---- 解释器后端 ----
    let interp = Command::new(&gtc)
        .arg("--run")
        .arg(src)
        .output()
        .expect("无法启动 gtc --run");
    assert!(
        interp.status.success(),
        "[{}] 解释器执行失败：\n{}",
        tag,
        decode(&interp.stderr)
    );

    // ---- 编译器后端 ----
    let exe = dir.join(format!("{}.exe", tag));
    let compiled = Command::new(&gtc)
        .arg("--c")
        .arg(src)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("无法启动 gtc --c");
    assert!(
        compiled.status.success(),
        "[{}] 编译失败：\n{}\n{}",
        tag,
        decode(&compiled.stdout),
        decode(&compiled.stderr)
    );

    let ran = Command::new(&exe).output().expect("无法运行编译产物");
    assert!(
        ran.status.success(),
        "[{}] 编译产物运行失败，退出码 {:?}",
        tag,
        ran.status.code()
    );

    (decode(&interp.stdout), decode(&ran.stdout))
}

/// 断言双后端输出完全一致
fn assert_consistent(tag: &str, src: &Path) {
    let (a, b) = both_backends(tag, src);
    if a != b {
        // 逐行 diff，便于定位第一处不一致
        let la: Vec<&str> = a.lines().collect();
        let lb: Vec<&str> = b.lines().collect();
        let mut report = String::new();
        for i in 0..la.len().max(lb.len()) {
            let x = la.get(i).copied().unwrap_or("<缺行>");
            let y = lb.get(i).copied().unwrap_or("<缺行>");
            if x != y {
                report.push_str(&format!("  第 {} 行\n    解释器: {:?}\n    编译器: {:?}\n", i + 1, x, y));
            }
        }
        panic!(
            "[{}] 两个后端结果不一致：\n{}\n--- 解释器完整输出 ---\n{}\n--- 编译器完整输出 ---\n{}",
            tag, report, a, b
        );
    }
}

/// 把源码写进临时文件后做一致性断言
fn assert_consistent_src(tag: &str, lines: &[&str]) {
    let p = tmp_dir().join(format!("{}.gt", tag));
    std::fs::write(&p, lines.join("\n")).expect("无法写临时源文件");
    assert_consistent(tag, &p);
}

// ============================================================
// 1. examples/ 全量双后端一致
// ============================================================

#[test]
fn examples_are_consistent_across_backends() {
    let files = example_files();
    for f in &files {
        let tag = f.file_stem().unwrap().to_string_lossy().to_string();
        assert_consistent(&tag, f);
    }
    println!("已校验 {} 个示例的双后端一致性", files.len());
}

// ============================================================
// 2. 针对性小样例
// ============================================================

#[test]
fn bitwise_operators_match() {
    assert_consistent_src(
        "bits",
        &[
            "const PAGE = 1 << 12",
            "fn main() {",
            "    x := 0b1100",
            "    y := 0b1010",
            "    put(\"and=${x & y} or=${x | y} xor=${x ^ y} not=${~x}\")",
            "    put(\"shl=${1 << 8} shr=${-16 >> 2} neg=${-1 >> 1}\")",
            "    put(\"PAGE=${PAGE} ~0=${~0}\")",
            "    m := 1",
            "    m <<= 4",
            "    m |= 3",
            "    m ^= 1",
            "    m &= 0xFE",
            "    put(\"复合 m=${m}\")",
            "    put(\"优先级 ${1 | 2 & 3} ${1 << 2 + 1} ${1 ^ 3 & 1}\")",
            "}",
        ],
    );
}

#[test]
fn array_element_assignment_matches() {
    assert_consistent_src(
        "arrset",
        &[
            "fn main() {",
            "    a := [1, 2, 3]",
            "    a[0] = 10",
            "    a[2] = 30",
            "    put(\"${a[0]} ${a[1]} ${a[2]}\")",
            "    a[1] += 5",
            "    a[0] *= 2",
            "    a[2] <<= 1",
            "    put(\"${a[0]} ${a[1]} ${a[2]}\")",
            "    i := 1",
            "    a[i] = a[i] + 100",
            "    put(\"${a[i]}\")",
            "    s := \"abc\"",
            "    put(\"${s[0]} ${s[2]}\")",
            "}",
        ],
    );
}

#[test]
fn short_circuit_matches() {
    assert_consistent_src(
        "shortcircuit",
        &[
            "fn side(n: int) -> bool {",
            "    put(\"  side(${n})\")",
            "    n > 0",
            "}",
            "fn main() {",
            "    put(\"A false && side(1)\")",
            "    put(\"  = ${false && side(1)}\")",
            "    put(\"B true || side(2)\")",
            "    put(\"  = ${true || side(2)}\")",
            "    put(\"C true && side(3)\")",
            "    put(\"  = ${true && side(3)}\")",
            "    put(\"D false || side(4)\")",
            "    put(\"  = ${false || side(4)}\")",
            "    put(\"E 链式 ${0 > 1 && side(5) && side(6)}\")",
            "    put(\"F 链式 ${1 > 0 || side(7) || side(8)}\")",
            "}",
        ],
    );
}

#[test]
fn inline_c_both_directions_match() {
    assert_consistent_src(
        "interop",
        &[
            "fn 三倍(x: int) -> int { x * 3 }",
            "fn 报数(n: int) { put(\"  C 回调 报数(${n})\") }",
            "C {",
            "    #include <stdio.h>",
            "    /* 调用 GTLang 的 三倍() */",
            "    static long long 用三倍(long long v) { return 三倍(v) + 1; }",
            "    static void 触发(long long n) { 报数(n); }",
            "    static long long 纯C(long long a, long long b) { return a * b + 7; }",
            "    static double 折半(double v) { return v / 2.0; }",
            "}",
            "fn main() {",
            "    put(\"纯C(6, 7) = ${纯C(6, 7)}\")",
            "    put(\"折半(9.0) = ${折半(9.0)}\")",
            "    put(\"用三倍(7) = ${用三倍(7)}\")",
            "    触发(42)",
            "    put(\"三倍(5) = ${三倍(5)}\")",
            "}",
        ],
    );
}

/// ASCII 名字的 GTLang 函数被 C 调用时，其 LLVM 符号不能与桥接包装重名。
///
/// 回归：`fn add` 若直接映射为 LLVM 符号 `add`，会与 C 桥接生成的同名包装
/// `add` 冲突，链接期报 duplicate symbol。编译器后端现统一给 GTLang
/// 函数加 `gt_` 前缀（与解释器一致），使其与桥接包装区分开。
#[test]
fn ascii_named_gt_fn_called_from_c_matches() {
    assert_consistent_src(
        "interop_ascii",
        &[
            "fn add(a: int, b: int) -> int { a + b }",
            "fn main() {",
            "    put(\"add(2, 3) = ${add(2, 3)}\")",
            "    put(\"via_c(4) = ${via_c(4)}\")",
            "}",
            "C {",
            "    /* 调用 ASCII 名字的 GTLang 函数 add */",
            "    static long long via_c(long long n) { return add(n, 100); }",
            "}",
        ],
    );
}

#[test]
fn type_annotations_and_conversions_match() {
    assert_consistent_src(
        "conv",
        &[
            "fn f(a: int) -> f64 {",
            "    f64(a) * 1.5",
            "}",
            "fn main() {",
            "    let n: int = 7",
            "    let s: str = \"42\"",
            "    let d: f64 = 2.5",
            "    put(\"${f(n)}\")",
            r#"    put("${int(s)} ${int(d)} ${int(\"7xyz\")}")"#,
            r#"    put("${str(n)} ${str(d)} ${str(true)} ${str(false)}")"#,
            r#"    put("${bool(n)} ${bool(0)} ${bool(\"\")} ${bool(\"x\")}")"#,
            r#"    put("${f64(s)} ${f64(n)} ${f64(\"-1.25e1\")}")"#,
            "}",
        ],
    );
}

#[test]
fn float_formatting_matches() {
    // %g 语义：6 位有效数字、去尾随 0、量级越界转科学计数法
    assert_consistent_src(
        "floatfmt",
        &[
            "fn main() {",
            "    put(\"${1.0 / 3.0}\")",
            "    put(\"${0.0001} ${0.00001}\")",
            "    put(\"${99999.0} ${100000.0} ${1000000.0}\")",
            "    put(\"${-2.5} ${7.0} ${1.0 / 7.0}\")",
            "}",
        ],
    );
}

#[test]
fn unicode_escapes_match() {
    // \uXXXX（BMP）与 \u{...}（任意码点）
    assert_consistent_src(
        "unicode_escapes",
        &[
            "fn main() {",
            "    put(\"\\u4f60\\u597d\")",     // 你好
            "    put(\"\\u{1F600}\")",          // 😀
            "    put(len(\"\\u0041\"))",        // 1
            "    put(\"a\\u0041b\")",           // aAb
            "}",
        ],
    );
}

#[test]
fn interpolation_and_escapes_match() {
    assert_consistent_src(
        "interp",
        &[
            "fn tag(名: str, 值: int) -> str { \"$名=$值\" }",
            "fn main() {",
            "    let 世界: str = \"世界\"",
            "    let 分: int = 95",
            "    put(\"你好 ${世界}\")",
            "    put(tag(\"计数\", 42))",
            "    put(\"${1 + 2 * 3}\")",
            "    put(\"换行[\\n]制表[\\t]结束\")",
            "    put(r\"原始 \\n 不转义，$名 不插值\")",
            "    put(\"字面美元 \\$分（上面的 95 没有被代入）\")",
            "}",
        ],
    );
}

#[test]
fn control_flow_and_arrays_match() {
    assert_consistent_src(
        "flow",
        &[
            "fn fib(n: int) -> int {",
            "    if n < 2 { return n }",
            "    fib(n - 1) + fib(n - 2)",
            "}",
            "fn main() {",
            "    a := [3, 1, 4, 1, 5]",
            "    s := 0",
            "    for v in a { s = s + v }",
            "    put(\"sum=${s} len=${len(a)}\")",
            "    for i in 0..8 {",
            "        if i % 3 == 0 { continue }",
            "        if i > 6 { break }",
            "        print(\"${i} \")",
            "    }",
            "    put(\"\")",
            "    put(\"fib(12)=${fib(12)}\")",
            "    put(\"a[2]=${a[2]}\")",
            "}",
        ],
    );
}

// ============================================================
// 3. 错误处理：同一套编译期检查拒绝同一批非法程序
// ============================================================

/// 非法程序在 `--check` 下必须失败（两个后端共用前端，因此拒绝行为一致）
fn assert_rejected(tag: &str, lines: &[&str], expect_msg: &str) {
    let p = tmp_dir().join(format!("bad_{}.gt", tag));
    std::fs::write(&p, lines.join("\n")).expect("无法写临时源文件");

    // 断言用中文消息：显式传 `zh`（默认英文）
    let out = Command::new(gtc())
        .arg("--check")
        .arg(&p)
        .arg("zh")
        .output()
        .expect("无法启动 gtc --check");
    let err = decode(&out.stderr) + &decode(&out.stdout);
    assert!(
        !out.status.success(),
        "[{}] 非法程序竟然通过了检查：\n{}",
        tag,
        err
    );
    assert!(
        err.contains(expect_msg),
        "[{}] 诊断信息未包含 {:?}：\n{}",
        tag,
        expect_msg,
        err
    );

    // 编译后端也必须在同一处停下
    let compiled = Command::new(gtc())
        .arg("--c")
        .arg(&p)
        .arg("-o")
        .arg(tmp_dir().join(format!("bad_{}.exe", tag)))
        .output()
        .expect("无法启动 gtc --c");
    assert!(
        !compiled.status.success(),
        "[{}] 非法程序竟然编译成功",
        tag
    );
}

#[test]
fn bounds_check_aborts_both_backends() {
    // 越界必须被**同样地**拦下：两后端都打印同一条运行时错误并退出码 1
    let p = tmp_dir().join("oob.gt");
    std::fs::write(
        &p,
        "fn main() {\n    a := [1, 2, 3]\n    put(\"${a[9]}\")\n    put(\"不可达\")\n}\n",
    )
    .unwrap();

    // 解释器
    let interp = Command::new(gtc())
        .arg("--run")
        .arg(&p)
        .arg("zh")
        .output()
        .expect("无法启动 gtc --run");
    assert!(!interp.status.success(), "解释器未拦下越界");
    let ie = decode(&interp.stderr) + &decode(&interp.stdout);
    assert!(ie.contains("越界"), "解释器缺少越界诊断：\n{}", ie);
    assert!(!ie.contains("不可达"), "解释器越界后仍继续执行");

    // 编译器
    let exe = tmp_dir().join("oob.exe");
    let c = Command::new(gtc())
        .arg("--c")
        .arg(&p)
        .arg("-o")
        .arg(&exe)
        .arg("zh")
        .output()
        .expect("无法启动 gtc --c");
    assert!(c.status.success(), "编译失败：\n{}", decode(&c.stderr));
    let ran = Command::new(&exe).output().expect("无法运行产物");
    assert!(!ran.status.success(), "编译产物未拦下越界");
    let ce = decode(&ran.stderr) + &decode(&ran.stdout);
    assert!(ce.contains("越界"), "编译产物缺少越界诊断：\n{}", ce);
    assert!(!ce.contains("不可达"), "编译产物越界后仍继续执行");

    // 负下标：Python 风格（-1 = 末尾），非法负数（越界）仍被拦
    let p2 = tmp_dir().join("oob_neg.gt");
    std::fs::write(&p2, "fn main() {\n    a := [1, 2, 3]\n    put(\"${a[-10]}\")\n}\n").unwrap();
    let neg = Command::new(gtc())
        .arg("--run")
        .arg(&p2)
        .arg("zh")
        .output()
        .expect("无法启动 gtc --run");
    assert!(!neg.status.success(), "负下标越界未被拦下");
    assert!(decode(&neg.stderr).contains("越界"), "负下标缺少越界诊断");
}

#[test]
fn floordiv_is_not_silently_a_comment() {
    // 回归：`x := a // 5` 必须真正做整除。
    // 旧规则"`//` 后跟空白即注释"会让它静默变成 `x := a`，属于不报错的算错。
    assert_consistent_src(
        "floordiv",
        &[
            "fn main() {",
            "    a := 17",
            "    b := a // 5",
            "    put(\"17 // 5 = ${b}\")",
            "    c := a // 5 + 1",
            "    put(\"17 // 5 + 1 = ${c}\")",
            "    d := 100 // 7 // 2",
            "    put(\"100 // 7 // 2 = ${d}\")",
            "    put(\"内联整除 ${20 // 6}\")",
            "}",
        ],
    );
}

#[test]
fn line_comments_both_styles_work() {
    // `//` 在语句/行首位置仍按注释处理；行内注释用 `#`
    assert_consistent_src(
        "comments",
        &[
            "// 行首 // 注释",
            "fn main() {",
            "    x := 1  # 行内注释",
            "    // 块内整行注释",
            "    put(\"${x}\")",
            "} // 函数结束后的注释",
        ],
    );
}

#[test]
fn div_by_zero_aborts_both_backends() {
    for (tag, expr) in [("div", "10 / x"), ("rem", "7 % x"), ("fdiv", "10 // x")] {
        let p = tmp_dir().join(format!("dz_{}.gt", tag));
        // 注意 `${{{}}}`：`{{`/`}}` 是 format! 的转义花括号，`{}` 才是占位符，
        // 合起来生成 GTLang 的插值语法 `${10 / x}`。
        let src = format!(
            "fn main() {{\n    x := 0\n    put(\"${{{}}}\")\n    put(\"不可达\")\n}}\n",
            expr
        );
        assert!(src.contains("${"), "测试用例生成错误：\n{}", src);
        assert!(!src.contains("${{"), "插值语法被 format! 转义破坏了：\n{}", src);
        std::fs::write(&p, &src).unwrap();

        // 解释器
        let interp = Command::new(gtc())
            .arg("--run")
            .arg(&p)
            .arg("zh")
            .output()
            .expect("无法启动 gtc --run");
        assert!(!interp.status.success(), "[{}] 解释器未拦下除零", tag);
        let ie = decode(&interp.stderr) + &decode(&interp.stdout);
        assert!(ie.contains("除数为 0"), "[{}] 解释器缺诊断：\n{}", tag, ie);
        assert!(!ie.contains("不可达"), "[{}] 解释器除零后仍继续", tag);

        // 编译器
        let exe = tmp_dir().join(format!("dz_{}.exe", tag));
        let c = Command::new(gtc())
            .arg("--c")
            .arg(&p)
            .arg("-o")
            .arg(&exe)
            .arg("zh")
            .output()
            .expect("无法启动 gtc --c");
        assert!(c.status.success(), "[{}] 编译失败：\n{}", tag, decode(&c.stderr));
        let ran = Command::new(&exe).output().expect("无法运行产物");
        assert!(!ran.status.success(), "[{}] 编译产物未拦下除零", tag);
        let ce = decode(&ran.stderr) + &decode(&ran.stdout);
        assert!(ce.contains("除数为 0"), "[{}] 编译产物缺诊断：\n{}", tag, ce);
        assert!(!ce.contains("不可达"), "[{}] 编译产物除零后仍继续", tag);
    }
}

#[test]
fn string_comparison_matches() {
    // 回归：JIT 里字符串 `!=` 曾返回 i1（Bool 在 JIT 用 i64），导致 verifier 失败。
    // 字符串只支持 == / !=，其余比较在前端统一拒绝。
    assert_consistent_src(
        "strcmp",
        &[
            "fn main() {",
            "    a := \"abc\"",
            "    b := \"abd\"",
            "    put(\"${a == b} ${a != b}\")",
            "    c := \"abc\"",
            "    put(\"${a == c} ${a != c}\")",
            "    if a == c { put(\"eq 分支\") }",
            "    if a != b { put(\"ne 分支\") }",
            "    put(\"${len(a) == len(b)}\")",
            "}",
        ],
    );
}

#[test]
fn string_ordering_is_rejected_up_front() {
    // 字符串 `<` 必须在**前端**就被拒绝（两后端同一条诊断），而不是等到后端
    assert_rejected(
        "str_lt",
        &[
            "fn main() {",
            "    a := \"abc\"",
            "    b := \"abd\"",
            "    put(\"${a < b}\")",
            "}",
        ],
        "字符串只支持 == / != 比较",
    );
}

#[test]
fn edge_cases_match() {
    // 递归深度 / 大数组 / 嵌套循环 / 交换排序 / 大整数
    assert_consistent_src(
        "edge",
        &[
            "fn 求和(n: int) -> int {",
            "    if n <= 0 { return 0 }",
            "    求和(n - 1) + n",
            "}",
            "fn main() {",
            "    put(\"求和(500) = ${求和(500)}\")",
            "    put(\"最大值 = ${9223372036854775807}\")",
            "    a := [5, 4, 3, 2, 1]",
            "    for i in 0..5 {",
            "        for j in 0..5 {",
            "            if a[i] < a[j] {",
            "                t := a[i]",
            "                a[i] = a[j]",
            "                a[j] = t",
            "            }",
            "        }",
            "    }",
            "    put(\"排序后 ${a[0]} ${a[1]} ${a[2]} ${a[3]} ${a[4]}\")",
            "    i := 0",
            "    while true {",
            "        i = i + 1",
            "        if i > 7 { break }",
            "    }",
            "    put(\"while+break i=${i}\")",
            "}",
        ],
    );
}

#[test]
fn float_edge_cases_match() {
    // 浮点除零遵循 IEEE（得 inf / nan），不做整数那样的检查
    assert_consistent_src(
        "floatedge",
        &[
            "fn main() {",
            "    put(\"1.0/0.0 = ${1.0 / 0.0}\")",
            "    z := 0.0",
            "    put(\"0.0/0.0 = ${z / z}\")",
            "    put(\"-1.0/0.0 = ${-1.0 / 0.0}\")",
            "}",
        ],
    );
}

#[test]
fn type_errors_are_rejected() {
    assert_rejected(
        "let_ty",
        &["fn main() {", "    let x: int = \"abc\"", "    put(\"${x}\")", "}"],
        "声明为 整数",
    );
    assert_rejected(
        "ret_ty",
        &["fn f() -> int { \"字符串\" }", "fn main() { put(\"${f()}\") }"],
        "函数返回值",
    );
    assert_rejected(
        "arith_ty",
        &["fn main() {", "    let s: str = \"a\"", "    put(\"${s * 2}\")", "}"],
        "算术运算需要数值",
    );
    assert_rejected(
        "cond_ty",
        &["fn main() {", "    if 1 { put(\"x\") }", "}"],
        "条件应为布尔",
    );
    assert_rejected(
        "const_ty",
        &["const K: f64 = \"abc\"", "fn main() { put(\"${K}\") }"],
        "常量 'K'",
    );
    assert_rejected(
        "assign_ty",
        &["fn main() {", "    let mut n: int = 1", "    n = 2.5", "    put(\"${n}\")", "}"],
        "显式类型",
    );
    assert_rejected(
        "builtin_arg",
        &["fn main() {", "    a := [1, 2]", "    put(\"${int(a)}\")", "}"],
        "int() 不能转换",
    );
    assert_rejected(
        "unknown_fn",
        &["fn main() {", "    nope(1)", "}"],
        "未定义的函数",
    );
    assert_rejected(
        "arity",
        &["fn f(a: int) -> int { a }", "fn main() { put(\"${f(1, 2)}\") }"],
        "需要 1 个参数",
    );
}

/// 新增的编译期检查：缺字段(E503) / 缺返回值(E301) / break·continue 在循环外(E303)
#[test]
fn new_compile_checks_are_rejected() {
    assert_rejected(
        "struct_missing_field",
        &[
            "struct P { x: int, y: int }",
            "fn main() { p := P { x: 1 }",
            "    put(\"${p.x}\") }",
        ],
        "缺少字段",
    );
    assert_rejected(
        "missing_return",
        &["fn f(n: int) -> int {", "    if n > 0 { return 1 }", "}", "fn main() { put(\"${f(1)}\") }"],
        "缺少返回值",
    );
    assert_rejected(
        "break_outside_loop",
        &["fn main() {", "    break", "}"],
        "只能出现在循环内",
    );
    assert_rejected(
        "continue_outside_loop",
        &["fn main() {", "    continue", "}"],
        "只能出现在循环内",
    );
}

/// 默认（不带 `zh`）应为英文诊断；带 `zh` 时为中文。
#[test]
fn default_language_is_english() {
    let p = tmp_dir().join("lang_default.gt");
    std::fs::write(&p, "fn main() { nope(1) }").unwrap();

    // 默认：英文
    let en = Command::new(gtc()).arg("--check").arg(&p).output().unwrap();
    let en_err = decode(&en.stderr) + &decode(&en.stdout);
    assert!(en_err.contains("undefined function"), "默认应为英文：\n{}", en_err);
    assert!(!en_err.contains("未定义"), "默认不应出现中文：\n{}", en_err);

    // zh：中文
    let zh = Command::new(gtc()).arg("--check").arg(&p).arg("zh").output().unwrap();
    let zh_err = decode(&zh.stderr) + &decode(&zh.stdout);
    assert!(zh_err.contains("未定义"), "带 zh 应为中文：\n{}", zh_err);
}


#[test]
fn multiple_syntax_errors_all_reported() {
    // 多行顶层裸标识符：应一次报出全部 3 个错误（而非首错即停）
    let p = tmp_dir().join("many_syntax.gt");
    std::fs::write(&p, "gjhrfhe\nyjrytrh\nhguyjert\n").expect("无法写临时源文件");
    let out = Command::new(gtc())
        .arg("--check")
        .arg(&p)
        .arg("zh")
        .output()
        .expect("无法启动 gtc --check");
    assert!(!out.status.success(), "非法程序竟然通过了检查");
    let err = decode(&out.stderr) + &decode(&out.stdout);
    assert!(err.contains("发现 3 个错误"), "应报出全部 3 个错误：\n{}", err);
    assert!(err.contains(":1:1"), "缺少第 1 行位置：\n{}", err);
    assert!(err.contains(":2:1"), "缺少第 2 行位置：\n{}", err);
    assert!(err.contains(":3:1"), "缺少第 3 行位置：\n{}", err);
}

#[test]
fn syntax_errors_are_reported_with_position() {
    let p = tmp_dir().join("bad_syntax.gt");
    std::fs::write(&p, "fn main() {\n    put(\"x\"\n}\n").expect("无法写临时源文件");
    let out = Command::new(gtc())
        .arg("--check")
        .arg(&p)
        .arg("zh")
        .output()
        .expect("无法启动 gtc --check");
    assert!(!out.status.success(), "缺右括号竟然通过了检查");
    let err = decode(&out.stderr) + &decode(&out.stdout);
    assert!(err.contains("语法错误"), "缺少「语法错误」诊断：\n{}", err);
    // ariadne 会画出源码片段：报出出错行号 + 该行内容，说明位置信息可用
    assert!(err.contains(":3:"), "诊断未给出出错行号：\n{}", err);
    assert!(err.contains("3 │ }"), "诊断未画出源码片段：\n{}", err);
}

// ============================================================
// 4. import 模块系统（方案 B：真模块）
// ============================================================

/// 把一组 (文件名, 内容) 写进临时子目录，返回入口文件路径。
fn write_project(tag: &str, files: &[(&str, &[&str])]) -> PathBuf {
    let dir = tmp_dir().join(format!("mod_{}", tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("无法创建模块目录");
    let mut entry = None;
    for (name, body) in files {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("无法创建子目录");
        }
        std::fs::write(&p, body.join("\n")).expect("无法写模块源文件");
        if *name == "main.gt" {
            entry = Some(p);
        }
    }
    entry.expect("测试项目必须包含 main.gt")
}

/// 多文件项目双后端一致性断言。
fn assert_consistent_project(tag: &str, files: &[(&str, &[&str])]) {
    let entry = write_project(tag, files);
    assert_consistent(tag, &entry);
}

#[test]
fn module_import_basic_matches() {
    assert_consistent_project(
        "mod_basic",
        &[
            (
                "math.gt",
                &[
                    "pub fn 平方(x: int) -> int { x * x }",
                    "pub const 圆周率: f64 = 3.14159",
                    "fn 私有(x: int) -> int { x + 1 }",
                ],
            ),
            (
                "main.gt",
                &[
                    "import math",
                    "fn main() {",
                    "    put(\"平方(5) = ${math.平方(5)}\")",
                    "    put(\"圆周率 = ${math.圆周率}\")",
                    "}",
                ],
            ),
        ],
    );
}

#[test]
fn module_import_alias_and_nested_matches() {
    assert_consistent_project(
        "mod_alias",
        &[
            ("a.gt", &["pub fn f() -> int { 10 }"]),
            ("b.gt", &["import a as x", "pub fn g() -> int { x.f() + 1 }"]),
            (
                "main.gt",
                &[
                    "import b",
                    "import a",
                    "fn main() {",
                    "    put(\"a.f() = ${a.f()}\")",
                    "    put(\"b.g() = ${b.g()}\")",
                    "}",
                ],
            ),
        ],
    );
}

/// 回归：跨模块 struct 类型名加前缀后，**方法签名**（参数/返回类型）也必须同步改写。
/// 曾出现 impl 点 { fn 平移(...) -> 点 } 的返回类型未加前缀，导致
/// "函数返回值 声明为 结构体(点)，但实际是 结构体(geometry__点)"。
#[test]
fn module_struct_type_prefix_in_methods_matches() {
    assert_consistent_project(
        "mod_struct_prefix",
        &[
            (
                "geometry.gt",
                &[
                    "pub struct 点 { x: f64, y: f64 }",
                    "impl 点 {",
                    "    fn 平移(self, dx: f64, dy: f64) -> 点 {",
                        "        点 { x: self.x + dx, y: self.y + dy }",
                    "    }",
                    "    fn 距离平方(self) -> f64 { self.x * self.x + self.y * self.y }",
                    "}",
                    "pub fn 造点(a: f64, b: f64) -> 点 { 点 { x: a, y: b } }",
                ],
            ),
            (
                "shapes.gt",
                &[
                    "import geometry",
                    "pub trait 形状 { fn 面积(self) -> f64 }",
                    "pub struct 圆 { 半径: f64 }",
                    "impl 形状 for 圆 {",
                    "    fn 面积(self) -> f64 { 3.14159 * self.半径 * self.半径 }",
                    "}",
                ],
            ),
            (
                "main.gt",
                &[
                    "import geometry",
                    "import shapes",
                    "fn main() {",
                    "    p := geometry.造点(3.0, 4.0)",
                    "    put(\"距离平方 = ${p.距离平方()}\")",
                    "    q := p.平移(1.0, 1.0)",
                    "    put(\"q.x = ${q.x}, q.y = ${q.y}\")",
                    "    c := shapes.圆 { 半径: 2.0 }",
                    "    put(\"圆面积 = ${c.面积()}\")",
                    "}",
                ],
            ),
        ],
    );
}

#[test]
fn module_private_symbol_is_rejected() {
    let entry = write_project(
        "mod_priv",
        &[
            ("lib.gt", &["fn 私有(x: int) -> int { x }"]),
            ("main.gt", &["import lib", "fn main() { put(\"${lib.私有(1)}\") }"]),
        ],
    );
    let out = Command::new(gtc())
        .arg("--check")
        .arg(&entry)
        .arg("zh")
        .output()
        .expect("无法启动 gtc --check");
    assert!(!out.status.success(), "私有函数竟然可以从外部访问");
    let err = decode(&out.stderr) + &decode(&out.stdout);
    assert!(err.contains("未定义"), "应报未定义符号：\n{}", err);
}

#[test]
fn module_cycle_is_rejected() {
    let entry = write_project(
        "mod_cycle",
        &[
            ("a.gt", &["import b", "pub fn fa() -> int { 1 }"]),
            ("b.gt", &["import a", "pub fn fb() -> int { 2 }"]),
            ("main.gt", &["import a", "fn main() { put(\"${a.fa()}\") }"]),
        ],
    );
    let out = Command::new(gtc())
        .arg("--check")
        .arg(&entry)
        .output()
        .expect("无法启动 gtc --check");
    assert!(!out.status.success(), "import 环路竟然通过了检查");
    let err = decode(&out.stderr) + &decode(&out.stdout);
    assert!(err.contains("环路"), "应报 import 环路：\n{}", err);
}

#[test]
fn module_missing_is_rejected() {
    let entry = write_project(
        "mod_missing",
        &[("main.gt", &["import 不存在的模块", "fn main() { }"])],
    );
    let out = Command::new(gtc())
        .arg("--check")
        .arg(&entry)
        .arg("zh")
        .output()
        .expect("无法启动 gtc --check");
    assert!(!out.status.success(), "缺失模块竟然通过了检查");
    let err = decode(&out.stderr) + &decode(&out.stdout);
    assert!(err.contains("找不到模块"), "应报找不到模块：\n{}", err);
}


// ============================================================
// 5. 容器类型 list / set / map（引用语义，双后端一致）
// ============================================================

#[test]
fn containers_match() {
    assert_consistent_src(
        "containers",
        &[
            "fn main() {",
            "    xs := list()",
            "    push(xs, 10)",
            "    push(xs, 20)",
            "    push(xs, 30)",
            "    put(\"len=${len(xs)} at0=${at(xs, 0)} at2=${at(xs, 2)}\")",
            "    put(\"has20=${has(xs, 20)} pop=${pop(xs)} len=${len(xs)}\")",
            "    s := set()",
            "    insert(s, 1)",
            "    insert(s, 1)",
            "    insert(s, 2)",
            "    put(\"setlen=${len(s)} has2=${has(s, 2)} has9=${has(s, 9)}\")",
            "    m := map()",
            "    insert(m, 1, 100)",
            "    insert(m, 1, 111)",
            "    insert(m, 2, 200)",
            "    put(\"maplen=${len(m)} m1=${at(m, 1)} m2=${at(m, 2)} keys=${len(keys(m))}\")",
            "}",
        ],
    );
}

#[test]
fn container_remove_and_values_match() {
    assert_consistent_src(
        "containers2",
        &[
            "fn main() {",
            "    xs := list()",
            "    push(xs, 5)",
            "    push(xs, 6)",
            "    push(xs, 7)",
            "    remove(xs, 0)",
            "    put(\"after remove: len=${len(xs)} at0=${at(xs, 0)}\")",
            "    m := map()",
            "    insert(m, 10, 1)",
            "    insert(m, 20, 2)",
            "    remove(m, 10)",
            "    put(\"map after remove: len=${len(m)} has10=${has(m, 10)}\")",
            "    vs := values(m)",
            "    put(\"values len=${len(vs)} v0=${at(vs, 0)}\")",
            "}",
        ],
    );
}


// ============================================================
// 6. 数值 / 字符串内置（双后端一致）
// ============================================================

#[test]
fn numeric_builtins_match() {
    assert_consistent_src(
        "numeric",
        &[
            "fn main() {",
            "    put(\"abs(-5)=${abs(-5)} abs(-3.5)=${abs(-3.5)}\")",
            "    put(\"min=${min(3, 7)} max=${max(3, 7)}\")",
            "    put(\"minf=${min(2.5, 1.5)} maxf=${max(2.5, 8.5)}\")",
            "    xs := list()",
            "    push(xs, 10)",
            "    push(xs, 20)",
            "    push(xs, 30)",
            "    put(\"sum=${sum(xs)}\")",
            "}",
        ],
    );
}

#[test]
fn string_builtins_match() {
    assert_consistent_src(
        "strbuiltin",
        &[
            "fn main() {",
            "    h := \"hello world\"",
            "    w := \"world\"",
            "    put(\"substr=${substr(h, 0, 5)}\")",
            "    put(\"find=${find(h, w)}\")",
            "    a := \"abc\"",
            "    A := \"ABC\"",
            "    put(\"upper=${upper(a)} lower=${lower(A)}\")",
            "    ab := \"ab\"",
            "    put(\"repeat=${repeat(ab, 3)}\")",
            "    raw := \"a-b-c\"",
            "    dash := \"-\"",
            "    plus := \"+\"",
            "    put(\"replace=${replace(raw, dash, plus)}\")",
            "    csv := \"a,b,c\"",
            "    comma := \",\"",
            "    parts := split(csv, comma)",
            "    put(\"split=${len(parts)} ${at(parts, 0)} ${at(parts, 2)}\")",
            "    bar := \"|\"",
            "    put(\"join=${join(parts, bar)}\")",
            "}",
        ],
    );
}


// ============================================================
// 7. 裸内存操作 mem_*（双后端一致）
// ============================================================

#[test]
fn mem_operations_match() {
    assert_consistent_src(
        "memops",
        &[
            "fn main() {",
            "    p := mem_alloc(16)",
            "    mem_store_i64(p, 0, 12345)",
            "    mem_store_i64(p, 8, 67890)",
            "    put(\"i64: ${mem_load_i64(p, 0)} ${mem_load_i64(p, 8)}\")",
            "    mem_store_u8(p, 0, 65)",
            "    put(\"u8: ${mem_load_u8(p, 0)}\")",
            "    q := mem_alloc(8)",
            "    mem_set(q, 7, 8)",
            "    put(\"set: ${mem_load_u8(q, 0)}\")",
            "    mem_copy(q, p, 8)",
            "    put(\"copy: ${mem_load_u8(q, 0)}\")",
            "    mem_free(p)",
            "    mem_free(q)",
            "}",
        ],
    );
}


// ============================================================
// 8. 标准库 libGT.dll（Rust + 第三方库，双后端一致）
// ============================================================

#[test]
fn stdlib_math_matches() {
    assert_consistent_src(
        "stdlib_math",
        &[
            "import math",
            "fn main() {",
            "    put(\"sqrt2=${sqrt(2.0)}\")",
            "    put(\"pow=${pow(2.0, 10.0)}\")",
            "    put(\"floor=${floor(3.7)} ceil=${ceil(3.2)} round=${round(3.5)}\")",
            "    put(\"gcd=${gcd(12, 18)} lcm=${lcm(4, 6)} ipow=${ipow(2, 8)}\")",
            "}",
        ],
    );
}

#[test]
fn stdlib_string_matches() {
    assert_consistent_src(
        "stdlib_str",
        &[
            "import string",
            "fn main() {",
            "    h := \"hello\"",
            "    put(\"cap=${capitalize(h)} rev=${reverse(h)}\")",
            "    b := \"banana\"",
            "    needle := \"an\"",
            "    put(\"count=${count(b, needle)}\")",
            "    s1 := \"123\"",
            "    s2 := \"12a\"",
            "    put(\"isnum=${isnumeric(s1)} notnum=${isnumeric(s2)}\")",
            "}",
        ],
    );
}


// ============================================================
// 9. 容器完整语义：数组 / 列表 / 集合 / 字典
// ============================================================

#[test]
fn array_ops_match() {
    assert_consistent_src(
        "array_full",
        &[
            "fn main() {",
            "    a := [1, 2, 3, 4, 5]",
            "    put(\"len=${len(a)} a0=${a[0]} a4=${a[4]}\")",
            "    a[0] = 10",
            "    a[4] = 50",
            "    put(\"after set: ${a[0]} ${a[4]}\")",
            "    s := 0",
            "    for v in a { s = s + v }",
            "    put(\"sum=${s}\")",
            "}",
        ],
    );
}

#[test]
fn list_full_ops_match() {
    assert_consistent_src(
        "list_full",
        &[
            "fn main() {",
            "    xs := list()",
            "    for i in 0..5 { push(xs, i * i) }",
            "    put(\"len=${len(xs)} at=${at(xs, 0)} ${at(xs, 4)}\")",
            "    put(\"has0=${has(xs, 0)} has9=${has(xs, 9)}\")",
            "    pop(xs)",
            "    put(\"after pop: len=${len(xs)}\")",
            "    remove(xs, 0)",
            "    put(\"after remove0: len=${len(xs)} at0=${at(xs, 0)}\")",
            "}",
        ],
    );
}

#[test]
fn set_full_ops_match() {
    assert_consistent_src(
        "set_full",
        &[
            "fn main() {",
            "    s := set()",
            "    for i in 0..20 { insert(s, i % 5) }",
            "    put(\"len=${len(s)}\")",
            "    put(\"has3=${has(s, 3)} has9=${has(s, 9)}\")",
            "    insert(s, 3)",
            "    put(\"after reinsert len=${len(s)}\")",
            "    remove(s, 3)",
            "    put(\"after remove has3=${has(s, 3)} len=${len(s)}\")",
            "}",
        ],
    );
}

#[test]
fn map_full_ops_match() {
    assert_consistent_src(
        "map_full",
        &[
            "fn main() {",
            "    m := map()",
            "    insert(m, 1, 10)",
            "    insert(m, 2, 20)",
            "    insert(m, 1, 111)",
            "    put(\"len=${len(m)} m1=${at(m, 1)} m2=${at(m, 2)}\")",
            "    put(\"has1=${has(m, 1)} has9=${has(m, 9)}\")",
            "    ks := keys(m)",
            "    vs := values(m)",
            "    put(\"keys=${len(ks)} values=${len(vs)}\")",
            "    remove(m, 1)",
            "    put(\"after remove: len=${len(m)} has1=${has(m, 1)}\")",
            "}",
        ],
    );
}


// ============================================================
// 10. elif + 嵌套函数 + 递归全形态
// ============================================================

#[test]
fn elif_matches() {
    assert_consistent_src(
        "elif",
        &[
            "fn 判定(分: int) -> str {",
            "    if 分 >= 90 { \"优秀\" }",
            "    elif 分 >= 60 { \"及格\" }",
            "    elif 分 >= 30 { \"待努力\" }",
            "    else { \"不及格\" }",
            "}",
            "fn main() {",
            "    put(判定(95))",
            "    put(判定(72))",
            "    put(判定(45))",
            "    put(判定(10))",
            "}",
        ],
    );
}

#[test]
fn nested_fn_matches() {
    assert_consistent_src(
        "nestedfn",
        &[
            "fn main() {",
            "    fn 双倍(x: int) -> int { x * 2 }",
            "    put(\"双倍=${双倍(21)}\")",
            "    fn 阶乘(n: int) -> int {",
            "        if n <= 1 { 1 } else { n * 阶乘(n - 1) }",
            "    }",
            "    put(\"阶乘=${阶乘(5)}\")",
            "    fn 外层(n: int) -> int {",
            "        fn 内层(k: int) -> int { k + 1 }",
            "        内层(n) * 10",
            "    }",
            "    put(\"多层=${外层(4)}\")",
            "}",
        ],
    );
}


// ============================================================
// 11. struct 结构体（定义/字段/字面量/赋值）
// ============================================================

#[test]
fn struct_matches() {
    assert_consistent_src(
        "struct",
        &[
            "struct Point { x: int, y: int }",
            "fn 距离平方(p: Point) -> int { p.x * p.x + p.y * p.y }",
            "fn main() {",
            "    p := Point { x: 3, y: 4 }",
            "    put(\"p = (${p.x}, ${p.y})\")",
            "    put(\"d2 = ${距离平方(p)}\")",
            "    p.x = 10",
            "    p.y = 20",
            "    put(\"after: (${p.x}, ${p.y})\")",
            "    p.x += 5",
            "    put(\"p.x += 5 -> ${p.x}\")",
            "    q := Point { x: p.x + 1, y: p.y + 1 }",
            "    put(\"q = (${q.x}, ${q.y})\")",
            "}",
        ],
    );
}


// ============================================================
// 12. match 模式匹配
// ============================================================

#[test]
fn match_matches() {
    assert_consistent_src(
        "match",
        &[
            "fn 描述(n: int) -> str {",
            "    match n {",
            "        0 => { \"零\" }",
            "        1 => { \"一\" }",
            "        _ => { \"其他\" }",
            "    }",
            "}",
            "fn main() {",
            "    put(描述(0))",
            "    put(描述(1))",
            "    put(描述(5))",
            "    x := 7",
            "    r := match x {",
            "        0 => { 100 }",
            "        _ if x > 5 => { 200 }",
            "        _ => { 300 }",
            "    }",
            "    put(\"r=${r}\")",
            "    s := \"b\"",
            "    put(match s {",
            "        \"a\" => { \"A\" }",
            "        \"b\" => { \"B\" }",
            "        _ => { \"?\" }",
            "    })",
            "}",
        ],
    );
}


// ============================================================
// 13. 泛型单态化
// ============================================================

#[test]
fn generics_matches() {
    assert_consistent_src(
        "generics",
        &[
            "fn 恒等[T](x: T) -> T { x }",
            "fn 最大[T](a: T, b: T) -> T { if a > b { a } else { b } }",
            "fn main() {",
            "    put(恒等(42))",
            "    put(恒等(\"hi\"))",
            "    put(最大(3, 7))",
            "    put(最大(2.5, 1.5))",
            "}",
        ],
    );
}


// ============================================================
// 14. 闭包（捕获 + 高阶）
// ============================================================

#[test]
fn closures_match() {
    assert_consistent_src(
        "closures",
        &[
            "fn 应用(f, x: int) -> int { f(x) }",
            "fn 加法器(n: int) -> int {",
            "    闭 := |x| x + n",
            "    闭(100)",
            "}",
            "fn main() {",
            "    双倍 := |x| x * 2",
            "    put(双倍(21))",
            "    a := 10",
            "    b := 20",
            "    合 := |x| x + a + b",
            "    put(合(5))",
            "    put(应用(|x| x * x, 7))",
            "    put(加法器(1000))",
            "}",
        ],
    );
}


// ============================================================
// 15. extern "C" 外部 C 函数声明
// ============================================================

#[test]
fn extern_c_matches() {
    assert_consistent_src(
        "extern_c",
        &[
            "C {",
            "    #include <string.h>",
            "    #include <stdlib.h>",
            "}",
            "extern \"C\" {",
            "    fn strlen(s: str) -> int",
            "    fn abs(x: int) -> int",
            "}",
            "fn main() {",
            "    put(\"strlen=${strlen(\\\"hello\\\")}\")",
            "    put(\"abs=${abs(-42)}\")",
            "}",
        ],
    );
}




// ============================================================
// 16. 面向对象：struct + impl 方法 + self
// ============================================================

#[test]
fn oop_matches() {
    assert_consistent_src(
        "oop",
        &[
            "struct 矩形 { 宽: int, 高: int }",
            "impl 矩形 {",
            "    fn 面积(self) -> int { self.宽 * self.高 }",
            "    fn 放大(self, k: int) -> int { self.宽 * k }",
            "}",
            "struct 计数 { n: int }",
            "impl 计数 {",
            "    fn 取(&self) -> int { self.n }",
            "    fn 加(&mut self, k: int) -> int { self.n + k }",
            "}",
            "fn main() {",
            "    r := 矩形 { 宽: 3, 高: 4 }",
            "    put(\"面积=${r.面积()}\")",
            "    put(\"放大=${r.放大(2)}\")",
            "    c := 计数 { n: 5 }",
            "    put(\"取=${c.取()} 加=${c.加(3)}\")",
            "}",
        ],
    );
}


// ============================================================
// 17. trait：声明 + impl Trait for Type + 静态分派
// ============================================================

#[test]
fn trait_matches() {
    assert_consistent_src(
        "trait",
        &[
            "trait 可面积 { fn 面积(self) -> int }",
            "struct 正方形 { 边: int }",
            "struct 长方形 { 宽: int, 高: int }",
            "impl 可面积 for 正方形 {",
            "    fn 面积(self) -> int { self.边 * self.边 }",
            "}",
            "impl 可面积 for 长方形 {",
            "    fn 面积(self) -> int { self.宽 * self.高 }",
            "}",
            "fn main() {",
            "    a := 正方形 { 边: 5 }",
            "    b := 长方形 { 宽: 3, 高: 4 }",
            "    put(a.面积())",
            "    put(b.面积())",
            "}",
        ],
    );
}


// ============================================================
// 18. 常量不可变 + 函数内 const
// ============================================================

#[test]
fn const_immutable_rejected() {
    assert_rejected(
        "const_reassign",
        &["const X = 10", "fn main() { X = 20 }"],
        "不能给不可变变量或常量",
    );
    assert_rejected(
        "let_reassign",
        &["fn main() {", "    let y = 1", "    y = 2", "}"],
        "不能给不可变变量或常量",
    );
}

#[test]
fn mutable_vars_and_local_const_ok() {
    assert_consistent_src(
        "mutable_ok",
        &[
            "fn main() {",
            "    x := 1",
            "    x = 2",
            "    let mut y = 3",
            "    y = 4",
            "    const K = 10",
            "    put(x + y + K)",
            "}",
        ],
    );
}

// ============================================================
// 19. C 交互（双向）
// ============================================================

#[test]
fn c_interop_matches() {
    assert_consistent_src(
        "c_interop",
        &[
            "C {",
            "    static long long 阶乘_c(long long n) {",
            "        long long r = 1;",
            "        for (long long i = 2; i <= n; i++) r *= i;",
            "        return r;",
            "    }",
            "    static long long 两次_gt(long long x) { return gt_平方(x) * 2; }",
            "}",
            "extern \"C\" { fn strlen(s: str) -> int }",
            "fn gt_平方(x: int) -> int { x * x }",
            "fn main() {",
            "    put(阶乘_c(5))",
            "    put(两次_gt(6))",
            "    put(strlen(\"hello\"))",
            "}",
        ],
    );
}


// ============================================================
// 20. 所有权 / 借用
// ============================================================

#[test]
fn move_and_borrow_rejected() {
    // move 后再用
    assert_rejected(
        "use_after_move",
        &["fn main() {", "    s := \"hi\"", "    t := s", "    put(s)", "}"],
        "使用了已移动的值",
    );
    // &mut 在仍被使用期间再做 & → 冲突
    assert_rejected(
        "borrow_conflict",
        &["fn main() {", "    x := 1", "    a := &mut x", "    b := &x", "    put(a)", "}"],
        "独占借用",
    );
}

/// Result[T,E] + ? 传播：双后端一致。
#[test]
fn result_and_try_matches() {
    assert_consistent_src(
        "result_try",
        &[
            "fn 安全除(a: int, b: int) -> Result[int, str] {",
            "    if b == 0 { return Err(\"div0\") }",
            "    Ok(a / b)",
            "}",
            "fn 计算(x: int) -> Result[int, str] {",
            "    v := 安全除(100, x)?",
            "    put(\"first ok\")",
            "    w := 安全除(v, 2)?",
            "    Ok(w + 1)",
            "}",
            "fn main() {",
            "    match 计算(5) { _ => { put(\"done5\") } }",
            "    match 计算(0) { _ => { put(\"done0\") } }",
            "}",
        ],
    );
}

#[test]
fn c_header_import_matches() {
    assert_consistent_project(
        "c_header",
        &[
            (
                "mymath.h",
                &[
                    "#ifndef MYMATH_H",
                    "#define MYMATH_H",
                    "long long my_add(long long a, long long b);",
                    "double my_hypot(double a, double b);",
                    "#endif",
                ],
            ),
            (
                "mymath.c",
                &[
                    "#include <math.h>",
                    "long long my_add(long long a, long long b) { return a + b; }",
                    "double my_hypot(double a, double b) { return sqrt(a*a + b*b); }",
                ],
            ),
            (
                "main.gt",
                &[
                    "import c \"mymath.h\" as m",
                    "fn main() {",
                    "    put(m.my_add(3, 4))",
                    "    put(m.my_hypot(3.0, 4.0))",
                    "}",
                ],
            ),
        ],
    );
}


// ============================================================
// 22. 内嵌 C 调用（GTLang→C，C→GTLang）
// ============================================================

#[test]
fn inline_c_call_matches() {
    assert_consistent_src(
        "inline_c_call",
        &[
            "C {",
            "    static long long 三倍_c(long long x) { return x * 3; }",
            "    static long long 调gt(long long x) { return gt_加一(x) * 10; }",
            "}",
            "fn gt_加一(x: int) -> int { x + 1 }",
            "fn main() {",
            "    put(三倍_c(7))",
            "    put(调gt(4))",
            "}",
        ],
    );
}

// ============================================================
// 35u. assert
// ============================================================

#[test]
fn assert_matches() {
    assert_consistent_src(
        "assert_ok",
        &[
            "fn main() {",
            "    x := 5",
            "    assert(x > 0)",
            "    assert(x < 10, \"x too big\")",
            "    put(\"ok\")",
            "}",
        ],
    );
    // 失败的 assert 必须被**同样地**拦下
    let p = tmp_dir().join("assert_fail.gt");
    std::fs::write(&p, "fn main() {\n    x := 5\n    assert(x > 100, \"x must exceed 100\")\n    put(\"不可达\")\n}\n").unwrap();
    let interp = Command::new(gtc()).arg("--run").arg(&p).output().expect("无法启动 gtc --run");
    assert!(!interp.status.success(), "解释器未拦下断言失败");
    let ie = decode(&interp.stderr) + &decode(&interp.stdout);
    assert!(ie.contains("x must exceed 100"), "解释器缺少断言消息：\n{}", ie);
    assert!(!ie.contains("不可达"), "断言失败后仍继续执行");
}

// ============================================================
// 35ab. @derive(Debug/Default)
// ============================================================

#[test]
fn derive_debug_default() {
    assert_consistent_src(
        "derive_debug",
        &[
            "@derive(Debug, Default)",
            "struct 点 { x: int, y: int }",
            "fn main() {",
            "    p := 点 { x: 1, y: 2 }",
            "    put(p.to_str())",
            "    d := 点.default()",
            "    put(d.x)",
            "    put(d.y)",
            "}",
        ],
    );
}

// ============================================================
// 35aa. match 穷尽性检查
// ============================================================

#[test]
fn match_exhaustive_enum() {
    // 缺变体 → 应被拒绝
    let p = tmp_dir().join("exh_enum.gt");
    std::fs::write(&p, "enum 颜色 { 红 绿 蓝 }\nfn main() {\n    c := 颜色::红\n    match c {\n        颜色::红 => { put(1) }\n        颜色::绿 => { put(2) }\n    }\n}\n").unwrap();
    let out = Command::new(gtc()).arg("--check").arg(&p).arg("zh").output().unwrap();
    assert!(!out.status.success(), "缺变体竟然通过");
    let err = decode(&out.stderr) + &decode(&out.stdout);
    assert!(err.contains("不穷尽"), "应报不穷尽：\n{}", err);
    assert!(err.contains("蓝"), "应指出缺 蓝：\n{}", err);
}

#[test]
fn match_exhaustive_ok() {
    // 全部变体 → 通过
    assert_consistent_src(
        "exh_ok",
        &[
            "enum 颜色 { 红 绿 蓝 }",
            "fn main() {",
            "    c := 颜色::红",
            "    match c {",
            "        颜色::红 => { put(1) }",
            "        颜色::绿 => { put(2) }",
            "        颜色::蓝 => { put(3) }",
            "    }",
            "}",
        ],
    );
}

// ============================================================
// 35ai. loop N { } 计数循环
// ============================================================

#[test]
fn loop_count() {
    assert_consistent_src(
        "loop_count",
        &[
            "fn main() {",
            "    loop 3 {",
            "        put(1)",
            "    }",
            "    n := 2",
            "    loop n {",
            "        put(2)",
            "    }",
            "}",
        ],
    );
}

// ============================================================
// 35ah. @derive(PartialEq, Display)
// ============================================================

#[test]
fn derive_partial_eq_display() {
    assert_consistent_src(
        "derive_pd",
        &[
            "@derive(PartialEq, Display)",
            "struct 点 { x: int, y: int }",
            "fn main() {",
            "    a := 点 { x: 1, y: 2 }",
            "    b := 点 { x: 1, y: 2 }",
            "    put(a.eq(b))",
            "    put(a.to_str())",
            "}",
        ],
    );
}

// ============================================================
// 35ag. @derive(Ord) + 运算符重载（< <= > >=）
// ============================================================

#[test]
fn derive_ord_operators() {
    assert_consistent_src(
        "derive_ord_ops",
        &[
            "@derive(Ord)",
            "struct 点 { x: int }",
            "fn main() {",
            "    a := 点 { x: 1 }",
            "    b := 点 { x: 2 }",
            "    put(a < b)",
            "    put(a > b)",
            "    put(a <= a)",
            "    put(b >= a)",
            "}",
        ],
    );
}

// ============================================================
// 35af. @derive(Ord)
// ============================================================

#[test]
fn derive_ord() {
    assert_consistent_src(
        "derive_ord",
        &[
            "@derive(Ord)",
            "struct 点 { x: int, y: int }",
            "fn main() {",
            "    a := 点 { x: 1, y: 2 }",
            "    b := 点 { x: 1, y: 3 }",
            "    put(a.cmp(b))",
            "    put(b.cmp(a))",
            "    put(a.cmp(a))",
            "}",
        ],
    );
}

// ============================================================
// 35ae. 双向类型推断（声明类型回填）
// ============================================================

#[test]
fn bidirectional_inference() {
    assert_consistent_src(
        "bidir",
        &[
            "fn main() {",
            "    let xs: list = list()",
            "    push(xs, 1)",
            "    push(xs, 2)",
            "    put(len(xs))",
            "}",
        ],
    );
}

// ============================================================
// 35ad. import 内置库（无引号）
// ============================================================

#[test]
fn import_builtin_lib() {
    assert_consistent_src(
        "import_builtin",
        &[
            "import math",
            "import string",
            "fn main() {",
            "    put(upper(\"ab\"))",
            "    put(abs(-3))",
            "}",
        ],
    );
}

// ============================================================
// 35ac. dyn Trait（动态分发）
// ============================================================

#[test]
fn dyn_trait_matches() {
    assert_consistent_src(
        "dyn_trait",
        &[
            "trait 形状 { fn 面积(self) -> int }",
            "struct 圆 { r: int }",
            "impl 形状 for 圆 { fn 面积(self) -> int { return self.r * self.r } }",
            "fn f(s: dyn 形状) -> int {",
            "    return s.面积()",
            "}",
            "fn main() {",
            "    put(f(dyn 形状(圆 { r: 3 })))",
            "}",
        ],
    );
}

// ============================================================
// 36. 复杂综合程序（多特性交叉）
// ============================================================

/// 综合 1：泛型容器 + trait + 运算符重载 + 闭包 + 字符串
#[test]
fn complex_generic_trait_ops() {
    assert_consistent_src(
        "complex_generic",
        &[
            "@derive(Eq)",
            "struct 向量 { x: f64, y: f64 }",
            "impl 向量 {",
            "    fn add(self, o: 向量) -> 向量 {",
            "        向量 { x: self.x + o.x, y: self.y + o.y }",
            "    }",
            "    fn dot(self, o: 向量) -> f64 {",
            "        self.x * o.x + self.y * o.y",
            "    }",
            "}",
            "fn 映射(l: list, f) -> list {",
            "    r := list()",
            "    for v in l {",
            "        push(r, f(v))",
            "    }",
            "    return r",
            "}",
            "fn main() {",
            "    a := 向量 { x: 1.0, y: 2.0 }",
            "    b := 向量 { x: 3.0, y: 4.0 }",
            "    c := a + b",
            "    put(c.x)",
            "    put(c.y)",
            "    put(a.dot(b))",
            "    l := list()",
            "    push(l, 1)",
            "    push(l, 2)",
            "    push(l, 3)",
            "    push(l, 4)",
            "    d := 映射(l, |x| x * x)",
            "    for v in d {",
            "        put(v)",
            "    }",
            "    put(a == 向量 { x: 1.0, y: 2.0 })",
            "}",
        ],
    );
}

/// 综合 2：枚举 + match + Result + try/expt + 递归
#[test]
fn complex_enum_result_recursion() {
    assert_consistent_src(
        "complex_enum_result",
        &[
            "enum 树 { 叶(int) 枝(int, int) }",
            "fn 深度(t: 树) -> int {",
            "    match t {",
            "        树::叶(_) => { return 1 }",
            "        树::枝(l, r) => { return 1 + 深度(树::叶(l)) }",
            "    }",
            "    return 0",
            "}",
            "fn 阶乘(n: int) -> int {",
            "    if n <= 1 { return 1 }",
            "    return n * 阶乘(n - 1)",
            "}",
            "fn 可能失败(n: int) -> Result[int, str] {",
            "    if n < 0 { return Err(\"负数\") }",
            "    return Ok(n * 2)",
            "}",
            "fn main() {",
            "    t := 树::枝(1, 2)",
            "    put(深度(t))",
            "    put(阶乘(6))",
            "    match 可能失败(21) {",
            "        Ok(v) => { put(v) }",
            "        Err(e) => { put(e) }",
            "    }",
            "    match 可能失败(-1) {",
            "        Ok(v) => { put(v) }",
            "        Err(e) => { put(e) }",
            "    }",
            "}",
        ],
    );
}

/// 综合 3：并发 go + chan 生产者-消费者 + 循环
#[test]
fn complex_concurrency_pipeline() {
    assert_consistent_src(
        "complex_concurrency",
        &[
            "fn 生产者(ch: int, n: int) {",
            "    i := 0",
            "    while i < n {",
            "        chan_send(ch, i * i)",
            "        i = i + 1",
            "    }",
            "}",
            "fn main() {",
            "    ch := chan()",
            "    go 生产者(ch, 5)",
            "    和 := 0",
            "    j := 0",
            "    while j < 5 {",
            "        和 = 和 + chan_recv(ch)",
            "        j = j + 1",
            "    }",
            "    put(和)",
            "}",
        ],
    );
}

/// 综合 4：列表推导 + 字符串操作 + 宏 + 元组
#[test]
fn complex_macro_listcomp_str() {
    assert_consistent_src(
        "complex_macro_listcomp",
        &[
            "macro 三倍(x) {",
            "    (x) * 3",
            "}",
            "fn main() {",
            "    l := [1, 2, 3, 4, 5]",
            "    evens := [x for x in l if x % 2 == 0]",
            "    for e in evens {",
            "        put(e)",
            "    }",
            "    tripled := [三倍(x) for x in l if x > 3]",
            "    for t in tripled {",
            "        put(t)",
            "    }",
            "    t := (1, 2)",
            "    put(t.0)",
            "    put(t.1)",
            "    s := \"hello\"",
            "    put(upper(s))",
            "    put(len(s))",
            "}",
        ],
    );
}

// ============================================================
// 35z. 通道 chan
// ============================================================

#[test]
fn chan_matches() {
    assert_consistent_src(
        "chan",
        &[
            "fn producer(ch: int) {",
            "    chan_send(ch, 10)",
            "    chan_send(ch, 20)",
            "    chan_send(ch, 30)",
            "}",
            "fn main() {",
            "    ch := chan()",
            "    go producer(ch)",
            "    put(chan_recv(ch))",
            "    put(chan_recv(ch))",
            "    put(chan_recv(ch))",
            "}",
        ],
    );
}

// ============================================================
// 35y. 并发 go
// ============================================================

#[test]
fn go_thread_matches() {
    // go 起线程；sleep 同步。顺序不保证，故只验证"最终都执行了"。
    assert_consistent_src(
        "go_thread",
        &[
            "fn worker(n: int) {",
            "    put(n)",
            "}",
            "fn main() {",
            "    go worker(1)",
            "    sleep(600)",
            "    put(0)",
            "}",
        ],
    );
}

// ============================================================
// 35x. 声明式宏
// ============================================================

#[test]
fn macro_matches() {
    assert_consistent_src(
        "macro",
        &[
            "macro 平方(x) {",
            "    (x) * (x)",
            "}",
            "macro 最大值(a, b) {",
            "    a if a > b else b",
            "}",
            "fn main() {",
            "    put(平方(3))",
            "    put(平方(2 + 1))",
            "    put(最大值(10, 7))",
            "}",
        ],
    );
}

// ============================================================
// 35w. enum + 模式匹配
// ============================================================

#[test]
fn enum_matches() {
    assert_consistent_src(
        "enum",
        &[
            "enum 形状 {",
            "    Circle(f64)",
            "    Rect(f64, f64)",
            "    Unit",
            "}",
            "fn main() {",
            "    a := 形状::Circle(3.0)",
            "    b := 形状::Rect(2.0, 4.0)",
            "    c := 形状::Unit",
            "    match a {",
            "        形状::Circle(r) => { put(r) }",
            "        _ => { put(0.0) }",
            "    }",
            "    match b {",
            "        形状::Rect(w, h) => { put(w) }",
            "        _ => { put(0.0) }",
            "    }",
            "    match c {",
            "        形状::Unit => { put(99) }",
            "        _ => { put(0) }",
            "    }",
            "}",
        ],
    );
}

// ============================================================
// 35t. range() + 三元
// ============================================================

#[test]
fn range_and_ternary_match() {
    assert_consistent_src(
        "range_ternary",
        &[
            "fn main() {",
            "    a := range(5)",
            "    put(len(a))",
            "    put(at(a, 0))",
            "    put(at(a, 4))",
            "    b := range(2, 6)",
            "    put(len(b))",
            "    put(at(b, 0))",
            "    x := 5",
            "    y := 10",
            "    put(x if x > y else y)",
            "    put(x if x < y else y)",
            "    s := \"yes\" if x > 0 else \"no\"",
            "    put(s)",
            "}",
        ],
    );
}

// ============================================================
// 35s. 列表推导
// ============================================================

#[test]
fn list_comp_matches() {
    assert_consistent_src(
        "list_comp",
        &[
            "fn main() {",
            "    l := [1, 2, 3, 4, 5]",
            "    d := [x * 2 for x in l]",
            "    put(len(d))",
            "    put(at(d, 0))",
            "    put(at(d, 4))",
            "    e := [x for x in l if x > 2]",
            "    put(len(e))",
            "    put(at(e, 0))",
            "    put(at(e, 2))",
            "}",
        ],
    );
}

// ============================================================
// 35r. 负索引 s[-1]
// ============================================================

#[test]
fn negative_index_matches() {
    assert_consistent_src(
        "negative_index",
        &[
            "fn main() {",
            "    s := \"hello\"",
            "    l := [10, 20, 30]",
            "    put(s[-1])",
            "    put(s[-5])",
            "    put(l[-1])",
            "    put(l[-3])",
            "    put(l[-2])",
            "}",
        ],
    );
}

// ============================================================
// 35q. 解包交换 a, b = b, a
// ============================================================

#[test]
fn unpack_swap_matches() {
    assert_consistent_src(
        "unpack_swap",
        &[
            "fn main() {",
            "    a := 1",
            "    b := 2",
            "    a, b = b, a",
            "    put(a)",
            "    put(b)",
            "    c := 10",
            "    d := 20",
            "    e := 30",
            "    c, d, e = e, c, d",
            "    put(c)",
            "    put(d)",
            "    put(e)",
            "}",
        ],
    );
}

// ============================================================
// 35p. 宏：@derive(Eq)
// ============================================================

#[test]
fn derive_eq_matches() {
    assert_consistent_src(
        "derive_eq",
        &[
            "@derive(Eq)",
            "struct 点 { x: int, y: int }",
            "fn main() {",
            "    a := 点 { x: 1, y: 2 }",
            "    b := 点 { x: 1, y: 2 }",
            "    c := 点 { x: 3, y: 4 }",
            "    put(a == b)",
            "    put(a == c)",
            "    put(a != c)",
            "}",
        ],
    );
}

// ============================================================
// 35o. 优先级4：字符串切片
// ============================================================

#[test]
fn str_slice_matches() {
    assert_consistent_src(
        "str_slice",
        &[
            "fn main() {",
            "    s := \"hello world\"",
            "    put(s[0..5])",
            "    put(s[6..11])",
            "    put(s[0..1])",
            "}",
        ],
    );
}

// ============================================================
// 35n. 优先级4：闭包类型标注
// ============================================================

#[test]
fn closure_type_annotation_matches() {
    assert_consistent_src(
        "closure_typed",
        &[
            "fn main() {",
            "    加 := |a: int, b: int| -> int { a + b }",
            "    put(加(3, 4))",
            "    平方 := |x: f64| -> f64 { x * x }",
            "    put(平方(2.0))",
            "}",
        ],
    );
}

// ============================================================
// 35m. 优先级4：运算符重载
// ============================================================

#[test]
fn operator_overload_matches() {
    assert_consistent_src(
        "operator_overload",
        &[
            "struct 向量 { x: int, y: int }",
            "impl 向量 {",
            "    fn add(self, o: 向量) -> 向量 { 向量 { x: self.x + o.x, y: self.y + o.y } }",
            "    fn neg(self) -> 向量 { 向量 { x: -self.x, y: -self.y } }",
            "}",
            "fn main() {",
            "    a := 向量 { x: 1, y: 2 }",
            "    b := 向量 { x: 3, y: 4 }",
            "    c := a + b",
            "    put(c.x)",
            "    put(c.y)",
            "    d := -a",
            "    put(d.x)",
            "    put(d.y)",
            "}",
        ],
    );
}

// ============================================================
// 35l. 优先级4：解构绑定
// ============================================================

#[test]
fn destructure_matches() {
    assert_consistent_src(
        "destructure",
        &[
            "fn main() {",
            "    let (a, b, c) = (1, 2, 3)",
            "    put(a)",
            "    put(b)",
            "    put(c)",
            "    let (x, y) = (10, 20)",
            "    put(x + y)",
            "}",
        ],
    );
}

// ============================================================
// 35k. 优先级4：元组
// ============================================================

#[test]
fn tuple_matches() {
    assert_consistent_src(
        "tuple",
        &[
            "fn main() {",
            "    t := (1, 2, 3)",
            "    put(t.0)",
            "    put(t.1)",
            "    put(t.2)",
            "    u := (10, \"hi\")",
            "    put(u.0)",
            "    put(u.1)",
            "    put(t.0 + t.1)",
            "}",
        ],
    );
}

// ============================================================
// 35j. 优先级3：trait 默认方法
// ============================================================

#[test]
fn trait_default_method_matches() {
    assert_consistent_src(
        "trait_default",
        &[
            "trait 打招呼 {",
            "    fn 你好(self) -> str { \"hi\" }",
            "}",
            "struct 人 { 名: str }",
            "impl 打招呼 for 人 {}",
            "fn main() {",
            "    p := 人 { 名: \"a\" }",
            "    put(p.你好())",
            "}",
        ],
    );
}

// ============================================================
// 35i. 优先级3：命名参数
// ============================================================

#[test]
fn named_args_matches() {
    assert_consistent_src(
        "named_args",
        &[
            "fn 减(a: int, b: int) -> int { a - b }",
            "fn main() {",
            "    put(减(10, 3))",
            "    put(减(b: 3, a: 10))",
            "    put(减(10, b: 3))",
            "}",
        ],
    );
}

// ============================================================
// 35h. 优先级3：默认参数
// ============================================================

#[test]
fn default_args_matches() {
    assert_consistent_src(
        "default_args",
        &[
            "fn 加(a: int, b: int = 5) -> int { a + b }",
            "fn 三(a: int = 1, b: int = 2, c: int = 3) -> int { a + b + c }",
            "fn main() {",
            "    put(加(1))",
            "    put(加(1, 2))",
            "    put(三())",
            "    put(三(10))",
            "    put(三(10, 20))",
            "}",
        ],
    );
}

// ============================================================
// 35g. 优先级3：match 范围模式
// ============================================================

#[test]
fn match_range_matches() {
    assert_consistent_src(
        "match_range",
        &[
            "fn main() {",
            "    for x in 0..15 {",
            "        r := match x {",
            "            0..5 => { \"low\" }",
            "            5..10 => { \"mid\" }",
            "            _ => { \"high\" }",
            "        }",
            "        put(r)",
            "    }",
            "}",
        ],
    );
}

// ============================================================
// 35f. 优先级2：do-while / for-else / 标签循环
// ============================================================

#[test]
fn control_flow_ext_matches() {
    assert_consistent_src(
        "cf_ext",
        &[
            "fn main() {",
            "    i := 0",
            "    do {",
            "        put(i)",
            "        i++",
            "    } while i < 3",
            "    for k in 0..3 {",
            "        put(\"k=${k}\")",
            "    } else {",
            "        put(\"no break\")",
            "    }",
            "    outer: for a in 0..3 {",
            "        for b in 0..3 {",
            "            if b == 1 { break outer }",
            "            put(a * 10 + b)",
            "        }",
            "    }",
            "    put(\"done\")",
            "}",
        ],
    );
}

// ============================================================
// 35e. 新增语法糖：++/--、成员 in、下标 m[..]/l[..]
// ============================================================

#[test]
fn syntax_sugar_matches() {
    assert_consistent_src(
        "syntax_sugar",
        &[
            "fn main() {",
            "    x := 1",
            "    x++",
            "    x++",
            "    put(x)",
            "    x--",
            "    put(x)",
            "    l := list()",
            "    push(l, 10)",
            "    push(l, 20)",
            "    put(l[0])",
            "    put(l[1])",
            "    l[0] = 99",
            "    put(l[0])",
            "    if 20 in l { put(\"has20\") }",
            "    if 5 in l { put(\"has5\") } else { put(\"no5\") }",
            "    m := map()",
            "    m[\"a\"] = 1",
            "    m[\"b\"] = 2",
            "    put(m[\"a\"] + m[\"b\"])",
            "}",
        ],
    );
}

// ============================================================
// 35d. 溢出不被常量折叠绕过（安全回归）
// ============================================================

#[test]
fn overflow_not_folded_away() {
    // i64::MAX + 1 必须在运行时被拦截（不能常量折叠成回绕值）
    let p = tmp_dir().join("ovf_not_folded.gt");
    std::fs::write(&p, "fn main() {\n    x := 9223372036854775807\n    put(x + 1)\n}\n").unwrap();
    // 解释器
    let interp = Command::new(gtc()).arg("--run").arg(&p).arg("zh").output().expect("无法启动 gtc --run");
    assert!(!interp.status.success(), "解释器未拦下溢出");
    let ie = decode(&interp.stderr) + &decode(&interp.stdout);
    assert!(ie.contains("溢出"), "解释器缺诊断：\n{}", ie);
    // 编译器
    let exe = tmp_dir().join("ovf_not_folded.exe");
    let c = Command::new(gtc()).arg("--c").arg(&p).arg("-o").arg(&exe).arg("zh").output().expect("无法启动 gtc --c");
    assert!(c.status.success(), "编译失败：\n{}", decode(&c.stderr));
    let ran = Command::new(&exe).output().expect("无法运行产物");
    assert!(!ran.status.success(), "编译产物未拦下溢出");
    let ce = decode(&ran.stderr) + &decode(&ran.stdout);
    assert!(ce.contains("溢出"), "编译产物缺诊断：\n{}", ce);
}

// ============================================================
// 35c. 范围分析：安全的加法省检查，危险的不省
// ============================================================

#[test]
fn range_analysis_safe_loop() {
    // for i in 0..N + 单调累积：范围分析可证不溢出，省检查（结果仍正确）
    assert_consistent_src(
        "range_safe",
        &[
            "fn main() {",
            "    s := 0",
            "    for i in 0..1000000 {",
            "        s = s + i",
            "    }",
            "    put(s)",
            "}",
        ],
    );
}

// ============================================================
// 35b. 用户函数遮蔽同名标准库函数
// ============================================================

#[test]
fn user_fn_shadows_stdlib() {
    // 用户定义 `fib`（递归，O(2^n)）；调用应命中用户函数而非标准库 py_fib（迭代）
    assert_consistent_src(
        "shadow_stdlib",
        &[
            "fn fib(n: int) -> int {",
            "    if n < 2 { return n }",
            "    fib(n-1) + fib(n-2)",
            "}",
            "fn main() {",
            "    put(fib(10))",
            "    put(fib(20))",
            "}",
        ],
    );
}

// ============================================================
// 36. try / expt / fily 异常捕获（双后端一致）
// ============================================================

#[test]
fn try_expt_matches() {
    assert_consistent_src(
        "try_expt",
        &[
            "fn 可能失败(x: int) -> Result[int, str] {",
            "    if x < 0 { return Err(\"neg\") }",
            "    Ok(x * 2)",
            "}",
            "fn main() {",
            "    try {",
            "        v := 可能失败(5)?",
            "        put(\"v=${v}\")",
            "    } expt e {",
            "        put(\"caught\")",
            "    }",
            "    try {",
            "        w := 可能失败(-1)?",
            "        put(\"w=${w}\")",
            "    } expt e {",
            "        put(\"caught2\")",
            "    }",
            "    put(\"after\")",
            "}",
        ],
    );
}

#[test]
fn try_throw_matches() {
    assert_consistent_src(
        "try_throw",
        &[
            "fn main() {",
            "    try {",
            "        throw 42",
            "    } expt e {",
            "        put(\"caught\")",
            "    }",
            "    put(\"done\")",
            "}",
        ],
    );
}















// ============================================================
// 9. 标准库（crypto / entropy / session / sql / string）
// ============================================================

#[test]
fn stdlib_crypto_hashes_match() {
    assert_consistent_src(
        "stdlib_crypto",
        &[
            "import crypto",
            "fn main() {",
            "    put(sha256(\"abc\"))",
            "    put(sha512(\"abc\"))",
            "    put(sha1(\"abc\"))",
            "    put(md5(\"abc\"))",
            "    put(hmac_sha256(\"key\", \"msg\"))",
            "    put(hex_encode(\"AB\"))",
            "    put(hex_decode(\"4142\"))",
            "    h := password_hash(\"pw\", \"salt\")",
            "    put(password_verify(\"pw\", h))",
            "    put(password_verify(\"bad\", h))",
            "}",
        ],
    );
}

#[test]
fn stdlib_entropy_shapes_match() {
    assert_consistent_src(
        "stdlib_entropy",
        &[
            "import entropy",
            "fn main() {",
            "    put(len(entropy_random_hex(16)))",
            "    put(len(entropy_uuid()))",
            "    put(entropy_random_int(1))",
            "    put(entropy_random_int(10) < 10)",
            "    put(entropy_random_int(10) >= 0)",
            "}",
        ],
    );
}

#[test]
fn stdlib_session_flow_matches() {
    assert_consistent_src(
        "stdlib_session",
        &[
            "import sql",
            "import session",
            "fn main() {",
            "    db := sql_open(\":memory:\")",
            "    sid := session_create(db, \"alice\", 3600)",
            "    put(len(sid))",
            "    put(session_get(db, sid))",
            "    put(session_count(db))",
            "    session_destroy(db, sid)",
            "    put(session_count(db))",
            "    sql_close(db)",
            "}",
        ],
    );
}

#[test]
fn stdlib_string_ops_match() {
    assert_consistent_src(
        "stdlib_string2",
        &[
            "import string",
            "fn main() {",
            "    put(strip(\"  hi  \", \"\"))",
            "    put(index(\"hello\", \"ll\"))",
            "    put(replace_all(\"a-b-c\", \"-\", \"+\"))",
            "    put(contains(\"hello\", \"ell\"))",
            "    put(utf8_len(\"你好\"))",
            "}",
        ],
    );
}
#[test]
fn stdlib_sql_crud_matches() {
    assert_consistent_src(
        "stdlib_sql",
        &[
            "import sql",
            "fn main() {",
            "    db := sql_open(\":memory:\")",
            "    put(sql_exec(db, \"CREATE TABLE t (id INTEGER, name TEXT)\"))",
            "    put(sql_exec_many(db, \"INSERT INTO t VALUES (1, 'a')\\nINSERT INTO t VALUES (2, 'b')\"))",
            "    put(sql_query(db, \"SELECT COUNT(*) FROM t\"))",
            "    sql_close(db)",
            "}",
        ],
    );
}

// ============================================================
// 36. 函数作一等值 / 闭包（v0.0.1d）
// ============================================================

#[test]
fn first_class_fn_stored_and_called() {
    assert_consistent_src(
        "fn_first_class",
        &[
            "fn 加一(n: int) -> int { return n + 1 }",
            "fn 乘二(n: int) -> int { return n * 2 }",
            "fn main() {",
            "    g := 加一",
            "    put(g(10))",
            "    fs := list()",
            "    push(fs, 加一)",
            "    push(fs, 乘二)",
            "    put(fs[0](5))",
            "    put(fs[1](5))",
            "}",
        ],
    );
}

#[test]
fn first_class_fn_as_higher_order_arg() {
    assert_consistent_src(
        "fn_higher_order",
        &[
            "fn 加一(n: int) -> int { return n + 1 }",
            "fn 乘二(n: int) -> int { return n * 2 }",
            "fn 应用(f, x: int) -> int { return f(x) }",
            "fn 组合(f, g, x: int) -> int { return g(f(x)) }",
            "fn main() {",
            "    put(应用(加一, 10))",
            "    put(组合(加一, 乘二, 5))",
            "}",
        ],
    );
}

#[test]
fn first_class_fn_from_if_and_match() {
    assert_consistent_src(
        "fn_from_if_match",
        &[
            "fn 加一(n: int) -> int { return n + 1 }",
            "fn 乘二(n: int) -> int { return n * 2 }",
            "fn 选(f: int) { if f == 1 { return 加一 }  return 乘二 }",
            "fn main() {",
            "    h := if true { 加一 } else { 乘二 }",
            "    put(h(100))",
            "    h2 := match 2 { 1 => { 加一 }  _ => { 乘二 } }",
            "    put(h2(10))",
            "    g := 选(1)",
            "    put(g(5))",
            "}",
        ],
    );
}

#[test]
fn closure_returning_closure_matches() {
    assert_consistent_src(
        "closure_factory",
        &[
            "fn main() {",
            "    造乘 := |n: int| |x: int| x * n",
            "    m3 := 造乘(3)",
            "    m5 := 造乘(5)",
            "    put(m3(10))",
            "    put(m5(10))",
            "    外 := |a: int| |b: int| a + b",
            "    add2 := 外(2)",
            "    put(add2(3))",
            "}",
        ],
    );
}

#[test]
fn closure_capturing_container_matches() {
    assert_consistent_src(
        "closure_capture_container",
        &[
            "fn main() {",
            "    xs := list()",
            "    push(xs, 1)",
            "    push(xs, 2)",
            "    g := |x: int| len(xs) + x",
            "    put(g(10))",
            "    s := \"hello\"",
            "    h := |x: int| len(s) + x",
            "    put(h(0))",
            "}",
        ],
    );
}

#[test]
fn closure_compose_and_fold_matches() {
    assert_consistent_src(
        "closure_compose_fold",
        &[
            "fn 复合(f, g) { return |x: int| g(f(x)) }",
            "fn main() {",
            "    inc := |x: int| x + 1",
            "    dbl := |x: int| x * 2",
            "    h := 复合(inc, dbl)",
            "    put(h(5))",
            "    h2 := 复合(dbl, inc)",
            "    put(h2(5))",
            "}",
        ],
    );
}

// ============================================================
// 36b. 诊断渲染不 panic（畸形/截断源码）
// ============================================================

#[test]
fn truncated_multibyte_source_does_not_panic() {
    // 源码被截断在多字节 UTF-8 字符中间 → 诊断渲染不得 panic（回归）
    // "计数器" 的 UTF-8 是 E8 AE A1 E6 95 B0 E5 99 A8；这里截断在第 2 字节中间
    let mut bytes: Vec<u8> = b"fn main() {\n  ".to_vec(); bytes.extend_from_slice(&[0xE8, 0xAE]); // truncate mid-multibyte
    bytes.push(b'\n');
    let p = tmp_dir().join("truncated_utf8.gt");
    std::fs::write(&p, &bytes).unwrap();
    let out = Command::new(gtc()).arg("--check").arg(&p).output().expect("gtc");
    let err = decode(&out.stderr) + &decode(&out.stdout);
    assert!(!err.contains("panicked"), "diagnostic rendering panicked:\n{}", err);
    assert!(!err.contains("char boundary"), "char boundary panic:\n{}", err);
}

// ============================================================
// 36c. 综合组合（v0.0.1d）
// ============================================================

#[test]
fn combo_generics_closures_oo_result_matches() {
    assert_consistent_src(
        "combo_all",
        &[
            "fn 恒等[T](x: T) -> T { return x }",
            "fn 映射(f, xs: list) -> list { out := list()  i := 0  while i < len(xs) { push(out, f(xs[i]))  i = i + 1 }  return out }",
            "fn 安全除(a: int, b: int) { if b == 0 { return Err(1) }  return Ok(a / b) }",
            "struct 盒 { v: int }",
            "impl 盒 { fn 取(self) -> int { return self.v } }",
            "enum 树 { 叶(int) 枝(树, 树) }",
            "fn 深度(t: 树) -> int { match t { 树::叶(v) => { return 1 }  树::枝(l, r) => { return 1 + 深度(l) + 深度(r) } } }",
            "fn main() {",
            "    加倍 := |x: int| x * 2",
            "    xs := list()  i := 0  while i < 5 { push(xs, i)  i = i + 1 }",
            "    ys := 映射(加倍, xs)",
            "    put(ys[4])",
            "    put(恒等(42))",
            "    put(恒等(\"hi\"))",
            "    b := 盒 { v: 7 }  put(b.取())",
            "    t := 树::枝(树::叶(1), 树::叶(2))  put(深度(t))",
            "    r := 安全除(10, 2)  match r { Ok(v) => { put(v) }  Err(e) => { put(-1) } }",
            "    r2 := 安全除(10, 0)  match r2 { Ok(v) => { put(v) }  Err(e) => { put(-2) } }",
            "}",
        ],
    );
}

#[test]
fn combo_concurrency_matches() {
    assert_consistent_src(
        "combo_conc",
        &[
            "fn 累加(n: int) -> int { s := 0  i := 0  while i < n { s = s + i  i = i + 1 }  return s }",
            "fn 工人(c, n: int) { chan_send(c, 累加(n)) }",
            "fn main() {",
            "    c := chan()",
            "    i := 0",
            "    while i < 10 { go 工人(c, i)  i = i + 1 }",
            "    total := 0  j := 0",
            "    while j < 10 { total = total + chan_recv(c)  j = j + 1 }",
            "    put(total)",
            "}",
        ],
    );
}

#[test]
fn combo_stdlib_closures_matches() {
    assert_consistent_src(
        "combo_stdlib",
        &[
            "import math",
            "import string",
            "fn 变换(f, xs: list) -> list { out := list()  i := 0  while i < len(xs) { push(out, f(xs[i]))  i = i + 1 }  return out }",
            "fn main() {",
            "    xs := list()  i := 1  while i <= 5 { push(xs, i)  i = i + 1 }",
            "    平方 := |x: int| x * x",
            "    ys := 变换(平方, xs)",
            "    put(ys[0] + ys[1] + ys[2] + ys[3] + ys[4])",
            "    put(gcd(48, 36))",
            "    put(isprime(17))",
            "    put(factorial(5))",
            "    put(capitalize(\"hello\"))",
            "    put(utf8_len(\"你好\"))",
            "}",
        ],
    );
}

/// 运行时错误源码：双端都必须**非零退出**且错误文本逐字节一致（不 panic）。
fn assert_runtime_error_matches(tag: &str, src_text: &str) {
    let p = tmp_dir().join(format!("rt_{}.gt", tag));
    std::fs::write(&p, src_text).expect("无法写临时源文件");

    let interp = Command::new(gtc())
        .arg("--run").arg(&p)
        .output().expect("无法启动 gtc --run");
    assert!(!interp.status.success(), "[{}] 解释器未报运行时错误", tag);
    let ie = decode(&interp.stderr) + &decode(&interp.stdout);
    assert!(!ie.contains("panicked"), "[{}] 解释器 panic：
{}", tag, ie);

    let exe = tmp_dir().join(format!("rt_{}.exe", tag));
    let c = Command::new(gtc())
        .arg("--c").arg(&p).arg("-o").arg(&exe)
        .output().expect("无法启动 gtc --c");
    assert!(c.status.success(), "[{}] 编译失败：
{}", tag, decode(&c.stderr));
    let ran = Command::new(&exe).output().expect("无法运行产物");
    assert!(!ran.status.success(), "[{}] 编译产物未报运行时错误", tag);
    let ce = decode(&ran.stderr) + &decode(&ran.stdout);

    assert_eq!(ie, ce, "[{}] 双端运行时错误不一致", tag);
}

#[test]
fn ref_field_access_autoderef_matches() {
    // 借用自动解引用：a := &p; a.x 视作 p.x（sema + JIT + LLVM）
    assert_consistent_src(
        "ref_autoderef",
        &[
            "struct P { x: int  y: int }",
            "fn main() {",
            "    p := P { x: 42, y: 7 }",
            "    a := &p",
            "    put(a.x)",
            "    put(a.x + a.y)",
            "    m := &mut p",
            "    m.x = 100",
            "    put(p.x)",
            "}",
        ],
    );
}

#[test]
fn match_binding_shadowing_outer() {
    // 回归：match 绑定名与外层参数同名（Err(e) 而外层也有 e）不应误报 use-after-move。
    assert_consistent_src(
        "match_shadow",
        &[
            "enum E { A(Result[int, str]) C }",
            "fn f(e: E) -> int { match e { E::A(Ok(v)) => { return v } E::A(Err(e)) => { return -2 } E::C => { return 0 } } }",
            "fn main() { put(f(E::A(Ok(7))))  put(f(E::A(Err(\"z\"))))  put(f(E::C)) }",
        ],
    );
}

#[test]
fn enum_payload_destructuring_matches() {
    // enum 载荷本身是解构模式（E::A(Some(v))）：sema/JIT/LLVM 都需递归绑定。
    assert_consistent_src(
        "enum_payload_destr",
        &[
            "enum E { A(Option[int]) B }",
            "fn f(e: E) -> int { match e { E::A(Some(v)) => { return v } E::A(None) => { return -1 } E::B => { return 0 } } }",
            "fn main() { put(f(E::A(Some(5))))  put(f(E::A(None)))  put(f(E::B)) }",
        ],
    );
}

#[test]
fn composite_match_pattern_rejected() {
    // 复合类型（元组）不能作 match 模式：--run 与 --c 都应明确拒绝
    // （不生成非法 IR、不静默走 _）。该错误在代码生成阶段，--check 不报。
    let p = tmp_dir().join("composite_pat.gt");
    std::fs::write(&p, "fn main() { t := (1, 2)  match t { (1, 2) => { put(10) } _ => { put(0) } } }\n").unwrap();

    let jit = Command::new(gtc()).arg("--run").arg(&p).output().expect("gtc --run");
    assert!(!jit.status.success(), "--run 未拒绝复合模式");
    let je = decode(&jit.stderr) + &decode(&jit.stdout);
    assert!(je.contains("composite"), "--run 缺少提示：\n{}", je);

    let exe = tmp_dir().join("composite_pat.exe");
    let aot = Command::new(gtc()).arg("--c").arg(&p).arg("-o").arg(&exe).output().expect("gtc --c");
    assert!(!aot.status.success(), "--c 未拒绝复合模式");
    let ae = decode(&aot.stderr) + &decode(&aot.stdout);
    assert!(ae.contains("composite"), "--c 缺少提示：\n{}", ae);
}

#[test]
fn nested_destructuring_matches() {
    // 嵌套解构：Some(Some(v)) / Ok(Some(v)) / enum 多载荷
    assert_consistent_src(
        "match_nested",
        &[
            "enum T { 叶(int) 枝(T, T) }",
            "fn 深(t: T) -> int { match t { T::叶(v) => { return v } T::枝(l, r) => { return 1 + 深(l) + 深(r) } } }",
            "fn main() {",
            "    o := Some(Some(5))",
            "    match o { Some(Some(v)) => { put(v) } _ => { put(0) } }",
            "    r := Ok(Some(3))",
            "    match r { Ok(Some(v)) => { put(v) } _ => { put(0) } }",
            "    put(深(T::枝(T::叶(1), T::叶(2))))",
            "}",
        ],
    );
}

#[test]
fn match_ident_binding_matches() {
    // 裸标识符模式绑定主体值（文档 "n if n > 0 =>" 的写法）
    assert_consistent_src(
        "match_bind",
        &[
            "fn main() {",
            "    v := 5",
            "    match v { n if n > 0 => { put(n) } _ => { put(0) } }",
            "    match v { n if n < 0 => { put(n) } _ => { put(9) } }",
            "}",
        ],
    );
}

#[test]
fn empty_match_is_rejected() {
    // 回归：空 match（无分支）曾被认为"穷尽"而通过检查。
    assert_rejected("empty_match", &["fn main() { match 1 { }  put(1) }"], "穷尽");
}

#[test]
fn non_enum_match_needs_wildcard() {
    // 整数主体的 match 必须有 _ 兜底（字面量永远无法穷尽）。
    assert_rejected("match_no_wild", &["fn main() { match 1 { 1 => { put(1) } } }"], "穷尽");
}

#[test]
fn if_stmt_same_line_as_prev_stmt() {
    // 回归：`a := 1  if a > 0 { ... }`（if 与前一语句同行）曾被误判为三元，报"缺 else"。
    assert_consistent_src(
        "if_same_line",
        &["fn main() { a := 1  if a > 0 { put(1) } }"],
    );
}

#[test]
fn elif_chain_matches() {
    // 回归：elif 条件曾被当成结构体字面量/三元，整条 if/elif/else 不可用。
    assert_consistent_src(
        "elif_chain",
        &[
            "fn main() {",
            "    a := 1  b := 2",
            "    if a > b { put(1) } elif a < b { put(-1) } else { put(0) }",
            "    if a < b { put(2) } elif a > b { put(3) } else { put(4) }",
            "}",
        ],
    );
}

#[test]
fn single_expr_interpolation_is_string() {
    // 回归："${x}"（整串只有一个插值）曾被优化成 x 本身，导致类型/值错误。
    assert_consistent_src(
        "interp_single",
        &[
            "struct 点 { x: int }",
            "impl 点 { fn to_str(self) -> str { return \"v=${self.x}\" } }",
            "fn main() {",
            "    x := 42",
            "    put(\"${x}\")",
            "    s := \"${x}\"",
            "    put(len(s))",
            "    p := 点 { x: 7 }",
            "    put(p.to_str())",
            "}",
        ],
    );
}

#[test]
fn option_or_default_matches() {
    // `x or y`：Some(v) 取 v，None 取 y
    assert_consistent_src(
        "or_default",
        &[
            "fn f(n: int) { if n < 0 { return None }  return Some(n * 2) }",
            "fn main() {",
            "    put(f(3) or 0)",
            "    put(f(-1) or 99)",
            "    v := f(0) or 7",
            "    put(v)",
            "    o := Some(5)",
            "    put(o or 1)",
            "    n := None",
            "    put(n or 42)",
            "}",
        ],
    );
}

#[test]
fn pipe_to_non_callable_is_rejected() {
    // 回归：`i |> i + 1` 过去会静默丢掉 lhs（等价于 i+1），造成无声的错值/死循环。
    // 现在应报"不可调用"。
    assert_rejected("pipe_noncallable", &["fn main() { i := 0  i |> i + 1  put(i) }"], "callable");
}

#[test]
fn pipe_to_function_still_works() {
    assert_consistent_src(
        "pipe_ok",
        &[
            "fn double(x: int) -> int { return x * 2 }",
            "fn main() {",
            "    put(5 |> double)",
            "    xs := list()  push(xs, 1)  push(xs, 2)",
            "    put(xs |> len)",
            "    put(3 |> double |> double)",
            "}",
        ],
    );
}

#[test]
fn runtime_errors_match() {
    // 溢出 / 除零 / 越界：双端给出相同的运行时错误（非 panic）
    assert_runtime_error_matches("overflow", "fn main() { a := 9223372036854775807  put(a + 1) }");
    assert_runtime_error_matches("div0", "fn main() { a := 1  b := 0  put(a / b) }");
    assert_runtime_error_matches("index", "fn main() { xs := list()  push(xs, 1)  put(xs[100]) }");
}

#[test]
fn edge_sources_do_not_panic() {
    // 空文件 / 仅注释 / 仅空白：不得 panic（诊断可报错）
    for (tag, text) in [
        ("empty", ""),
        ("comment", "// 只有注释"),
        ("ws", "   \n\t\n"),
    ] {
        let p = tmp_dir().join(format!("edge_{}.gt", tag));
        std::fs::write(&p, text).unwrap();
        let out = Command::new(gtc())
            .arg("--check").arg(&p)
            .output().expect("无法启动 gtc --check");
        let err = decode(&out.stderr) + &decode(&out.stdout);
        assert!(!err.contains("panicked"), "[{}] panic：
{}", tag, err);
        assert!(!err.contains("char boundary"), "[{}] char boundary panic：
{}", tag, err);
    }
}

// ============================================================
// 37. 深嵌套 / 边界（v0.0.1d 加固）
// ============================================================

#[test]
fn i64_min_literal_matches() {
    assert_consistent_src(
        "i64_min",
        &[
            "fn main() {",
            "    put(-9223372036854775808)",
            "    put(9223372036854775807)",
            "}",
        ],
    );
}

#[test]
fn deeply_nested_type_is_rejected() {
    let n = 3200;
    let ty = "list[".repeat(n) + "int" + &"]".repeat(n);
    let lines = [format!("struct S {{ f: {} }}", ty), "fn main() { put(1) }".to_string()];
    let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    assert_rejected("deep_type", &refs, "过深");
}

#[test]
fn long_binary_chain_is_rejected() {
    let body = "1 + ".repeat(2000) + "1";
    let lines = [format!("fn main() {{ x := {}  put(x) }}", body)];
    let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    assert_rejected("long_chain", &refs, "过深");
}

#[test]
fn deeply_nested_if_is_rejected() {
    // 500 层嵌套 if 应报"嵌套过深"而非爆栈（回归）
    let n = 500;
    let src = format!("fn main() {{ {}put(1) {} }}", "if true { ".repeat(n), "} ".repeat(n));
    let p = tmp_dir().join("deep_if.gt");
    std::fs::write(&p, &src).unwrap();
    assert_rejected("deep_if", &[src.as_str()], "过深");
}

#[test]
fn loop_without_count_is_rejected() {
    assert_rejected(
        "loop_no_count",
        &[
            "fn main() {",
            "    loop { put(1) }",
            "}",
        ],
        "循环次数",
    );
}

// ============================================================
// 38. 并发 / 通道（v0.0.1d）
// ============================================================

#[test]
fn go_thread_pool_matches() {
    assert_consistent_src(
        "go_pool",
        &[
            "fn 生产者(c, n: int) { chan_send(c, n * n) }",
            "fn main() {",
            "    c := chan()",
            "    i := 0",
            "    while i < 20 {",
            "        go 生产者(c, i)",
            "        i = i + 1",
            "    }",
            "    total := 0",
            "    j := 0",
            "    while j < 20 { total = total + chan_recv(c)  j = j + 1 }",
            "    put(total)",
            "}",
        ],
    );
}

// ============================================================
// 39. 泛型（v0.0.1d）
// ============================================================

#[test]
fn generic_multi_instantiation_matches() {
    assert_consistent_src(
        "generic_multi_inst",
        &[
            "fn 恒等[T](x: T) -> T { return x }",
            "fn main() {",
            "    put(恒等(42))",
            "    put(恒等(\"hi\"))",
            "    put(恒等(3.5))",
            "}",
        ],
    );
}

#[test]
fn generic_list_element_matches() {
    assert_consistent_src(
        "generic_list_elem",
        &[
            "fn 首[T](xs: list[T]) -> T { return xs[0] }",
            "fn main() {",
            "    a := list()",
            "    push(a, 42)",
            "    put(首(a))",
            "    b := list()",
            "    push(b, \"hi\")",
            "    put(首(b))",
            "}",
        ],
    );
}

// ============================================================
// 40. 错误处理（v0.0.1d）
// ============================================================

#[test]
fn result_with_str_payload_early_return_matches() {
    // 回归：fn 无标注返回类型，两个 return 的 Result 载荷需逐字段合并
    // （Ok 载荷 i64 + Err 载荷 str），否则 match 崩
    assert_consistent_src(
        "result_str_payload",
        &[
            "fn f(b: int) {",
            "    if b == 0 { return Err(\"x\") }",
            "    return Ok(1)",
            "}",
            "fn main() {",
            "    r := f(10)",
            "    match r { Ok(v) => { put(v) }  Err(e) => { put(0) } }",
            "    r2 := f(0)",
            "    match r2 { Ok(v) => { put(v) }  Err(e) => { put(-1) } }",
            "}",
        ],
    );
}

#[test]
fn closure_with_result_matches() {
    // 注意：Err 载荷用整数（字符串载荷的 Result match 目前会崩，见 known issue）
    assert_consistent_src(
        "closure_result",
        &[
            "fn 安全除(a: int, b: int) {",
            "    if b == 0 { return Err(\"div0\") }",
            "    return Ok(a / b)",
            "}",
            "fn main() {",
            "    除 := |d: int| 安全除(100, d)",
            "    r1 := 除(10)",
            "    match r1 { Ok(v) => { put(v) }  Err(e) => { put(-1) } }",
            "    r2 := 除(0)",
            "    match r2 { Ok(v) => { put(v) }  Err(e) => { put(-2) } }",
            "}",
        ],
    );
}

// ============================================================
// 41. 字符串 / 数值边界（v0.0.1d）
// ============================================================

#[test]
fn string_and_num_edge_matches() {
    assert_consistent_src(
        "str_num_edge",
        &[
            "fn main() {",
            "    put(len(str(123)))",
            "    put(str(1.5))",
            "    put(int(\"-9223372036854775808\"))",
            "    put(1.0 / 0.0)",
            "    put(0.0 / 0.0)",
            "    s := \"\"",
            "    i := 0",
            "    while i < 100 { s = s + \"x\"  i = i + 1 }",
            "    put(len(s))",
            "}",
        ],
    );
}