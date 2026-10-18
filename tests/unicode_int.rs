//! Unicode 标识符 / 整数边界 / 空程序测试。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("uni_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn chinese_identifiers() { let out = run("fn main() { 计数器 := 0 累加 := 5 计数器 = 计数器 + 累加 put(计数器) }\n"); assert_eq!(out.trim(), "5"); }
#[test] fn chinese_fn() { let out = run("fn 加法(a: int, b: int) -> int { a + b }\nfn main() { put(加法(2, 3)) }\n"); assert_eq!(out.trim(), "5"); }
#[test] fn i64_max() { let out = run("fn main() { put(9223372036854775807) }\n"); assert_eq!(out.trim(), "9223372036854775807"); }
#[test] fn i64_min() { let out = run("fn main() { put(-9223372036854775807 - 1) }\n"); assert_eq!(out.trim(), "-9223372036854775808"); }
#[test] fn hex_bin_oct() { let out = run("fn main() { put(0xFF) put(0b1010) put(0o17) }\n"); assert_eq!(out.trim(), "255\n10\n15"); }
#[test] fn big_literal() { let out = run("fn main() { put(1000000000000 * 1000000) }\n"); assert_eq!(out.trim(), "1000000000000000000"); }
#[test] fn empty_main() { let out = run("fn main() { }\n"); assert_eq!(out.trim(), ""); }
