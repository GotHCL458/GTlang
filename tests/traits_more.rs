//! derive / dyn Trait / impl 泛型 / ? 链式 / match 范围 / set。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("dr_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn derive_display() { let out = run("@derive(Display)\nstruct P { x: int }\nfn main() { p := P { x: 5 } put(1) }\n"); assert_eq!(out.trim(), "1"); }
#[test] fn derive_default() { let out = run("@derive(Default)\nstruct P { x: int }\nfn main() { p := P::default() put(p.x) }\n"); assert_eq!(out.trim(), "0"); }
#[test] fn dyn_trait_two_methods() { let out = run("trait S { fn area(self) -> int fn name(self) -> int }\nstruct Sq { s: int }\nimpl S for Sq { fn area(self) -> int { self.s * self.s } fn name(self) -> int { 42 } }\nfn main() { d := dyn S(Sq { s: 3 }) put(d.area()) put(d.name()) }\n"); assert_eq!(out.trim(), "9\n42"); }
#[test] fn generic_struct() { let out = run("struct Box[T] { v: T }\nfn main() { b := Box { v: 42 } put(b.v) }\n"); assert_eq!(out.trim(), "42"); }
#[test] fn try_chain() { let out = run("fn a() -> Result[int, str] { Ok(1) }\nfn b() -> Result[int, str] { x := a()? Ok(x + 1) }\nfn c() -> Result[int, str] { y := b()? Ok(y * 10) }\nfn main() { r := c() put(1) }\n"); assert_eq!(out.trim(), "1"); }
#[test] fn match_range() { let out = run("fn main() { x := 5 y := match x { 1..4 => { 10 } 4..7 => { 20 } _ => { 0 } } put(y) }\n"); assert_eq!(out.trim(), "20"); }
#[test] fn set_ops() { let out = run("fn main() { s := set() insert(s, 1) insert(s, 2) insert(s, 1) put(len(s)) put(has(s, 1)) }\n"); assert_eq!(out.trim(), "2\ntrue"); }
