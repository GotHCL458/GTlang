//! defer / 嵌套闭包 / 容器遍历 / 字符串操作。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("dm_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn defer_lifo() { let out = run("fn main() { defer put(1) defer put(2) defer put(3) put(0) }\n"); assert_eq!(out.trim(), "0\n3\n2\n1"); }
#[test] fn nested_closure() { let out = run("fn main() { f := |x| { g := |y| y * 2 g(x) + 1 } put(f(5)) }\n"); assert_eq!(out.trim(), "11"); }
#[test] fn map_keys_values() { let out = run("fn main() { m := map() insert(m, 1, 10) insert(m, 2, 20) ks := keys(m) vs := values(m) put(len(ks)) put(len(vs)) }\n"); assert_eq!(out.trim(), "2\n2"); }
#[test] fn set_iter() { let out = run("fn main() { s := set() insert(s, 1) insert(s, 2) n := 0 for k in s { n = n + 1 } put(n) }\n"); assert_eq!(out.trim(), "2"); }
#[test] fn option_unwrap() { let out = run("fn main() { o := Some(42) y := match o { Some(v) => { v } None => { 0 } } put(y) }\n"); assert_eq!(out.trim(), "42"); }
#[test] fn str_split_join() { let out = run("import string\nfn main() { parts := split(\"a,b,c\", \",\") put(len(parts)) put(join(parts, \"-\")) }\n"); assert_eq!(out.trim(), "3\na-b-c"); }
#[test] fn str_replace() { let out = run("import string\nfn main() { put(replace_all(\"aXbXc\", \"X\", \"-\")) }\n"); assert_eq!(out.trim(), "a-b-c"); }
