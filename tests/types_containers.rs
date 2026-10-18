//! 结构体/枚举/元组/切片测试。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("te_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn struct_field() { let out = run("struct P { x: int, y: int }\nfn main() { p := P { x: 3, y: 4 } put(p.x + p.y) }\n"); assert_eq!(out.trim(), "7"); }
#[test] fn struct_impl() { let out = run("struct P { x: int }\nimpl P { fn dbl(self) -> int { self.x * 2 } }\nfn main() { p := P { x: 5 } put(p.dbl()) }\n"); assert_eq!(out.trim(), "10"); }
#[test] fn enum_match() { let out = run("enum E { A B(int) }\nfn f(e: E) -> int { match e { E::A => { 0 } E::B(n) => { n } } }\nfn main() { put(f(E::B(42))) }\n"); assert_eq!(out.trim(), "42"); }
#[test] fn tuple_index() { let out = run("fn main() { t := (1, \"two\", 3.0) put(t.0) }\n"); assert_eq!(out.trim(), "1"); }
#[test] fn array_slice() { let out = run("fn main() { a := [1, 2, 3, 4, 5] s := a[1..4] put(len(s)) put(s[0]) }\n"); assert_eq!(out.trim(), "3\n2"); }
#[test] fn list_comp() { let out = run("fn main() { l := [1, 2, 3] xs := [x * 2 for x in l] put(len(xs)) put(xs[2]) }\n"); assert_eq!(out.trim(), "3\n6"); }
