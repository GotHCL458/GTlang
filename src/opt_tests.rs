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


/// 哨兵调用：可被 collect_calls 识别的唯一函数名。
fn sentinel_call() -> Expr { e(ExprKind::Call("__SENT_CALL__".into(), vec![])) }

/// collect_calls 的遍历完备性：每个 ExprKind 变体的子位置都应被递归。
#[test]
fn collect_calls_covers_all_variants() {
    use std::collections::HashSet;
    let s = || Box::new(sentinel_call());
    let blk = || vec![Stmt::Expr(sentinel_call())];
    let mut cases: Vec<(&str, Expr)> = Vec::new();
    cases.push(("Unary", e(ExprKind::Unary(UnOp::Neg, s()))));
    cases.push(("Binary", e(ExprKind::Binary(BinOp::Add, s(), s()))));
    cases.push(("Call-arg", e(ExprKind::Call("outer".into(), vec![sentinel_call()]))));
    cases.push(("CallValue", e(ExprKind::CallValue { callee: s(), args: vec![sentinel_call()] })));
    cases.push(("MethodOn", e(ExprKind::MethodOn { recv: s(), method: "m".into(), args: vec![sentinel_call()] })));
    cases.push(("Borrow", e(ExprKind::Borrow { mutable: false, inner: s() })));
    cases.push(("DynBox", e(ExprKind::DynBox { trait_name: "T".into(), value: s() })));
    cases.push(("Index", e(ExprKind::Index(s(), s()))));
    cases.push(("Slice", e(ExprKind::Slice(s(), s(), s()))));
    cases.push(("TupleLit", e(ExprKind::TupleLit(vec![sentinel_call()]))));
    cases.push(("ListComp", e(ExprKind::ListComp { expr: s(), var: "x".into(), iter: s(), cond: Some(s()) })));
    cases.push(("Field", e(ExprKind::Field(s(), "f".into()))));
    cases.push(("StructLit", e(ExprKind::StructLit("S".into(), vec![("a".into(), sentinel_call())]))));
    cases.push(("EnumLit", e(ExprKind::EnumLit("E".into(), "V".into(), vec![sentinel_call()]))));
    cases.push(("If", e(ExprKind::If { cond: s(), then: blk(), els: Some(blk()) })));
    cases.push(("Match", e(ExprKind::Match { subject: s(), arms: vec![MatchArm { pat: Some(sentinel_call()), range: None, guard: Some(sentinel_call()), body: blk(), line: 0 }] })));
    cases.push(("Closure", e(ExprKind::Closure { params: vec![], param_tys: vec![], ret_ty: None, body: s(), line: 0 })));
    cases.push(("ClosureNew", e(ExprKind::ClosureNew { fn_name: "c".into(), captures: vec![sentinel_call()] })));
    cases.push(("Ok", e(ExprKind::Ok(s()))));
    cases.push(("Err", e(ExprKind::Err(s()))));
    cases.push(("Try", e(ExprKind::Try(s()))));
    cases.push(("Some", e(ExprKind::Some(s()))));
    cases.push(("Interp", e(ExprKind::Interp(vec![StrPart::Expr(s())]))));
    let mut missing: Vec<&str> = Vec::new();
    for (name, ex) in &cases {
        let mut out: HashSet<String> = HashSet::new();
        super::collect_calls(ex, &mut out);
        if !out.contains("__SENT_CALL__") { missing.push(name); }
    }
    assert!(missing.is_empty(), "collect_calls 未遍历这些变体的子表达式：{:?}", missing);
}



/// rename_expr（内联替换）的遍历完备性：每个变体里的哨兵 Ident 都应被替换。
#[test]
fn rename_expr_covers_all_variants() {
    use std::collections::HashMap;
    let sent = || Expr::new(ExprKind::Ident("__SENT__".into()), 0);
    let s = || Box::new(sent());
    let blk = || vec![Stmt::Expr(sent())];
    let mut subst: HashMap<String, Expr> = HashMap::new();
    subst.insert("__SENT__".into(), Expr::new(ExprKind::Int(99), 0));
    let rename: HashMap<String, String> = HashMap::new();
    let mut cases: Vec<(&str, Expr)> = Vec::new();
    cases.push(("Unary", e(ExprKind::Unary(UnOp::Neg, s()))));
    cases.push(("Binary", e(ExprKind::Binary(BinOp::Add, s(), s()))));
    cases.push(("Call-arg", e(ExprKind::Call("outer".into(), vec![sent()]))));
    cases.push(("CallValue", e(ExprKind::CallValue { callee: s(), args: vec![sent()] })));
    cases.push(("Index", e(ExprKind::Index(s(), s()))));
    cases.push(("ArrayLit", e(ExprKind::ArrayLit(vec![sent()]))));
    cases.push(("Interp", e(ExprKind::Interp(vec![StrPart::Expr(s())]))));
    cases.push(("Field", e(ExprKind::Field(s(), "f".into()))));
    cases.push(("StructLit", e(ExprKind::StructLit("S".into(), vec![("a".into(), sent())]))));
    cases.push(("If", e(ExprKind::If { cond: s(), then: blk(), els: Some(blk()) })));
    cases.push(("Closure", e(ExprKind::Closure { params: vec![], param_tys: vec![], ret_ty: None, body: s(), line: 0 })));
    cases.push(("ClosureNew", e(ExprKind::ClosureNew { fn_name: "c".into(), captures: vec![sent()] })));
    let mut missing: Vec<&str> = Vec::new();
    for (name, mut ex) in cases {
        let ok = super::rename_expr(&mut ex, &rename, &subst);
        // 替换后不应再含 __SENT__ 标识符（按字符串检查）
        let dbg = format!("{:?}", ex);
        if ok.is_some() && dbg.contains("__SENT__") { missing.push(name); }
    }
    assert!(missing.is_empty(), "rename_expr 未替换这些变体里的标识符：{:?}", missing);
}

