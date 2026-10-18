//! 控制流边缘测试（for-else / try-fily / 嵌套）。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("cf_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn for_else_no_break() { let out = run("fn main() { for i in 0..3 { put(i) } else { put(\"done\") } }\n"); assert_eq!(out.trim(), "0\n1\n2\ndone"); }
#[test] fn for_else_break() { let out = run("fn main() { for i in 0..3 { put(i) if i == 1 { break } } else { put(\"done\") } }\n"); assert_eq!(out.trim(), "0\n1"); }
#[test] fn try_finally() { let out = run("fn main() { try { put(\"try\") } expt e { put(\"expt\") } fily { put(\"fin\") } }\n"); assert_eq!(out.trim(), "try\nfin"); }
#[test] fn try_catch_finally() { let out = run("fn main() { try { throw \"e\" } expt e { put(\"caught\") } fily { put(\"fin\") } }\n"); assert_eq!(out.trim(), "caught\nfin"); }
#[test] fn nested_loops_break() { let out = run("fn main() { s := 0 for i in 0..3 { for j in 0..3 { if j == 1 { break } s = s + 1 } } put(s) }\n"); assert_eq!(out.trim(), "3"); }
#[test] fn while_continue() { let out = run("fn main() { i := 0 s := 0 while i < 5 { i = i + 1 if i == 3 { continue } s = s + i } put(s) }\n"); assert_eq!(out.trim(), "12"); }
