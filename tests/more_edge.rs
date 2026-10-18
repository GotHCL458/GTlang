//! 更多边缘。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("mx_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn nested_for_else() { let out = run("fn main() { for i in 0..2 { for j in 0..2 { put(i) } else { put(9) } } }\n"); assert_eq!(out.trim(), "0\n0\n9\n1\n1\n9"); }
#[test] fn match_nested() { let out = run("fn main() { x := 2 y := 3 r := match x { 1 => { 10 } 2 => { match y { 3 => { 30 } _ => { 0 } } } _ => { 0 } } put(r) }\n"); assert_eq!(out.trim(), "30"); }
#[test] fn range_empty() { let out = run("fn main() { s := 0 for i in 5..5 { s = s + i } put(s) }\n"); assert_eq!(out.trim(), "0"); }
#[test] fn recursion_mutual() { let out = run("fn even(n: int) -> bool { if n == 0 { return true } odd(n - 1) }\nfn odd(n: int) -> bool { if n == 0 { return false } even(n - 1) }\nfn main() { put(even(10)) put(odd(10)) }\n"); assert_eq!(out.trim(), "true\nfalse"); }
#[test] fn shadowing() { let out = run("fn main() { x := 1 if true { x := 2 put(x) } put(x) }\n"); assert_eq!(out.trim(), "2\n1"); }
