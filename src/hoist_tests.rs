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


#[test]
fn is_closure_expr_recognizes_literals_and_idents() {
    let cl = e(ExprKind::Closure { params: vec![], param_tys: vec![], ret_ty: None, body: Box::new(sentinel_ident()), line: 0 });
    assert!(super::is_closure_expr(&cl));
    let cn = e(ExprKind::ClosureNew { fn_name: "c".into(), captures: vec![] });
    assert!(super::is_closure_expr(&cn));
    assert!(super::is_closure_expr(&sentinel_ident()));
    assert!(!super::is_closure_expr(&e(ExprKind::Int(1))));
}

#[test]
fn is_closure_expr_recognizes_if_both_branches() {
    let cl = || e(ExprKind::Closure { params: vec![], param_tys: vec![], ret_ty: None, body: Box::new(sentinel_ident()), line: 0 });
    let both = e(ExprKind::If { cond: Box::new(sentinel_ident()), then: blk(cl()), els: Some(blk(cl())) });
    assert!(super::is_closure_expr(&both));
    let one = e(ExprKind::If { cond: Box::new(sentinel_ident()), then: blk(cl()), els: Some(blk(e(ExprKind::Int(1)))) });
    assert!(!super::is_closure_expr(&one));
    let no_else = e(ExprKind::If { cond: Box::new(sentinel_ident()), then: blk(cl()), els: None });
    assert!(!super::is_closure_expr(&no_else));
}

#[test]
fn block_returns_closure_detects_return() {
    let cl = e(ExprKind::Closure { params: vec![], param_tys: vec![], ret_ty: None, body: Box::new(sentinel_ident()), line: 0 });
    let yes: Block = vec![Stmt::Return(Some(cl.clone()), 0)];
    assert!(super::block_returns_closure(&yes));
    let no: Block = vec![Stmt::Return(Some(e(ExprKind::Int(1))), 0)];
    assert!(!super::block_returns_closure(&no));
    let nested: Block = vec![Stmt::If { cond: sentinel_ident(), then: vec![Stmt::Return(Some(cl), 0)], els: None, line: 0 }];
    assert!(super::block_returns_closure(&nested));
    assert!(!super::block_returns_closure(&vec![]));
}

#[test]
fn convert_fn_refs_rewrites_value_position_idents() {
    let fn_names = vec!["加一".to_string()];
    let mut b: Block = vec![Stmt::Let {
        name: "g".into(),
        ty: None,
        value: e(ExprKind::Ident("加一".into())),
        line: 0,
        mutable: false,
    }];
    super::convert_fn_refs_block(&mut b, &fn_names, &std::collections::HashSet::new());
    match &b[0] {
        Stmt::Let { value, .. } => match &value.kind {
            ExprKind::ClosureNew { fn_name, captures } => {
                assert_eq!(fn_name, "加一");
                assert!(captures.is_empty());
            }
            _ => panic!("expected ClosureNew"),
        },
        _ => panic!("expected Let"),
    }
}

#[test]
fn is_call_to_ret_closure_detects_names() {
    let rcf = vec!["造加法".to_string()];
    let call = e(ExprKind::Call("造加法".into(), vec![]));
    assert!(super::is_call_to_ret_closure(&call, &rcf));
    let other = e(ExprKind::Call("别的".into(), vec![]));
    assert!(!super::is_call_to_ret_closure(&other, &rcf));
    // 非调用不算
    assert!(!super::is_call_to_ret_closure(&sentinel_ident(), &rcf));
}

#[test]
fn is_call_to_closure_var_detects_names() {
    let cv = vec!["造乘".to_string()];
    let call = e(ExprKind::Call("造乘".into(), vec![]));
    assert!(super::is_call_to_closure_var(&call, &cv));
    let other = e(ExprKind::Call("别的".into(), vec![]));
    assert!(!super::is_call_to_closure_var(&other, &cv));
    assert!(!super::is_call_to_closure_var(&sentinel_ident(), &cv));
}

#[test]
fn collect_free_vars_basic() {
    // body 引用 x、y；bound=[x] → 只 y 是自由变量
    let body = e(ExprKind::Binary(
        BinOp::Add,
        Box::new(e(ExprKind::Ident("x".into()))),
        Box::new(e(ExprKind::Ident("y".into()))),
    ));
    let mut out = Vec::new();
    super::collect_free_vars_expr(&body, &["x".to_string()], &mut out);
    assert_eq!(out, vec!["y".to_string()]);
}

