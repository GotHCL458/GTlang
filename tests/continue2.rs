//! 继续：derive Hash/Ord/PartialEq、defer+return、match 嵌套解构、enum 递归、并发压力。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("z_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn derive_partial_eq() { let out = run("@derive(PartialEq)\nstruct P { x: int }\nfn main() { put(P { x: 1 } == P { x: 1 }) }\n"); assert_eq!(out.trim(), "true"); }
#[test] fn derive_ord() { let out = run("@derive(Ord, Eq)\nstruct P { x: int }\nfn main() { put(P { x: 1 } < P { x: 2 }) }\n"); assert_eq!(out.trim(), "true"); }
#[test] fn derive_hash() { let out = run("@derive(Hash, Eq)\nstruct P { x: int }\nfn main() { put(1) }\n"); assert_eq!(out.trim(), "1"); }
#[test] fn defer_return_order() { let out = run("fn f() -> int { defer put(9) return 5 }\nfn main() { put(f()) }\n"); assert_eq!(out.trim(), "9\n5"); }
#[test] fn match_nested_some() { let out = run("fn main() { o := Some(Some(5)) r := match o { Some(Some(v)) => { v } Some(None) => { -1 } None => { 0 } } put(r) }\n"); assert_eq!(out.trim(), "5"); }
#[test] fn enum_recursive() { let out = run("enum T { Leaf(int) Node(T, T) }\nfn sum(t: T) -> int { match t { T::Leaf(n) => { n } T::Node(l, r) => { sum(l) + sum(r) } } }\nfn main() { put(sum(T::Node(T::Leaf(1), T::Node(T::Leaf(2), T::Leaf(3))))) }\n"); assert_eq!(out.trim(), "6"); }
#[test] fn conc_stress() { let out = run("fn producer(ch: int) { for i in 0..1000 { chan_send(ch, i) } }\nfn main() { ch := chan() go producer(ch) s := 0 for i in 0..1000 { s = s + chan_recv(ch) } put(s) }\n"); assert_eq!(out.trim(), "499500"); }
