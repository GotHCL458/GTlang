//! 模块 import 测试（跨文件）。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("mod_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
}
fn run_files(files: &[(&str, &str)], main: &str) -> String {
    let d = tmp(); std::fs::create_dir_all(&d).unwrap();
    for (name, src) in files { std::fs::write(d.join(name), src).unwrap(); }
    let g = d.join("main.gt"); std::fs::write(&g, main).unwrap();
    let exe = d.join("t.exe");
    let c = Command::new(gtc()).arg("--c").arg(&g).arg("-o").arg(&exe).arg("-O").arg("2").output().unwrap();
    assert!(c.status.success(), "compile failed: {}", String::from_utf8_lossy(&c.stderr));
    let stdlib = Path::new(env!("CARGO_MANIFEST_DIR")).join("res").join("lib");
    let r = Command::new(&exe).env("GTC_STDLIB_DIR", &stdlib).output().unwrap();
    String::from_utf8_lossy(&r.stdout).into_owned()
}

#[test] fn import_user_module() {
    let out = run_files(&[("m.gt", "pub fn add(a: int, b: int) -> int { a + b }\n")], "import \"m.gt\"\nfn main() { put(m.add(2, 3)) }\n");
    assert_eq!(out.trim(), "5");
}
