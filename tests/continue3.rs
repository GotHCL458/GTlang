//! 继续：try-break 跨函数边界、defer+?、match 守卫+解构、impl 泛型、trait 默认方法。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("w_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn defer_with_question() { let out = run("fn g() -> Result[int, str] { Ok(3) }\nfn f() -> Result[int, str] { defer put(\"d\") x := g()? Ok(x + 1) }\nfn main() { r := f() put(1) }\n"); assert_eq!(out.trim(), "1"); }
#[test] fn trait_default_method() { let out = run("trait S { fn area(self) -> int fn twice(self) -> int { self.area() * 2 } }\nstruct R { w: int }\nimpl S for R { fn area(self) -> int { self.w * self.w } }\nfn main() { r := R { w: 3 } put(r.area()) put(r.twice()) }\n"); assert_eq!(out.trim(), "9\n18"); }
#[test] fn try_break_outer() { let out = run("fn main() { for i in 0..5 { try { if i == 2 { break } put(i) } expt e { } } put(99) }\n"); assert_eq!(out.trim(), "0\n1\n99"); }

