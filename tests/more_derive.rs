//! derive 更多 trait / try-break / defer-in-match-for / 字符串边界 / map-set 遍历。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("x_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

// ---- derive 多 trait ----
#[test] fn derive_eq() { let out = run("@derive(Eq)\nstruct P { x: int }\nfn main() { a := P { x: 1 } b := P { x: 1 } c := P { x: 2 } put(a == b) put(a == c) }\n"); assert_eq!(out.trim(), "true\nfalse"); }
#[test] fn derive_clone() { let out = run("@derive(Clone)\nstruct P { x: int }\nfn main() { a := P { x: 5 } b := a.clone() put(b.x) }\n"); assert_eq!(out.trim(), "5"); }
#[test] fn derive_default() { let out = run("@derive(Default)\nstruct P { x: int }\nfn main() { p := P::default() put(p.x) }\n"); assert_eq!(out.trim(), "0"); }
#[test] fn derive_debug() { let out = run("@derive(Debug)\nstruct P { x: int }\nfn main() { put(1) }\n"); assert_eq!(out.trim(), "1"); }
#[test] fn derive_display() { let out = run("@derive(Display)\nstruct P { x: int }\nfn main() { put(1) }\n"); assert_eq!(out.trim(), "1"); }

// ---- try 内 break/continue ----
#[test] fn try_break() { let out = run("fn main() { for i in 0..5 { try { if i == 2 { break } put(i) } expt e { } } }\n"); assert_eq!(out.trim(), "0\n1"); }
#[test] fn try_continue() { let out = run("fn main() { for i in 0..4 { try { if i == 1 { continue } put(i) } expt e { } } }\n"); assert_eq!(out.trim(), "0\n2\n3"); }

// ---- defer 在 match 臂 / for 内 ----
#[test] fn defer_in_match_arm() { let out = run("fn main() { x := 1 match x { 1 => { defer put(\"d\") put(\"b\") } _ => { } } }\n"); assert_eq!(out.trim(), "b\nd"); }
#[test] fn defer_in_for() { let out = run("fn main() { for i in 0..2 { defer put(9) put(100 + i) } }\n"); assert_eq!(out.trim(), "100\n101\n9"); }

// ---- 字符串边界 ----
#[test] fn substr_oob() { let out = run("import string\nfn main() { put(substr(\"hello\", 1, 100)) }\n"); assert_eq!(out.trim(), "ello"); }
#[test] fn split_empty() { let out = run("import string\nfn main() { parts := split(\"\", \",\") put(len(parts)) }\n"); assert_eq!(out.trim(), "1"); }
#[test] fn replace_all_special() { let out = run("import string\nfn main() { put(replace_all(\"a.b.c\", \".\", \"-\")) }\n"); assert_eq!(out.trim(), "a-b-c"); }

// ---- map/set 遍历 ----
#[test] fn map_keys_values() { let out = run("fn main() { m := map() insert(m, 1, 10) insert(m, 2, 20) ks := keys(m) vs := values(m) put(len(ks)) put(len(vs)) }\n"); assert_eq!(out.trim(), "2\n2"); }
#[test] fn map_iter() { let out = run("fn main() { m := map() insert(m, 1, 10) insert(m, 2, 20) s := 0 for k in m { s = s + k } put(s) }\n"); assert_eq!(out.trim(), "3"); }
#[test] fn set_iter() { let out = run("fn main() { s := set() insert(s, 1) insert(s, 2) insert(s, 3) n := 0 for k in s { n = n + 1 } put(n) }\n"); assert_eq!(out.trim(), "3"); }