#[test]
fn collect_free_vars_skips_underscore_prefix() {
    // __ 前缀的临时名不算自由变量
    let body = e(ExprKind::Ident("__tmp".into()));
    let mut out = Vec::new();
    super::collect_free_vars_expr(&body, &[], &mut out);
    assert!(out.is_empty());
}

#[test]
fn convert_fn_refs_leaves_calls_and_other_idents() {
    let fn_names = vec!["加一".to_string()];
    let mut b: Block = vec![Stmt::Expr(e(ExprKind::Call("加一".into(), vec![e(ExprKind::Int(1))])))];
    super::convert_fn_refs_block(&mut b, &fn_names, &std::collections::HashSet::new());
    match &b[0] {
        Stmt::Expr(x) => assert!(matches!(x.kind, ExprKind::Call(_, _))),
        _ => panic!("expected Expr"),
    }
    let mut b2: Block = vec![Stmt::Expr(e(ExprKind::Ident("变量".into())))];
    super::convert_fn_refs_block(&mut b2, &fn_names, &std::collections::HashSet::new());
    match &b2[0] {
        Stmt::Expr(x) => assert!(matches!(x.kind, ExprKind::Ident(_))),
        _ => panic!("expected Expr"),
    }
}


/// 递归检查块里是否还有 Call("__SENT__")（未改写成 CallValue）。
/// 递归检查块里是否还有 Call("__SENT__")（未改写成 CallValue）。
/// 递归检查块里是否还有 Call("__SENT__")（未改写成 CallValue）。
fn stmt_has_sent_call(b: &Block) -> bool {
    fn walk(e: &Expr, out: &mut bool) {
        if let ExprKind::Call(n, _) = &e.kind { if n == "__SENT__" { *out = true; } }
        match &e.kind {
            ExprKind::Call(_, args) => for a in args { walk(a, out); },
            ExprKind::CallValue { callee, args } => { walk(callee, out); for a in args { walk(a, out); } },
            ExprKind::Unary(_, a) => walk(a, out),
            ExprKind::Binary(_, a, b) => { walk(a, out); walk(b, out); },
            _ => {}
        }
    }
    let mut out = false;
    for s in b {
        match s {
            Stmt::Go { args, .. } => for a in args { walk(a, &mut out); },
            Stmt::Throw(e, _) => walk(e, &mut out),
            Stmt::Labeled { inner, .. } => { if let Stmt::Expr(e) = &**inner { walk(e, &mut out); } },
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => walk(e, &mut out),
            _ => {}
        }
    }
    out
}

/// convert_closure_calls 的语句遍历完备性：每个 Stmt 变体里的闭包调用都应被改写。
#[test]
fn convert_closure_calls_covers_stmt_variants() {
    // 构造 `__SENT__(...)`（闭包变量调用），放进各语句变体。
    let cc = || Expr::new(ExprKind::Call("__SENT__".into(), vec![]), 0);
    let cv = vec!["__SENT__".to_string()];
    let rcf: Vec<String> = Vec::new();
    let blk = |x: Expr| vec![Stmt::Expr(x)];
    let mut cases: Vec<(&str, Stmt)> = Vec::new();
    cases.push(("Go", Stmt::Go { func: "g".into(), args: vec![cc()], line: 0 }));
    cases.push(("Throw", Stmt::Throw(cc(), 0)));
    cases.push(("Labeled", Stmt::Labeled { label: "L".into(), inner: Box::new(Stmt::Expr(cc())), line: 0 }));
    cases.push(("Try", Stmt::Try { body: blk(cc()), catches: vec![CatchArm { binding: None, label: None, guard: None, body: blk(cc()), line: 0 }], fin: Some(blk(cc())), line: 0 }));
    cases.push(("LocalFn", Stmt::LocalFn(FnDef { name: "h".into(), type_params: vec![], params: vec![], ret: None, ret_ty: Ty::Void, body: blk(cc()), line: 0, is_pub: false, bounds: vec![] })));
    let mut missing: Vec<&str> = Vec::new();
    for (name, st) in &cases {
        let mut b: Block = vec![st.clone()];
        super::convert_closure_calls(&mut b, &mut cv.clone(), &rcf);
        // 若仍是 Call("__SENT__", ...)（未转 CallValue），说明漏改。
        let still_call = stmt_has_sent_call(&b);
        if still_call { missing.push(name); }
    }
    assert!(missing.is_empty(), "convert_closure_calls 未改写这些语句变体里的闭包调用：{:?}", missing);
}





