//! 深递归 / 大输入健壮性。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("rob_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
}
fn run(src: &str) -> String {
    let d = tmp(); std::fs::create_dir_all(&d).unwrap();
    let g = d.join("t.gt"); std::fs::write(&g, src).unwrap();
    let exe = d.join("t.exe");
    let c = Command::new(gtc()).arg("--c").arg(&g).arg("-o").arg(&exe).arg("-O").arg("2").output().unwrap();
    assert!(c.status.success(), "compile failed: {}", String::from_utf8_lossy(&c.stderr));
    let stdlib = Path::new(env!("CARGO_MANIFEST_DIR")).join("res").join("lib");
    let r = Command::new(&exe).env("GTC_STDLIB_DIR", &stdlib).output().unwrap();
    assert!(r.status.success(), "run failed");
    String::from_utf8_lossy(&r.stdout).into_owned()
}

#[test] fn deep_recursion() { let out = run("fn f(n: int) -> int { if n <= 0 { return 0 } f(n - 1) + 1 }\nfn main() { put(f(10000)) }\n"); assert_eq!(out.trim(), "10000"); }
#[test] fn large_list() { let out = run("fn main() { xs := list() for i in 0..100000 { push(xs, i) } put(len(xs)) }\n"); assert_eq!(out.trim(), "100000"); }
#[test] fn many_vars() { let out = run("fn main() { a := 1 b := 2 c := 3 d := 4 e := 5 put(a+b+c+d+e) }\n"); assert_eq!(out.trim(), "15"); }
#[test] fn nested_loops() { let out = run("fn main() { s := 0 for i in 0..10 { for j in 0..10 { s = s + 1 } } put(s) }\n"); assert_eq!(out.trim(), "100"); }
