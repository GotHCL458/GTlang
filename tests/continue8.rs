//! 继续：mem 越界、嵌套 finally、match 守卫+return、impl Trait 返回、derive Ord 排序。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("q_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn nested_finally_order() { let out = run("fn main() { try { try { throw \"x\" } expt e { put(1) } fily { put(2) } } expt e2 { put(3) } fily { put(4) } }\n"); assert_eq!(out.trim(), "1\n2\n4"); }
#[test] fn match_guard_return() { let out = run("fn f(x: int) -> int { match x { n if n > 0 => { return 1 } _ => { return 0 } } }\nfn main() { put(f(5)) put(f(-5)) }\n"); assert_eq!(out.trim(), "1\n0"); }
#[test] fn derive_ord_sort() { let out = run("@derive(Ord, Eq)\nstruct P { x: int }\nfn main() { a := P { x: 1 } b := P { x: 2 } put(a < b) put(b < a) }\n"); assert_eq!(out.trim(), "true\nfalse"); }
#[test] fn impl_trait_return() { let out = run("trait S { fn v(self) -> int }\nstruct A {} impl S for A { fn v(self) -> int { 7 } }\nfn make() -> dyn S { dyn S(A {}) }\nfn main() { x := make() put(x.v()) }\n"); assert_eq!(out.trim(), "7"); }
#[test] fn dyn_downcast() { let out = run("trait S { fn v(self) -> int }\nstruct A { n: int } impl S for A { fn v(self) -> int { self.n } }\nfn main() { x := dyn S(A { n: 9 }) put(x.v()) }\n"); assert_eq!(out.trim(), "9"); }
