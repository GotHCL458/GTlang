//! 所有权 / 错误处理测试。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("own_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn move_rejected() {
    let d = tmp(); std::fs::create_dir_all(&d).unwrap();
    let g = d.join("t.gt"); std::fs::write(&g, "fn main() { s := \"hi\" t := s put(s) }\n").unwrap();
    let c = Command::new(gtc()).arg("--check").arg(&g).output().unwrap();
    assert!(!c.status.success(), "move-then-use should be rejected");
}
#[test] fn result_ok() { let out = run("fn f() -> Result[int, str] { Ok(42) }\nfn main() { r := f() put(1) }\n"); assert_eq!(out.trim(), "1"); }
#[test] fn try_propagation() { let out = run("fn f() -> Result[int, str] { Ok(7) }\nfn g() -> Result[int, str] { x := f()? Ok(x + 1) }\nfn main() { r := g() put(1) }\n"); assert_eq!(out.trim(), "1"); }
#[test] fn throw_catch() { let out = run("fn main() { try { throw \"err\" } expt e { put(\"caught\") } }\n"); assert_eq!(out.trim(), "caught"); }
#[test] fn assert_pass() { let out = run("fn main() { assert(1 < 2) put(\"ok\") }\n"); assert_eq!(out.trim(), "ok"); }
