//! lint 静态检查的单元测试。
#![cfg(test)]

use crate::ast::*;
use super::lint;

fn e(kind: ExprKind) -> Expr { Expr::new(kind, 0) }

fn fn_def(name: &str, body: Block, line: usize) -> FnDef {
    FnDef {
        name: name.into(),
        type_params: vec![],
        params: vec![],
        ret: None,
        ret_ty: Ty::Void,
        body,
        line,
        is_pub: false,
        bounds: vec![],
        attrs: vec![],
    }
}

fn prog(fns: Vec<FnDef>) -> Program {
    Program { items: fns.into_iter().map(Item::Fn).collect(), ..Default::default() }
}

fn has(msgs: &[(usize, String)], needle: &str) -> bool {
    msgs.iter().any(|(_, m)| m.contains(needle))
}

#[test]
fn unused_function_is_reported() {
    let p = prog(vec![
        fn_def("从未用", vec![], 3),
        fn_def("main", vec![], 1),
    ]);
    let msgs = lint(&p);
    assert!(has(&msgs, "从未被调用"), "got {:?}", msgs);
}

#[test]
fn used_function_is_not_reported() {
    // main 调用了 helper → helper 不算未用
    let body: Block = vec![Stmt::Expr(e(ExprKind::Call("helper".into(), vec![])))];
    let p = prog(vec![
        fn_def("helper", vec![], 2),
        fn_def("main", body, 1),
    ]);
    let msgs = lint(&p);
    assert!(!has(&msgs, "helper"), "got {:?}", msgs);
}

#[test]
fn unreachable_after_return() {
    let body: Block = vec![
        Stmt::Return(Some(e(ExprKind::Int(1))), 0),
        Stmt::Expr(e(ExprKind::Int(2))),
    ];
    let p = prog(vec![fn_def("main", body, 1)]);
    let msgs = lint(&p);
    assert!(has(&msgs, "不可达"), "got {:?}", msgs);
}

#[test]
fn const_condition_reported() {
    let body: Block = vec![Stmt::If {
        cond: e(ExprKind::Bool(true)),
        then: vec![Stmt::Expr(e(ExprKind::Int(1)))],
        els: None,
        line: 0,
    }];
    let p = prog(vec![fn_def("main", body, 1)]);
    let msgs = lint(&p);
    assert!(has(&msgs, "常量条件"), "got {:?}", msgs);
}

#[test]
fn empty_if_reported() {
    let body: Block = vec![Stmt::If {
        cond: e(ExprKind::Ident("c".into())),
        then: vec![],
        els: None,
        line: 0,
    }];
    let p = prog(vec![fn_def("main", body, 1)]);
    let msgs = lint(&p);
    assert!(has(&msgs, "为空"), "got {:?}", msgs);
}

#[test]
fn self_compare_reported() {
    let body: Block = vec![Stmt::Expr(e(ExprKind::Binary(
        BinOp::Eq,
        Box::new(e(ExprKind::Ident("x".into()))),
        Box::new(e(ExprKind::Ident("x".into()))),
    )))];
    let p = prog(vec![fn_def("main", body, 1)]);
    let msgs = lint(&p);
    assert!(has(&msgs, "自身") || has(&msgs, "自比较"), "got {:?}", msgs);
}

#[test]
fn clean_program_has_no_warnings() {
    let body: Block = vec![Stmt::Expr(e(ExprKind::Int(1)))];
    let p = prog(vec![fn_def("main", body, 1)]);
    let msgs = lint(&p);
    assert!(msgs.is_empty(), "expected no warnings, got {:?}", msgs);
}
