//! 所有权 / 借用检查的单元测试。
#![cfg(test)]

use crate::ast::*;

fn e(kind: ExprKind) -> Expr { Expr::new(kind, 0) }

/// 构造一个参数化函数（用于给变量提供类型）。
fn fn_with(params: Vec<(&str, Ty)>, body: Block) -> Item {
    Item::Fn(FnDef {
        name: "main".into(),
        type_params: vec![],
        params: params.into_iter().map(|(n, t)| Param { name: n.into(), ty: Some(t), default: None, line: 0 }).collect(),
        ret: None,
        ret_ty: Ty::Void,
        body,
        line: 0,
        is_pub: false,
        bounds: vec![],
        attrs: vec![],
    })
}

fn prog(items: Vec<Item>) -> Program { Program { items, ..Default::default() } }

fn let_stmt(name: &str, value: Expr) -> Stmt {
    Stmt::Let { name: name.into(), ty: None, value, line: 0, mutable: false }
}

#[test]
fn move_then_use_reported() {
    // a: struct（非 Copy）；b := a（move）；put(a)（use after move）
    let body: Block = vec![
        let_stmt("b", e(ExprKind::Ident("a".into()))),
        Stmt::Expr(e(ExprKind::Call("put".into(), vec![e(ExprKind::Ident("a".into()))]))),
    ];
    let p = prog(vec![fn_with(vec![("a", Ty::Struct("P".into()))], body)]);
    let errs = super::check(&p);
    assert!(!errs.is_empty(), "expected move error");
}

#[test]
fn scalar_copy_not_moved() {
    // a: int（Copy）；b := a；put(a) 合法
    let body: Block = vec![
        let_stmt("b", e(ExprKind::Ident("a".into()))),
        Stmt::Expr(e(ExprKind::Call("put".into(), vec![e(ExprKind::Ident("a".into()))]))),
    ];
    let p = prog(vec![fn_with(vec![("a", Ty::I64)], body)]);
    let errs = super::check(&p);
    assert!(errs.is_empty(), "scalar copy must be allowed, got {:?}", errs);
}

#[test]
fn container_is_copy_semantics() {
    // list 是引用语义（视为 Copy），不触发 move
    let body: Block = vec![
        let_stmt("b", e(ExprKind::Ident("a".into()))),
        Stmt::Expr(e(ExprKind::Call("put".into(), vec![e(ExprKind::Ident("a".into()))]))),
    ];
    let p = prog(vec![fn_with(vec![("a", Ty::List(Box::new(Ty::I64)))], body)]);
    let errs = super::check(&p);
    assert!(errs.is_empty(), "container is copy-semantics, got {:?}", errs);
}

#[test]
fn clean_program_no_errors() {
    let body: Block = vec![Stmt::Expr(e(ExprKind::Call("put".into(), vec![e(ExprKind::Int(1))])))];
    let p = prog(vec![fn_with(vec![], body)]);
    assert!(super::check(&p).is_empty());
}

#[test]
fn is_copy_rules() {
    assert!(super::is_copy(&Ty::I64));
    assert!(super::is_copy(&Ty::F64));
    assert!(super::is_copy(&Ty::Bool));
    assert!(super::is_copy(&Ty::List(Box::new(Ty::I64))));
    assert!(super::is_copy(&Ty::Ref(Box::new(Ty::I64))));
    assert!(!super::is_copy(&Ty::Str));
    assert!(!super::is_copy(&Ty::Struct("P".into())));
    assert!(!super::is_copy(&Ty::Array(Box::new(Ty::I64), 3)));
}
