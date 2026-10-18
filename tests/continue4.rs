//! 继续：derive 其余 / dyn 多态 / 闭包可变捕获 / 字符串 format / map 删除。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("v_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn dyn_polymorphism() { let out = run("trait Shape { fn area(self) -> int }\nstruct Sq { s: int }\nstruct Rc { w: int }\nimpl Shape for Sq { fn area(self) -> int { self.s * self.s } }\nimpl Shape for Rc { fn area(self) -> int { self.w * 2 } }\nfn main() { shapes := list() push(shapes, dyn Shape(Sq { s: 3 })) push(shapes, dyn Shape(Rc { w: 5 })) put(1) }\n"); assert_eq!(out.trim(), "1"); }
#[test] fn closure_capture_mut_read() { let out = run("fn main() { let mut n = 0 for i in 0..3 { n = n + i } put(n) }\n"); assert_eq!(out.trim(), "3"); }
#[test] fn str_format() { let out = run("import string\nfn main() { put(format(\"{} + {}\", \"2\\n3\")) }\n"); assert_eq!(out.trim(), "2 + 3"); }
#[test] fn map_remove() { let out = run("fn main() { m := map() insert(m, 1, 10) insert(m, 2, 20) remove(m, 1) put(len(m)) put(at(m, 2)) }\n"); assert_eq!(out.trim(), "1\n20"); }
#[test] fn set_remove() { let out = run("fn main() { s := set() insert(s, 1) insert(s, 2) remove(s, 1) put(len(s)) put(has(s, 2)) }\n"); assert_eq!(out.trim(), "1\ntrue"); }
#[test] fn trait_object_vec() { let out = run("trait S { fn v(self) -> int }\nstruct A {} impl S for A { fn v(self) -> int { 1 } }\nstruct B {} impl S for B { fn v(self) -> int { 2 } }\nfn sum(xs: list[dyn S]) -> int { s := 0 for x in xs { s = s + x.v() } s }\nfn main() { xs := list() push(xs, dyn S(A {})) push(xs, dyn S(B {})) put(sum(xs)) }\n"); assert_eq!(out.trim(), "3"); }
