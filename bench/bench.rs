fn fib(n: i64) -> i64 {
    if n < 2 { return n; }
    fib(n-1) + fib(n-2)
}
fn loop_sum() -> i64 {
    let mut s: i64 = 0;
    for i in 0..200000000i64 {
        s = s.wrapping_add(i);
    }
    s
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let which = args.get(1).map(|s| s.as_str()).unwrap_or("fib");
    if which == "fib" { println!("{}", fib(35)); }
    else { println!("{}", loop_sum()); }
}
