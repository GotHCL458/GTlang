//! 泛型 / 闭包测试。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("gen_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn generic_fn() { let out = run("fn id[T](x: T) -> T { x }\nfn main() { put(id(42)) put(id(\"hi\")) }\n"); assert_eq!(out.trim(), "42\nhi"); }
#[test] fn generic_pair() { let out = run("fn first[T, U](a: T, b: U) -> T { a }\nfn main() { put(first(1, \"x\")) }\n"); assert_eq!(out.trim(), "1"); }
#[test] fn closure_capture() { let out = run("fn main() { n := 10 f := |x| x * n put(f(3)) }\n"); assert_eq!(out.trim(), "30"); }
#[test] fn closure_nocapture() { let out = run("fn main() { f := |x| x + 1 put(f(41)) }\n"); assert_eq!(out.trim(), "42"); }
#[test] fn closure_hof() { let out = run("fn apply(x: int, f) -> int { f(x) }\nfn main() { put(apply(5, |v| v * 2)) }\n"); assert_eq!(out.trim(), "10"); }
