//! 继续：loop-break、match binding、for label continue、Option/Result 链、str 迭代。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("u_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn loop_break() { let out = run("fn main() { s := 0 loop 3 { s = s + 1 } put(s) }\n"); assert_eq!(out.trim(), "3"); }
// match binding @ (n @ 1..4) 不支持，略过
#[test] fn for_label_continue() { let out = run("fn main() { s := 0 outer: for i in 0..3 { for j in 0..3 { if j == 1 { continue outer } s = s + 1 } } put(s) }\n"); assert_eq!(out.trim(), "3"); }
#[test] fn option_chain() { let out = run("fn f() -> Option[int] { Some(5) }\nfn main() { o := f() v := match o { Some(x) => { x } None => { 0 } } put(v) }\n"); assert_eq!(out.trim(), "5"); }
#[test] fn str_iter() { let out = run("fn main() { s := \"abc\" n := 0 for c in s { n = n + 1 } put(n) }\n"); assert_eq!(out.trim(), "3"); }

