//! 高级语言特性测试（宏 / @derive / defer / 标签循环 / where / dyn Trait）。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("adv_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn macro_basic() { let out = run("macro sq(x) { x * x }\nfn main() { put(sq(5)) }\n"); assert_eq!(out.trim(), "25"); }
#[test] fn labeled_break() { let out = run("fn main() { outer: for i in 0..3 { for j in 0..3 { if j == 1 { break outer } } } put(\"done\") }\n"); assert_eq!(out.trim(), "done"); }
#[test] fn loop_count() { let out = run("fn main() { loop 3 { put(1) } }\n"); assert_eq!(out.trim(), "1\n1\n1"); }
#[test] fn defer_runs() { let out = run("fn main() { defer put(\"deferred\") put(\"body\") }\n"); assert_eq!(out.trim(), "body\ndeferred"); }
#[test] fn dyn_trait_dispatch() { let out = run("trait Shape { fn area(self) -> f64 }\nstruct Sq { s: f64 }\nimpl Shape for Sq { fn area(self) -> f64 { self.s * self.s } }\nfn main() { x := dyn Shape(Sq { s: 3.0 }) put(1) }\n"); assert_eq!(out.trim(), "1"); }
#[test] fn where_bound() { let out = run("fn maxx[T](a: T, b: T) -> T where T: Ord { if a > b { a } else { b } }\nfn main() { put(maxx(3, 7)) }\n"); assert_eq!(out.trim(), "7"); }
#[test] fn derive_eq() { let out = run("@derive(Eq)\nstruct P { x: int }\nfn main() { a := P { x: 1 } b := P { x: 1 } put(a == b) }\n"); assert_eq!(out.trim(), "true"); }
