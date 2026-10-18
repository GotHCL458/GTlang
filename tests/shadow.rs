//! 变量遮蔽（shadowing）专项测试。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("sh_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
}
fn run(src: &str, opt: u8) -> String {
    let d = tmp(); std::fs::create_dir_all(&d).unwrap();
    let g = d.join("t.gt"); std::fs::write(&g, src).unwrap();
    let exe = d.join("t.exe");
    let c = Command::new(gtc()).arg("--c").arg(&g).arg("-o").arg(&exe).arg("-O").arg(opt.to_string()).output().unwrap();
    assert!(c.status.success(), "compile O{} failed: {}", opt, String::from_utf8_lossy(&c.stderr));
    let stdlib = Path::new(env!("CARGO_MANIFEST_DIR")).join("res").join("lib");
    let r = Command::new(&exe).env("GTC_STDLIB_DIR", &stdlib).output().unwrap();
    String::from_utf8_lossy(&r.stdout).into_owned()
}
fn eq(src: &str, expected: &str) {
    for o in [0u8, 1, 2, 3, 4] {
        let out = run(src, o);
        assert_eq!(out.trim(), expected, "-O{} mismatch:\n{}", o, src);
    }
}

#[test] fn shadow_if_block() {
    eq("fn main() { x := 1 if true { x := 2 put(x) } put(x) }\n", "2\n1");
}
#[test] fn shadow_else_block() {
    eq("fn main() { x := 1 if false { } else { x := 2 put(x) } put(x) }\n", "2\n1");
}
#[test] fn shadow_for() {
    eq("fn main() { n := 5 for i in 0..3 { n := i * 10 put(n) } put(n) }\n", "0\n10\n20\n5");
}
#[test] fn shadow_while() {
    eq("fn main() { x := 100 i := 0 while i < 2 { x := i put(x) i = i + 1 } put(x) }\n", "0\n1\n100");
}
#[test] fn shadow_loop_var_name() {
    eq("fn main() { i := 99 for i in 0..3 { put(i) } put(i) }\n", "0\n1\n2\n99");
}
#[test] fn shadow_chain_same_block() {
    eq("fn main() { x := 1 x := x + 1 x := x + 1 put(x) }\n", "3");
}
#[test] fn shadow_const_vs_mut() {
    eq("fn main() { v := 10 for k in 0..2 { v := k + 100 put(v) } put(v) }\n", "100\n101\n10");
}
#[test] fn shadow_nested_if() {
    // if 块有独立作用域；嵌套 if 内层 shadow 外层。
    eq("fn main() { a := 1 if true { a := 2 if true { a := 3 put(a) } put(a) } put(a) }\n", "3\n2\n1");
}

#[test] fn bare_block_not_isolated() {
    // 裸 { } 块不引入新作用域（解构依赖），变量覆盖外层——设计限制。
    eq("fn main() { a := 1 { a := 2 put(a) } put(a) }\n", "2\n2");
}
#[test] fn shadow_loop_accumulator() {
    eq("fn main() { s := 100 for i in 0..3 { s := i put(s) } put(s) }\n", "0\n1\n2\n100");
}
