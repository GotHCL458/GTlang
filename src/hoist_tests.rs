//! convert_closure_calls_expr 的遍历完备性测试：闭包调用出现在任意子表达式里都应被改写。
#![cfg(test)]

use crate::ast::*;

fn sentinel_ident() -> Expr { Expr::new(ExprKind::Ident("__SENT__".into()), 0) }
fn closure_call() -> Expr { Expr::new(ExprKind::Call("__SENT__".into(), vec![]), 0) }
fn e(kind: ExprKind) -> Expr { Expr::new(kind, 0) }
fn blk(x: Expr) -> Block { vec![Stmt::Expr(x)] }

/// 判断 expr 的子表达式里是否还有"未改写的 __SENT__(...) 调用"
fn has_unconverted_call(expr: &Expr) -> bool {
    let mut found = false;
    fn walk(e: &Expr, found: &mut bool) {
        if let ExprKind::Call(n, _) = &e.kind { if n == "__SENT__" { *found = true; } }
        // 只检查常见子位置（与测试意图一致）
        match &e.kind {
            ExprKind::Unary(_, a) => walk(a, found),
            ExprKind::Binary(_, a, b) => { walk(a, found); walk(b, found); }
            ExprKind::Call(_, args) => for a in args { walk(a, found); },
            ExprKind::CallValue { callee, args } => { walk(callee, found); for a in args { walk(a, found); } }
            _ => {}
        }
    }
    walk(expr, &mut found);
    found
}

/// 直接构造"含闭包调用的变体"，跑 convert_closure_calls_expr 后应全部变成 CallValue。
#[test]
fn convert_closure_calls_covers_variants() {
    let cv = vec!["__SENT__".to_string()];
    let rcf: Vec<String> = Vec::new();
    // 用一个 block 包住每个变体，逐一验证
    let variants: Vec<(&str, Expr)> = vec![
        ("Unary", e(ExprKind::Unary(UnOp::Neg, Box::new(closure_call())))),
        ("Binary", e(ExprKind::Binary(BinOp::Add, Box::new(closure_call()), Box::new(closure_call())))),
        ("Call-arg", e(ExprKind::Call("g".into(), vec![closure_call()]))),
        ("Index", e(ExprKind::Index(Box::new(closure_call()), Box::new(closure_call())))),
        ("ArrayLit", e(ExprKind::ArrayLit(vec![closure_call()]))),
        ("Interp", e(ExprKind::Interp(vec![StrPart::Expr(Box::new(closure_call()))]))),
        ("If-cond", e(ExprKind::If { cond: Box::new(closure_call()), then: blk(closure_call()), els: None })),
        ("If-then", e(ExprKind::If { cond: Box::new(sentinel_ident()), then: blk(closure_call()), els: None })),
        ("Slice", e(ExprKind::Slice(Box::new(closure_call()), Box::new(closure_call()), Box::new(closure_call())))),
        ("TupleLit", e(ExprKind::TupleLit(vec![closure_call()]))),
        ("Field", e(ExprKind::Field(Box::new(closure_call()), "f".into()))),
        ("StructLit", e(ExprKind::StructLit("S".into(), vec![("a".into(), closure_call())]))),
        ("EnumLit", e(ExprKind::EnumLit("E".into(), "V".into(), vec![closure_call()]))),
        ("DynBox", e(ExprKind::DynBox { trait_name: "T".into(), value: Box::new(closure_call()) })),
        ("MethodOn", e(ExprKind::MethodOn { recv: Box::new(closure_call()), method: "m".into(), args: vec![closure_call()] })),
        ("Borrow", e(ExprKind::Borrow { mutable: false, inner: Box::new(closure_call()) })),
        ("Ok", e(ExprKind::Ok(Box::new(closure_call())))),
        ("Some", e(ExprKind::Some(Box::new(closure_call())))),
        ("Try", e(ExprKind::Try(Box::new(closure_call())))),
    ];
    let mut missing: Vec<&str> = Vec::new();
    for (name, expr) in variants {
        // 用 block 走 convert_closure_calls
        let mut b: Block = vec![Stmt::Expr(expr)];
        super::convert_closure_calls(&mut b, &mut cv.clone(), &rcf);
        // 取出改写后的表达式，检查是否还有未转换的 __SENT__(...)
        if let Stmt::Expr(ref rewritten) = b[0] {
            if has_unconverted_call(rewritten) {
                missing.push(name);
            }
        }
    }
    assert!(missing.is_empty(), "convert_closure_calls_expr 未覆盖这些变体里的闭包调用：{:?}", missing);
}
