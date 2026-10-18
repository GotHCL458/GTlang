//! C 互操作 / 内联 C 块语义测试。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("ci_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
}
fn run(src: &str) -> String {
    let d = tmp(); std::fs::create_dir_all(&d).unwrap();
    let g = d.join("t.gt"); std::fs::write(&g, src).unwrap();
    let exe = d.join("t.exe");
    let c = Command::new(gtc()).arg("--c").arg(&g).arg("-o").arg(&exe).arg("-O").arg("2").output().unwrap();
    assert!(c.status.success(), "compile failed: {}", String::from_utf8_lossy(&c.stderr));
    let stdlib = Path::new(env!("CARGO_MANIFEST_DIR")).join("res").join("lib");
    let r = Command::new(&exe).env("GTC_STDLIB_DIR", &stdlib).output().unwrap();
    assert!(r.status.success(), "run failed");
    String::from_utf8_lossy(&r.stdout).into_owned()
}

#[test] fn inline_c_call() { let out = run("C {\n    static long long triple(long long x) { return x * 3; }\n}\nfn main() { put(triple(7)) }\n"); assert_eq!(out.trim(), "21"); }
#[test] fn c_calls_gt() { let out = run("C {\n    static long long call_gt(long long x) { return gt_add1(x) * 10; }\n}\nfn gt_add1(x: int) -> int { x + 1 }\nfn main() { put(call_gt(4)) }\n"); assert_eq!(out.trim(), "50"); }
#[test] fn extern_c_strlen() { let out = run("extern \"C\" { fn strlen(s: str) -> int }\nfn main() { put(strlen(\"hello\")) }\n"); assert_eq!(out.trim(), "5"); }
