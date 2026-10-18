//! 继续：match 范围+守卫、多字节 str 迭代、? 链式、chan 压力。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("y_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn match_range_guard() { let out = run("fn main() { x := 5 r := match x { n if n > 3 && n < 7 => { 100 } _ => { 0 } } put(r) }\n"); assert_eq!(out.trim(), "100"); }
#[test] fn str_iter_multibyte() { let out = run("fn main() { s := \"你好\" n := 0 for c in s { n = n + 1 } put(n) }\n"); assert_eq!(out.trim(), "2"); }
#[test] fn question_chain() { let out = run("fn a() -> Result[int, str] { Ok(2) }\nfn b() -> Result[int, str] { x := a()? y := a()? Ok(x + y) }\nfn c() -> Result[int, str] { z := b()? Ok(z * 10) }\nfn main() { match c() { Ok(v) => { put(v) } Err(e) => { put(-1) } } }\n"); assert_eq!(out.trim(), "40"); }
#[test] fn chan_stress_5000() { let out = run("fn producer(ch: int) { for i in 0..5000 { chan_send(ch, i) } }\nfn main() { ch := chan() go producer(ch) s := 0 for i in 0..5000 { s = s + chan_recv(ch) } put(s) }\n"); assert_eq!(out.trim(), "12497500"); }
