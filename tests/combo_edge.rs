//! 控制流/表达式组合边缘。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("cx_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
}
fn run(src: &str) -> String {
    let d = tmp(); std::fs::create_dir_all(&d).unwrap();
    let g = d.join("t.gt"); std::fs::write(&g, src).unwrap();
    let exe = d.join("t.exe");
    let c = Command::new(gtc()).arg("--c").arg(&g).arg("-o").arg(&exe).arg("-O").arg("2").output().unwrap();
    assert!(c.status.success(), "compile failed: {}", String::from_utf8_lossy(&c.stderr));
    let stdlib = Path::new(env!("CARGO_MANIFEST_DIR")).join("res").join("lib");
    let r = Command::new(&exe).env("GTC_STDLIB_DIR", &stdlib).output().unwrap();
    String::from_utf8_lossy(&r.stdout).into_owned()
}

#[test] fn match_guard() { let out = run("fn main() { x := 7 y := match x { n if n > 5 => { 100 } _ => { 0 } } put(y) }\n"); assert_eq!(out.trim(), "100"); }
#[test] fn match_or_pattern() { let out = run("fn main() { x := 2 y := match x { 1 | 2 => { 10 } _ => { 0 } } put(y) }\n"); assert_eq!(out.trim(), "10"); }
#[test] fn try_return() { let out = run("fn f() -> int { try { return 5 } expt e { return 0 } }\nfn main() { put(f()) }\n"); assert_eq!(out.trim(), "5"); }
#[test] fn for_else_break() { let out = run("fn main() { for i in 0..5 { if i == 2 { break } } else { put(\"no break\") } put(\"end\") }\n"); assert_eq!(out.trim(), "end"); }
#[test] fn const_propagate() { let out = run("const N = 10\nfn main() { put(N * 2) }\n"); assert_eq!(out.trim(), "20"); }
#[test] fn nested_if_expr() { let out = run("fn main() { x := 5 y := if x > 0 { if x > 3 { 10 } else { 5 } } else { 0 } put(y) }\n"); assert_eq!(out.trim(), "10"); }
#[test] fn while_sum() { let out = run("fn main() { i := 0 s := 0 while i < 3 { s = s + i i = i + 1 } put(s) }\n"); assert_eq!(out.trim(), "3"); }
