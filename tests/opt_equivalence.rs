//! 优化器语义等价性：-O0 与 -O4 的运行输出必须一致。

use std::path::{Path, PathBuf};
use std::process::Command;

fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join("gtc").join(format!("opt_eq_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&d).unwrap(); d
}
fn run_src(src: &str, opt: u8) -> String {
    let d = tmp();
    let g = d.join("t.gt"); std::fs::write(&g, src).unwrap();
    let exe = d.join("t.exe");
    let c = Command::new(gtc()).arg("--c").arg(&g).arg("-o").arg(&exe).arg("-O").arg(opt.to_string()).output().unwrap();
    assert!(c.status.success(), "compile O{} failed: {}", opt, String::from_utf8_lossy(&c.stderr));
    let stdlib = Path::new(env!("CARGO_MANIFEST_DIR")).join("res").join("lib");
    let r = Command::new(&exe).env("GTC_STDLIB_DIR", &stdlib).output().unwrap();
    assert!(r.status.success(), "run O{} failed", opt);
    String::from_utf8_lossy(&r.stdout).into_owned()
}
fn eq(src: &str) {
    let o0 = run_src(src, 0); let o4 = run_src(src, 4);
    assert_eq!(o0, o4, "O0 vs O4 mismatch:\n{}", src);
}

#[test] fn eq_const_fold() { eq("fn main() {\n    a := 2 + 3 * 4\n    put(a)\n}\n"); }
#[test] fn eq_unused_let() { eq("fn main() {\n    x := 42\n    put(7)\n}\n"); }
#[test] fn eq_cse() { eq("fn main() {\n    a := 10\n    b := 10\n    put(a + b)\n}\n"); }
#[test] fn eq_if_prune() { eq("fn main() {\n    if true { put(1) } else { put(2) }\n}\n"); }
#[test] fn eq_licm() { eq("fn main() {\n    s := 0\n    for i in 0..10 {\n        k := 5\n        s = s + k + i\n    }\n    put(s)\n}\n"); }
#[test] fn eq_dowhile() { eq("fn main() {\n    i := 0\n    do {\n        put(i)\n        i++\n    } while i < 3\n}\n"); }
#[test] fn eq_algebraic() { eq("fn main() {\n    x := 7\n    put(x + 0)\n    put(x * 1)\n    put(x - 0)\n    put(x / 1)\n}\n"); }
#[test] fn eq_match() { eq("fn main() {\n    x := 3\n    y := match x {\n        1 => { 10 }\n        _ if x > 2 => { 20 }\n        _ => { 30 }\n    }\n    put(y)\n}\n"); }
#[test] fn eq_interp() { eq("fn main() {\n    x := 3\n    put(\"a${x}b\")\n}\n"); }
