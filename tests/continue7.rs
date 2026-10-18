//! 继续：chan 关闭/select、throw 非 str、mem 边界、unparse 再编译。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("k_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn throw_non_str() { let out = run("fn main() { try { throw 42 } expt e { put(\"caught\") } }\n"); assert_eq!(out.trim(), "caught"); }
#[test] fn mem_alloc_load() { let out = run("fn main() { p := mem_alloc(8) mem_store_i64(p, 0, 12345) put(mem_load_i64(p, 0)) mem_free(p) }\n"); assert_eq!(out.trim(), "12345"); }
#[test] fn mem_copy_set() { let out = run("fn main() { a := mem_alloc(8) b := mem_alloc(8) mem_store_i64(a, 0, 42) mem_copy(b, a, 8) put(mem_load_i64(b, 0)) mem_free(a) mem_free(b) }\n"); assert_eq!(out.trim(), "42"); }
#[test] fn mem_u8() { let out = run("fn main() { p := mem_alloc(4) mem_store_u8(p, 0, 200) put(mem_load_u8(p, 0)) mem_free(p) }\n"); assert_eq!(out.trim(), "200"); }

