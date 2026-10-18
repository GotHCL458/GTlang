//! try 嵌套 / match 载荷 / 闭包捕获 / derive。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("tz_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn try_nested() { let out = run("fn main() { try { try { throw \"inner\" } expt e { put(\"in\") } } expt e { put(\"out\") } }\n"); assert_eq!(out.trim(), "in"); }
#[test] fn match_payload_destructure() { let out = run("enum E { A(int) B }\nfn f(e: E) -> int { match e { E::A(n) => { n * 2 } E::B => { 0 } } }\nfn main() { put(f(E::A(21))) }\n"); assert_eq!(out.trim(), "42"); }
#[test] fn closure_captures_mut() { let out = run("fn main() { let mut x = 10 f := |v| v + x put(f(5)) }\n"); assert_eq!(out.trim(), "15"); }
#[test] fn derive_multiple() { let out = run("@derive(Eq, Clone, Debug)\nstruct P { x: int }\nfn main() { a := P { x: 1 } b := a.clone() put(a == b) }\n"); assert_eq!(out.trim(), "true"); }
#[test] fn try_finally_return() { let out = run("fn f() -> int { try { return 1 } fily { put(\"fin\") } }\nfn main() { put(f()) }\n"); assert_eq!(out.trim(), "fin\n1"); }
#[test] fn nested_match_enum() { let out = run("enum E { A(int) B }\nfn g(e: E) -> int { match e { E::A(n) => { if n > 0 { 1 } else { -1 } } E::B => { 0 } } }\nfn main() { put(g(E::A(5))) put(g(E::A(-5))) }\n"); assert_eq!(out.trim(), "1\n-1"); }
