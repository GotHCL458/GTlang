//! 字符串 / 容器边界测试。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("sb_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn str_byte_len() { let out = run("fn main() { put(len(\"你好世界\")) }\n"); assert_eq!(out.trim(), "12"); }
#[test] fn str_concat() { let out = run("fn main() { a := \"foo\" b := \"bar\" put(a + b) }\n"); assert_eq!(out.trim(), "foobar"); }
#[test] fn list_empty_len() { let out = run("fn main() { xs := list() put(len(xs)) }\n"); assert_eq!(out.trim(), "0"); }
#[test] fn list_push_pop() { let out = run("fn main() { xs := list() push(xs, 1) push(xs, 2) put(pop(xs)) put(len(xs)) }\n"); assert_eq!(out.trim(), "2\n1"); }
#[test] fn map_empty() { let out = run("fn main() { m := map() put(len(m)) }\n"); assert_eq!(out.trim(), "0"); }
#[test] fn map_put_get() { let out = run("fn main() { m := map() insert(m, \"k\", 42) put(at(m, \"k\")) }\n"); assert_eq!(out.trim(), "42"); }
#[test] fn big_int_arith() { let out = run("fn main() { a := 1000000000000 b := 2000000000000 put(a + b) }\n"); assert_eq!(out.trim(), "3000000000000"); }
#[test] fn str_slice() { let out = run("fn main() { s := \"hello\" put(s[1..4]) }\n"); assert_eq!(out.trim(), "ell"); }
