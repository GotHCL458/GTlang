//! 优化器对复杂控制流的语义等价性（-O0 vs -O4）。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("eq2_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
}
fn run(src: &str, opt: u8) -> String {
    let d = tmp(); std::fs::create_dir_all(&d).unwrap();
    let g = d.join("t.gt"); std::fs::write(&g, src).unwrap();
    let exe = d.join("t.exe");
    let c = Command::new(gtc()).arg("--c").arg(&g).arg("-o").arg(&exe).arg("-O").arg(opt.to_string()).output().unwrap();
    assert!(c.status.success(), "compile O{} failed: {}", opt, String::from_utf8_lossy(&c.stderr));
    let stdlib = Path::new(env!("CARGO_MANIFEST_DIR")).join("res").join("lib");
    let r = Command::new(&exe).env("GTC_STDLIB_DIR", &stdlib).output().unwrap();
    String::from_utf8_lossy(&r.stdout).into_owned()
}
fn eq(src: &str) { let o0 = run(src, 0); let o4 = run(src, 4); assert_eq!(o0, o4, "mismatch:\n{}", src); }

#[test] fn eq_shadow_if() { eq("fn main() { x := 1 if true { x := 2 put(x) } put(x) }\n"); }
#[test] fn eq_shadow_for() { eq("fn main() { n := 5 for i in 0..3 { n := i * 10 put(n) } put(n) }\n"); }
#[test] fn eq_try_catch() { eq("fn main() { try { throw \"e\" } expt e { put(1) } }\n"); }
#[test] fn eq_match_guard() { eq("fn main() { x := 7 y := match x { n if n > 5 => { 100 } _ => { 0 } } put(y) }\n"); }
#[test] fn eq_closure_capture() { eq("fn main() { n := 10 f := |x| x * n put(f(3)) }\n"); }
#[test] fn eq_while_break_continue() { eq("fn main() { i := 0 s := 0 while i < 10 { i = i + 1 if i % 2 == 0 { continue } if i > 7 { break } s = s + i } put(s) }\n"); }
#[test] fn eq_nested_interp() { eq("fn main() { a := 1 b := 2 put(\"x${a}y${b}z\") }\n"); }
#[test] fn eq_for_else() { eq("fn main() { for i in 0..3 { put(i) } else { put(9) } }\n"); }
