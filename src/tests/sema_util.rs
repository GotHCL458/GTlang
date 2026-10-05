//! AST 遍历完备性测试：确保 each_expr / each_expr_block 能到达每个 ExprKind 变体的子表达式。
//!
//! 新增 ExprKind 变体时，若忘记在 each_expr 里递归，这里的"哨兵"就会漏访问 → 测试失败。

#![cfg(test)]

use crate::ast::*;

/// 构造一个"哨兵"表达式（唯一可识别的标识符）。
fn sentinel() -> Expr {
    Expr::new(ExprKind::Ident("__SENTINEL__".into()), 0)
}

fn e(kind: ExprKind) -> Expr {
    Expr::new(kind, 0)
}

fn s() -> Box<Expr> {
    Box::new(sentinel())
}

fn block_with(e: Expr) -> Block {
    vec![Stmt::Expr(e)]
}

/// 用 each_expr 遍历，返回是否访问到哨兵。
fn visited(expr: &Expr) -> bool {
    let mut found = false;
    super::sema_util::each_expr(expr, &mut |x| {
        if let ExprKind::Ident(n) = &x.kind {
            if n == "__SENTINEL__" { found = true; }
        }
    });
    found
}

/// 每个变体：把哨兵放进一个"应被遍历的子位置"。
fn all_variants() -> Vec<(&'static str, Expr)> {
    let mut v: Vec<(&'static str, Expr)> = Vec::new();
    v.push(("Unary", e(ExprKind::Unary(UnOp::Neg, s()))));
    v.push(("Binary", e(ExprKind::Binary(BinOp::Add, s(), s()))));
    v.push(("Call", e(ExprKind::Call("f".into(), vec![sentinel()]))));
    v.push(("CallNamed", e(ExprKind::CallNamed("f".into(), vec![("a".into(), sentinel())]))));
    v.push(("Index", e(ExprKind::Index(s(), s()))));
    v.push(("Slice", e(ExprKind::Slice(s(), s(), s()))));
    v.push(("ArrayLit", e(ExprKind::ArrayLit(vec![sentinel()]))));
    v.push(("TupleLit", e(ExprKind::TupleLit(vec![sentinel()]))));
    v.push(("ListComp", e(ExprKind::ListComp { expr: s(), var: "x".into(), iter: s(), cond: Some(s()) })));
    v.push(("Field", e(ExprKind::Field(s(), "f".into()))));
    v.push(("StructLit", e(ExprKind::StructLit("S".into(), vec![("a".into(), sentinel())]))));
    v.push(("EnumLit", e(ExprKind::EnumLit("E".into(), "V".into(), vec![sentinel()]))));
    v.push(("DynBox", e(ExprKind::DynBox { trait_name: "T".into(), value: s() })));
    v.push(("If", e(ExprKind::If { cond: s(), then: block_with(sentinel()), els: Some(block_with(sentinel())) })));
    v.push(("Match", e(ExprKind::Match { subject: s(), arms: vec![MatchArm { pat: Some(sentinel()), range: None, guard: Some(sentinel()), body: block_with(sentinel()), line: 0 }] })));
    v.push(("Closure", e(ExprKind::Closure { params: vec![], param_tys: vec![], ret_ty: None, body: s(), line: 0 })));
    v.push(("CallValue", e(ExprKind::CallValue { callee: s(), args: vec![sentinel()] })));
    v.push(("MethodOn", e(ExprKind::MethodOn { recv: s(), method: "m".into(), args: vec![sentinel()] })));
    v.push(("ClosureNew", e(ExprKind::ClosureNew { fn_name: "c".into(), captures: vec![sentinel()] })));
    v.push(("Borrow", e(ExprKind::Borrow { mutable: false, inner: s() })));
    v.push(("Ok", e(ExprKind::Ok(s()))));
    v.push(("Err", e(ExprKind::Err(s()))));
    v.push(("Try", e(ExprKind::Try(s()))));
    v.push(("Some", e(ExprKind::Some(s()))));
    v.push(("TryBlock", e(ExprKind::TryBlock { body: block_with(sentinel()), catches: vec![CatchArm { binding: Some("e".into()), label: None, guard: Some(sentinel()), body: block_with(sentinel()), line: 0 }], fin: Some(block_with(sentinel())) })));
    v
}

#[test]
fn each_expr_covers_all_variants() {
    let mut missing: Vec<&str> = Vec::new();
    for (name, expr) in all_variants() {
        if !visited(&expr) {
            missing.push(name);
        }
    }
    assert!(missing.is_empty(), "each_expr 未遍历到这些变体的子表达式（请在 each_expr 里补递归）：{:?}", missing);
}
