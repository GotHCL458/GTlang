//! opt pass 的单元测试：常量折叠与常量传播（AST 级）。
#![cfg(test)]

use crate::ast::*;

fn e(kind: ExprKind) -> Expr { Expr::new(kind, 0) }

fn lit_int(v: i64) -> Expr { e(ExprKind::Int(v)) }
fn lit_bool(v: bool) -> Expr { e(ExprKind::Bool(v)) }

fn let_stmt(name: &str, value: Expr) -> Stmt {
    Stmt::Let { name: name.into(), ty: None, value, line: 0, mutable: false }
}

/// 取出块尾表达式（用于断言折叠结果）。
fn tail_expr(b: &Block) -> &Expr {
    match b.last() {
        Some(Stmt::Expr(x)) => x,
        other => panic!("expected tail expr, got {:?}", other),
    }
}

#[test]
fn folds_int_add() {
    let mut b: Block = vec![Stmt::Expr(e(ExprKind::Binary(BinOp::Add, Box::new(lit_int(2)), Box::new(lit_int(3)))))];
    super::optimize_block(&mut b);
    match &tail_expr(&b).kind {
        ExprKind::Int(5) => {}
        other => panic!("expected folded Int(5), got {:?}", other),
    }
}

#[test]
fn folds_nested_arithmetic() {
    // (1 + 2) * (3 - 1) = 6
    let inner1 = e(ExprKind::Binary(BinOp::Add, Box::new(lit_int(1)), Box::new(lit_int(2))));
    let inner2 = e(ExprKind::Binary(BinOp::Sub, Box::new(lit_int(3)), Box::new(lit_int(1))));
    let mut b: Block = vec![Stmt::Expr(e(ExprKind::Binary(BinOp::Mul, Box::new(inner1), Box::new(inner2))))];
    super::optimize_block(&mut b);
    match &tail_expr(&b).kind {
        ExprKind::Int(6) => {}
        other => panic!("expected Int(6), got {:?}", other),
    }
}

#[test]
fn folds_unary_neg() {
    let mut b: Block = vec![Stmt::Expr(e(ExprKind::Unary(UnOp::Neg, Box::new(lit_int(7)))))];
    super::optimize_block(&mut b);
    match &tail_expr(&b).kind {
        ExprKind::Int(-7) => {}
        other => panic!("expected Int(-7), got {:?}", other),
    }
}

#[test]
fn folds_bool_not() {
    let mut b: Block = vec![Stmt::Expr(e(ExprKind::Unary(UnOp::Not, Box::new(lit_bool(true)))))];
    super::optimize_block(&mut b);
    match &tail_expr(&b).kind {
        ExprKind::Bool(false) => {}
        other => panic!("expected Bool(false), got {:?}", other),
    }
}

#[test]
fn does_not_fold_non_literals() {
    // x + 1（x 非字面量）不应折叠
    let mut b: Block = vec![Stmt::Expr(e(ExprKind::Binary(
        BinOp::Add,
        Box::new(e(ExprKind::Ident("x".into()))),
        Box::new(lit_int(1)),
    )))];
    super::optimize_block(&mut b);
    assert!(matches!(tail_expr(&b).kind, ExprKind::Binary(_, _, _)), "non-literal must not fold");
}

#[test]
fn propagate_replaces_const_binding() {
    // a := 5; put(a)  →  put(5)
    let mut b: Block = vec![
        let_stmt("a", lit_int(5)),
        Stmt::Expr(e(ExprKind::Call("put".into(), vec![e(ExprKind::Ident("a".into()))]))),
    ];
    super::propagate_block(&mut b);
    // 尾部调用 put(...) 的参数应变成 Int(5)
    match &tail_expr(&b).kind {
        ExprKind::Call(_, args) => match &args[0].kind {
            ExprKind::Int(5) => {}
            other => panic!("expected propagated Int(5), got {:?}", other),
        },
        other => panic!("expected Call, got {:?}", other),
    }
}

#[test]
fn propagate_skips_assigned_variable() {
    // a := 5; a = 6; put(a)  →  a 被赋值过，不传播
    let mut b: Block = vec![
        let_stmt("a", lit_int(5)),
        Stmt::Assign { name: "a".into(), index: None, op: None, value: lit_int(6), line: 0 },
        Stmt::Expr(e(ExprKind::Call("put".into(), vec![e(ExprKind::Ident("a".into()))]))),
    ];
    super::propagate_block(&mut b);
    match &tail_expr(&b).kind {
        ExprKind::Call(_, args) => assert!(matches!(args[0].kind, ExprKind::Ident(_)), "assigned var must not be propagated"),
        other => panic!("expected Call, got {:?}", other),
    }
}

#[test]
fn propagate_does_not_replace_string() {
    // s := "x" 的字符串有 move 语义，不应传播
    let mut b: Block = vec![
        let_stmt("s", e(ExprKind::Str("x".into()))),
        Stmt::Expr(e(ExprKind::Call("put".into(), vec![e(ExprKind::Ident("s".into()))]))),
    ];
    super::propagate_block(&mut b);
    match &tail_expr(&b).kind {
        ExprKind::Call(_, args) => assert!(matches!(args[0].kind, ExprKind::Ident(_)), "string must not propagate"),
        other => panic!("expected Call, got {:?}", other),
    }
}
