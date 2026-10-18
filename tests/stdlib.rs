//! 标准库函数测试。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("lib_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
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

#[test] fn math_sqrt() { let out = run("import math\nfn main() { put(sqrt(16.0)) }\n"); assert_eq!(out.trim(), "4"); }
#[test] fn math_isqrt() { let out = run("import math\nfn main() { put(isqrt(17)) }\n"); assert_eq!(out.trim(), "4"); }
#[test] fn math_gcd() { let out = run("import math\nfn main() { put(gcd(48, 36)) }\n"); assert_eq!(out.trim(), "12"); }
#[test] fn math_isprime() { let out = run("import math\nfn main() { put(isprime(17)) put(isprime(18)) }\n"); assert_eq!(out.trim(), "true\nfalse"); }
#[test] fn math_factorial() { let out = run("import math\nfn main() { put(factorial(5)) }\n"); assert_eq!(out.trim(), "120"); }
#[test] fn string_upper() { let out = run("import string\nfn main() { put(upper(\"abc\")) }\n"); assert_eq!(out.trim(), "ABC"); }
#[test] fn string_capitalize() { let out = run("import string\nfn main() { put(capitalize(\"hello\")) }\n"); assert_eq!(out.trim(), "Hello"); }
#[test] fn string_zfill() { let out = run("import string\nfn main() { put(zfill(\"42\", 5)) }\n"); assert_eq!(out.trim(), "00042"); }
#[test] fn string_utf8_len() { let out = run("import string\nfn main() { put(utf8_len(\"你好世界\")) }\n"); assert_eq!(out.trim(), "4"); }
#[test] fn json_escape_str() { let out = run("import json\nfn main() { s := json_dumps(\"hello\") put(len(s) > 0) }\n"); assert_eq!(out.trim(), "true"); }
