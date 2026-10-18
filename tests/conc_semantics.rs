//! 并发（go + chan）语义测试。

use std::path::{Path, PathBuf};
use std::process::Command;

fn gtc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_gtc")) }
fn tmp() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join("gtc").join(format!("conc_{}_{}", std::process::id(), C.fetch_add(1, Ordering::Relaxed)))
}
fn run(src: &str, timeout_secs: u64) -> String {
    let d = tmp();
    std::fs::create_dir_all(&d).unwrap();
    let g = d.join("t.gt"); std::fs::write(&g, src).unwrap();
    let exe = d.join("t.exe");
    let c = Command::new(gtc()).arg("--c").arg(&g).arg("-o").arg(&exe).arg("-O").arg("2").output().unwrap();
    assert!(c.status.success(), "compile failed: {}", String::from_utf8_lossy(&c.stderr));
    let stdlib = Path::new(env!("CARGO_MANIFEST_DIR")).join("res").join("lib");
    let mut child = Command::new(&exe).env("GTC_STDLIB_DIR", &stdlib).stdout(std::process::Stdio::piped()).spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if std::time::Instant::now() > deadline { child.kill().ok(); panic!("timeout (deadlock?)"); }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => panic!("wait err: {}", e),
        }
    }
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn chan_producer_consumer() {
    let out = run("fn producer(ch: int) {\n    for i in 0..100 { chan_send(ch, i) }\n}\nfn main() {\n    ch := chan()\n    go producer(ch)\n    s := 0\n    for i in 0..100 { s = s + chan_recv(ch) }\n    put(s)\n}\n", 10);
    assert_eq!(out.trim(), "4950", "sum 0..99");
}

#[test]
fn chan_batch() {
    let out = run("fn main() {\n    ch := chan()\n    xs := list()\n    for i in 0..5 { push(xs, i) }\n    put(chan_send_batch(ch, xs))\n    ys := chan_recv_batch(ch, 10)\n    put(len(ys))\n}\n", 10);
    assert_eq!(out.trim(), "5\n5", "batch send 5, recv 5");
}

#[test]
fn go_spawn_many() {
    // 100 个 go 任务，各做 1e5 次累加；sleep 等待完成，验证不崩溃。
    let out = run("fn worker(id: int) {\n    s := 0\n    for i in 0..100000 { s = s + i }\n    if s < 0 { put(id) }\n}\nfn main() {\n    for t in 0..100 { go worker(t) }\n    sleep(500)\n    put(\"done\")\n}\n", 10);
    assert!(out.contains("done"), "got: {}", out);
}
