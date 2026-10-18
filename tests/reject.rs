//! 非法程序应被拒绝（诊断正确性）。

use std::path::{Path, PathBuf};
use std::process::Command;
fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("rej_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
}
fn rejected(src: &str) {
    let d = tmp(); std::fs::create_dir_all(&d).unwrap();
    let g = d.join("t.gt"); std::fs::write(&g, src).unwrap();
    let c = Command::new(gtc()).arg("--check").arg(&g).output().unwrap();
    assert!(!c.status.success(), "should be rejected but compiled:\n{}", src);
}

#[test] fn undefined_var() { rejected("fn main() { put(x) }\n"); }
#[test] fn type_mismatch() { rejected("fn main() { x: int = \"str\" put(x) }\n"); }
#[test] fn const_reassign() { rejected("const X = 1\nfn main() { X = 2 }\n"); }
#[test] fn move_then_use() { rejected("fn main() { s := \"hi\" t := s put(s) }\n"); }
#[test] fn non_exhaustive_match() { rejected("fn main() { x := 1 y := match x { 1 => { 10 } } }\n"); }
#[test] fn wrong_arity() { rejected("fn f(a: int) -> int { a }\nfn main() { put(f(1, 2)) }\n"); }
#[test] fn undefined_fn() { rejected("fn main() { put(undefined_fn()) }\n"); }
#[test] fn return_type_mismatch() { rejected("fn f() -> int { return \"x\" }\nfn main() { put(f()) }\n"); }
#[test] fn break_outside_loop() { rejected("fn main() { break }\n"); }
#[test] fn assign_immutable() { rejected("fn main() { let y = 1 y = 2 }\n"); }
