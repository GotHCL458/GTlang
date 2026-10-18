fn fib(n: i64) -> i64 { if n < 2 { return n; } fib(n-1) + fib(n-2) }
fn loop_sum() -> i64 { let mut s: i64 = 0; for i in 0..200000000i64 { s = s.wrapping_add(i); } s }
static mut G_STATE: i64 = 1;
fn step() -> i64 { unsafe { G_STATE = G_STATE.wrapping_mul(1103515245).wrapping_add(12345) & 0x7fffffff; G_STATE } }
fn loop_lcg() -> i64 { let mut s: i64 = 0; for _ in 0..10000000 { s = (s.wrapping_add(step())) % 1000000007; } s }
fn arr_sum() -> i64 { let mut a = [0i64; 1000]; for i in 0..1000 { a[i] = (i as i64) * 997; } let mut s: i64 = 0; for _ in 0..100000 { for i in 0..1000 { s = s.wrapping_add(a[i]); } } s }
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let w = args.get(1).map(|s| s.as_str()).unwrap_or("loop");
    let r = if w == "lcg" { loop_lcg() } else if w == "arr" { arr_sum() } else if w == "fib" { fib(35) } else { loop_sum() };
    println!("{}", r);
}
