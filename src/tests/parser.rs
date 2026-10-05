//! parser 的单元测试：语法边界（成功解析 / 友好报错）。
#![cfg(test)]

use super::parse_program;

fn ok(src: &str) {
    if let Err(e) = parse_program(src) {
        panic!("should parse: {} --- {}", e.msg, src);
    }
}

fn err(src: &str, needle: &str) {
    match parse_program(src) {
        Ok(_) => panic!("should fail: {}", src),
        Err(e) => assert!(e.msg.contains(needle), "expected {:?} in {:?}", needle, e.msg),
    }
}

#[test]
fn basic_fn_parses() {
    ok("fn main() { put(1) }");
    ok("fn 加(a: int, b: int) -> int { return a + b }");
    ok("fn main() { x := 1 + 2 * 3 }");
}

#[test]
fn closures_parse() {
    ok("fn main() { f := |x: int| x + 1 }");
    ok("fn main() { f := |a: int, b: int| a * b }");
    ok("fn main() { f := |x: int| -> int { x * x } }");
}

#[test]
fn any_expression_as_callee_parses() {
    ok("fn main() { fs := list()  put(fs[0](1)) }");
    ok("fn main() { f := |x: int| x  put((f)(1)) }");
}

#[test]
fn first_class_fn_name_parses() {
    ok("fn 加一(x: int) -> int { x + 1 }  fn main() { g := 加一  put(g(1)) }");
    ok("fn 加一(x: int) -> int { x + 1 }  fn main() { put(应用(加一, 1)) }");
}

#[test]
fn generic_and_where_parse() {
    ok("fn 恒等[T](x: T) -> T { x }");
    ok("fn max[T](a: T, b: T) -> T where T: Ord { a }");
    ok("fn f[T: A + B](x: T) -> int { 0 }");
}

#[test]
fn struct_enum_trait_parse() {
    ok("struct 点 { x: int  y: int }");
    ok("enum 形状 { Circle(f64) Rect(f64, f64) Unit }");
    ok("trait 面积 { fn area(self) -> f64 }");
    ok("impl 面积 for 圆 { fn area(self) -> f64 { 1.0 } }");
}

#[test]
fn control_flow_parses() {
    ok("fn main() { if true { put(1) } elif false { put(2) } else { put(3) } }");
    ok("fn main() { i := 0  while i < 3 { i = i + 1 } }");
    ok("fn main() { loop 3 { put(1) } }");
    ok("fn main() { for i in 0..10 { put(i) } }");
    ok("fn main() { for v in list() { put(v) } }");
}

#[test]
fn match_parses() {
    ok("fn main() { m := match 1 { 1 => { put(1) }  _ => { put(2) } } }");
    ok("fn main() { m := match 5 { 1..10 => { put(1) }  _ => { put(2) } } }");
}

#[test]
fn loop_without_count_reports_friendly() {
    err("fn main() { loop { put(1) } }", "loop");
}

#[test]
fn unbalanced_reports_error() {
    // 缺 '}' 时报"文件意外结束"（友好提示）
    err("fn main() { put(1)", "意外结束");
    err("fn main() {", "意外结束");
}

#[test]
fn deeply_nested_is_rejected_not_overflow() {
    // 300 层括号应报错而非爆栈
    let src = format!("fn main() {{ x := {}1{} }}", "(".repeat(300), ")".repeat(300));
    err(&src, "deep");
}

#[test]
fn long_binary_chain_is_rejected() {
    let src = format!("fn main() {{ x := {}1 }}", "1 + ".repeat(500));
    err(&src, "deep");
}
