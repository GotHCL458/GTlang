//! 优化器 pass 的边界/回归测试。

use gtc_rust::build;

fn ok(src: &str) {
    match build("t.gt", src) {
        Ok(_) => {}
        Err(d) => panic!("frontend failed: {:?}", d.iter().map(|x| &x.message).collect::<Vec<_>>()),
    }
}

#[test]
fn licm_respects_loop_dependency() {
    ok("fn main() {\n    s := 0\n    for i in 0..10 {\n        k := 100\n        d := i * 2\n        s = s + k + d\n    }\n    put(s)\n}\n");
}

#[test]
fn unused_let_keeps_do_while_var() {
    ok("fn main() {\n    i := 0\n    do {\n        put(i)\n        i++\n    } while i < 3\n}\n");
}

#[test]
fn cse_skips_mut() {
    ok("fn main() {\n    let mut a = 5\n    let mut b = 5\n    a = 6\n    put(a + b)\n}\n");
}

#[test]
fn const_if_prune_semantics() {
    ok("fn main() {\n    if true { put(1) } else { put(2) }\n    if false { put(3) } else { put(4) }\n}\n");
}
