//! 剩余方向：set/loop N/match 范围/?-in-try/defer-in-try/string 模块。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("rs_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn set_insert_vs_push() {
    let out = run("fn main() { s := set() insert(s, 1) insert(s, 2) insert(s, 1) put(len(s)) put(has(s, 2)) put(has(s, 9)) }\n");
    assert_eq!(out.trim(), "2\ntrue\nfalse");
}
#[test] fn loop_n_variable() {
    let out = run("fn main() { n := 3 loop n { put(1) } }\n");
    assert_eq!(out.trim(), "1\n1\n1");
}
#[test] fn match_range_bounds() {
    let out = run("fn f(x: int) -> int { match x { 1..4 => { 10 } 4..7 => { 20 } _ => { 0 } } }\nfn main() { put(f(1)) put(f(3)) put(f(4)) put(f(6)) put(f(7)) put(f(0)) }\n");
    assert_eq!(out.trim(), "10\n10\n20\n20\n0\n0");
}
#[test] fn question_in_try() {
    let out = run("fn g() -> Result[int, str] { Ok(5) }\nfn f() -> int { try { x := g()? put(x) } expt e { put(-1) } return 0 }\nfn main() { f() }\n");
    assert_eq!(out.trim(), "5");
}
#[test] fn defer_in_try() {
    let out = run("fn main() { try { defer put(\"d\") put(\"b\") } expt e { } }\n");
    assert_eq!(out.trim(), "b\nd");
}
#[test] fn str_find_starts_strip() { let out = run("import string\nfn main() { put(find(\"hello\", \"ll\")) put(startswith(\"hello\", \"he\")) put(strip(\"  x  \", \"\")) }\n"); assert_eq!(out.trim(), "2\ntrue\nx"); }


