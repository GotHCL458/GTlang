/// 命名参数：CallNamed 按形参名重排为 Call（需 FnDef 形参名）。
pub fn resolve_named(prog: &mut Program) {
    use std::collections::HashMap;
    let mut params: HashMap<String, Vec<String>> = HashMap::new();
    for item in &prog.items {
        if let Item::Fn(f) = item {
            params.insert(f.name.clone(), f.params.iter().map(|p| p.name.clone()).collect());
        }
    }
    for item in prog.items.iter_mut() {
        match item {
            Item::Fn(f) => resolve_block(&mut f.body, &params),
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods.iter_mut() { resolve_block(&mut m.body, &params); }
            }
            _ => {}
        }
    }
}

fn resolve_block(b: &mut Block, params: &std::collections::HashMap<String, Vec<String>>) {
    for s in b.iter_mut() { resolve_stmt(s, params); }
}

fn resolve_stmt(s: &mut Stmt, params: &std::collections::HashMap<String, Vec<String>>) {
    match s {
        Stmt::Let { value, .. } | Stmt::Const { value, .. } => resolve_expr(value, params),
        Stmt::Assign { value, index, .. } => { resolve_expr(value, params); if let Some(i) = index { resolve_expr(i, params); } }
        Stmt::FieldAssign { value, .. } => resolve_expr(value, params),
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => resolve_expr(e, params),
        Stmt::If { cond, then, els, .. } => { resolve_expr(cond, params); resolve_block(then, params); if let Some(e) = els { resolve_block(e, params); } }
        Stmt::While { cond, body, .. } => { resolve_expr(cond, params); resolve_block(body, params); }
        Stmt::DoWhile { body, cond, .. } => { resolve_block(body, params); resolve_expr(cond, params); }
        Stmt::ForRange { from, to, body, els, .. } => { resolve_expr(from, params); resolve_expr(to, params); resolve_block(body, params); if let Some(e) = els { resolve_block(e, params); } }
        Stmt::ForEach { iter, body, els, .. } => { resolve_expr(iter, params); resolve_block(body, params); if let Some(e) = els { resolve_block(e, params); } }
        Stmt::Block(inner) => resolve_block(inner, params),
        Stmt::Throw(e, _) => resolve_expr(e, params),
        Stmt::Try { body, catches, fin, .. } => {
            resolve_block(body, params);
            for ca in catches { resolve_block(&mut ca.body, params); }
            if let Some(f) = fin { resolve_block(f, params); }
        }
        _ => {}
    }
}

fn resolve_expr(e: &mut Expr, params: &std::collections::HashMap<String, Vec<String>>) {
    match &mut e.kind {
        ExprKind::Unary(_, a) => resolve_expr(a, params),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => { resolve_expr(a, params); resolve_expr(b, params); }
        ExprKind::Call(_, args) => for a in args { resolve_expr(a, params); },
        ExprKind::CallValue { callee, args } => { resolve_expr(callee, params); for a in args { resolve_expr(a, params); } }
        ExprKind::ArrayLit(xs) => for x in xs { resolve_expr(x, params); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { resolve_expr(i, params); } },
        ExprKind::Field(base, _) => resolve_expr(base, params),
        ExprKind::StructLit(_, fs) => for (_, v) in fs { resolve_expr(v, params); },
        ExprKind::If { cond, then, els } => { resolve_expr(cond, params); resolve_block(then, params); if let Some(x) = els { resolve_block(x, params); } }
        ExprKind::Match { subject, arms } => {
            resolve_expr(subject, params);
            for arm in arms {
                if let Some(p) = &mut arm.pat { resolve_expr(p, params); }
                if let Some((lo, hi)) = &mut arm.range { resolve_expr(lo, params); resolve_expr(hi, params); }
                if let Some(g) = &mut arm.guard { resolve_expr(g, params); }
                resolve_block(&mut arm.body, params);
            }
        }
        _ => {}
    }
    if let ExprKind::CallNamed(name, all) = &mut e.kind {
        let mut ordered: Vec<Expr> = Vec::new();
        if let Some(ps) = params.get(name) {
            let mut slot: Vec<Option<Expr>> = vec![None; ps.len()];
            let mut pos = 0usize;
            for (n, v) in all.iter() {
                let mut vv = v.clone();
                resolve_expr(&mut vv, params);
                if n.is_empty() {
                    if pos < slot.len() { slot[pos] = Some(vv); pos += 1; }
                } else if let Some(i) = ps.iter().position(|p| p == n) {
                    slot[i] = Some(vv);
                }
            }
            for s in slot { if let Some(v) = s { ordered.push(v); } }
        } else {
            for (_, v) in all.iter() { ordered.push(v.clone()); }
        }
        e.kind = ExprKind::Call(name.clone(), ordered);
    }
}

/// 列表推导 desugar：`[e for v in it if c]` -> `if true { acc := []; for v in it { if c { push(acc, e) } }; acc }`。
pub fn expand_list_comp(prog: &mut Program) {
    for item in prog.items.iter_mut() {
        match item {
            Item::Fn(f) => expand_block(&mut f.body),
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods.iter_mut() { expand_block(&mut m.body); }
            }
            _ => {}
        }
    }
}

fn expand_block(b: &mut Block) {
    for s in b.iter_mut() { expand_stmt(s); }
}

fn expand_stmt(s: &mut Stmt) {
    match s {
        Stmt::Let { value, .. } | Stmt::Const { value, .. } => expand_expr(value),
        Stmt::Assign { value, index, .. } => { expand_expr(value); if let Some(i) = index { expand_expr(i); } }
        Stmt::FieldAssign { value, .. } => expand_expr(value),
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => expand_expr(e),
        Stmt::If { cond, then, els, .. } => { expand_expr(cond); expand_block(then); if let Some(e) = els { expand_block(e); } }
        Stmt::While { cond, body, .. } => { expand_expr(cond); expand_block(body); }
        Stmt::DoWhile { body, cond, .. } => { expand_block(body); expand_expr(cond); }
        Stmt::ForRange { from, to, body, els, .. } => { expand_expr(from); expand_expr(to); expand_block(body); if let Some(e) = els { expand_block(e); } }
        Stmt::ForEach { iter, body, els, .. } => { expand_expr(iter); expand_block(body); if let Some(e) = els { expand_block(e); } }
        Stmt::Block(inner) => expand_block(inner),
        Stmt::Throw(e, _) => expand_expr(e),
        Stmt::Try { body, catches, fin, .. } => {
            expand_block(body);
            for ca in catches { expand_block(&mut ca.body); }
            if let Some(f) = fin { expand_block(f); }
        }
        _ => {}
    }
}

fn expand_expr(e: &mut Expr) {
    // 先递归
    match &mut e.kind {
        ExprKind::Unary(_, a) => expand_expr(a),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => { expand_expr(a); expand_expr(b); }
        ExprKind::Call(_, args) => for a in args { expand_expr(a); },
        ExprKind::CallNamed(_, named) => for (_, a) in named { expand_expr(a); },
        ExprKind::ArrayLit(xs) | ExprKind::TupleLit(xs) => for x in xs { expand_expr(x); },
        ExprKind::Slice(a, b, c) => { expand_expr(a); expand_expr(b); expand_expr(c); }
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { expand_expr(i); } },
        ExprKind::Field(base, _) => expand_expr(base),
        ExprKind::StructLit(_, fs) => for (_, v) in fs { expand_expr(v); },
        ExprKind::If { cond, then, els } => { expand_expr(cond); expand_block(then); if let Some(x) = els { expand_block(x); } }
        ExprKind::Match { subject, arms } => {
            expand_expr(subject);
            for arm in arms { if let Some(g) = &mut arm.guard { expand_expr(g); } expand_block(&mut arm.body); }
        }
        ExprKind::CallValue { callee, args } => { expand_expr(callee); for a in args { expand_expr(a); } }
        ExprKind::ClosureNew { captures, .. } => for c in captures { expand_expr(c); },
        ExprKind::Borrow { inner, .. } => expand_expr(inner),
        ExprKind::Ok(x) | ExprKind::Err(x) | ExprKind::Some(x) | ExprKind::Try(x) => expand_expr(x),
        ExprKind::TryOr { inner, default } => { expand_expr(inner); expand_expr(default); }
        ExprKind::TryBlock { body, catches, fin } => {
            expand_block(body);
            for ca in catches { expand_block(&mut ca.body); }
            if let Some(f) = fin { expand_block(f); }
        }
        ExprKind::ListComp { expr, iter, cond, .. } => { expand_expr(expr); expand_expr(iter); if let Some(c) = cond { expand_expr(c); } }
        _ => {}
    }
    // 展开 ListComp
    if let ExprKind::ListComp { expr, var, iter, cond } = &e.kind {
        let acc = format!("__lc{}", e.line);
        let mut stmts: Block = Vec::new();
        // acc := list()
        stmts.push(Stmt::Let { name: acc.clone(), ty: None, value: Expr::new(ExprKind::Call("list".into(), vec![]), e.line), mutable: true, line: e.line });
        // for v in iter { if cond { push(acc, expr) } }
        let mut body: Block = Vec::new();
        let push = Expr::new(ExprKind::Call("push".into(), vec![Expr::new(ExprKind::Ident(acc.clone()), e.line), (**expr).clone()]), e.line);
        match cond {
            Some(c) => {
                body.push(Stmt::If { cond: (**c).clone(), then: vec![Stmt::Expr(push)], els: None, line: e.line });
            }
            None => body.push(Stmt::Expr(push)),
        }
        stmts.push(Stmt::ForEach { var: var.clone(), iter: (**iter).clone(), body, els: None, line: e.line });
        // 块尾 acc
        stmts.push(Stmt::Expr(Expr::new(ExprKind::Ident(acc.clone()), e.line)));
        e.kind = ExprKind::If { cond: Box::new(Expr::new(ExprKind::Bool(true), e.line)), then: stmts, els: None };
    }
}


/// 展开声明式宏（调用点替换为模板）。
pub fn expand_macros(prog: &mut Program) {
    use std::collections::HashMap;
    let mut macros: HashMap<String, (Vec<String>, Expr)> = HashMap::new();
    for item in &prog.items {
        if let Item::Macro { name, params, body, .. } = item {
            macros.insert(name.clone(), (params.clone(), body.clone()));
        }
    }
    if macros.is_empty() { return; }
    for item in prog.items.iter_mut() {
        match item {
            Item::Fn(f) => macro_expand_block(&mut f.body, &macros),
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods.iter_mut() { macro_expand_block(&mut m.body, &macros); }
            }
            Item::Const { value, .. } => macro_expand_expr(value, &macros),
            _ => {}
        }
    }
}

const MACRO_MAX_DEPTH: usize = 64;

fn macro_expand_block(b: &mut Block, macros: &std::collections::HashMap<String, (Vec<String>, Expr)>) {
    for s in b.iter_mut() { macro_expand_stmt(s, macros); }
}

fn macro_expand_stmt(s: &mut Stmt, macros: &std::collections::HashMap<String, (Vec<String>, Expr)>) {
    match s {
        Stmt::Let { value, .. } | Stmt::Const { value, .. } => macro_expand_expr(value, macros),
        Stmt::Assign { value, index, .. } => { macro_expand_expr(value, macros); if let Some(i) = index { macro_expand_expr(i, macros); } }
        Stmt::FieldAssign { value, .. } => macro_expand_expr(value, macros),
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => macro_expand_expr(e, macros),
        Stmt::If { cond, then, els, .. } => { macro_expand_expr(cond, macros); macro_expand_block(then, macros); if let Some(e) = els { macro_expand_block(e, macros); } }
        Stmt::While { cond, body, .. } => { macro_expand_expr(cond, macros); macro_expand_block(body, macros); }
        Stmt::DoWhile { body, cond, .. } => { macro_expand_block(body, macros); macro_expand_expr(cond, macros); }
        Stmt::ForRange { from, to, body, els, .. } => { macro_expand_expr(from, macros); macro_expand_expr(to, macros); macro_expand_block(body, macros); if let Some(e) = els { macro_expand_block(e, macros); } }
        Stmt::ForEach { iter, body, els, .. } => { macro_expand_expr(iter, macros); macro_expand_block(body, macros); if let Some(e) = els { macro_expand_block(e, macros); } }
        Stmt::Block(inner) => macro_expand_block(inner, macros),
        Stmt::Throw(e, _) => macro_expand_expr(e, macros),
        Stmt::Try { body, catches, fin, .. } => {
            macro_expand_block(body, macros);
            for ca in catches { macro_expand_block(&mut ca.body, macros); }
            if let Some(f) = fin { macro_expand_block(f, macros); }
        }
        _ => {}
    }
}

thread_local! {
    static MACRO_DEPTH: std::cell::Cell<usize> = std::cell::Cell::new(0);
}

fn macro_expand_expr(e: &mut Expr, macros: &std::collections::HashMap<String, (Vec<String>, Expr)>) {
    // 展开深度保护（防止宏无限递归，借 Vix 的 64 上限）
    if MACRO_DEPTH.with(|d| d.get()) >= MACRO_MAX_DEPTH { return; }
    // 先递归子表达式（宏可能嵌套）
    match &mut e.kind {
        ExprKind::Unary(_, a) => macro_expand_expr(a, macros),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => { macro_expand_expr(a, macros); macro_expand_expr(b, macros); }
        ExprKind::Call(_, args) => for a in args { macro_expand_expr(a, macros); },
        ExprKind::CallNamed(_, named) => for (_, a) in named { macro_expand_expr(a, macros); },
        ExprKind::ArrayLit(xs) | ExprKind::TupleLit(xs) => for x in xs { macro_expand_expr(x, macros); },
        ExprKind::Slice(a, b, c) => { macro_expand_expr(a, macros); macro_expand_expr(b, macros); macro_expand_expr(c, macros); }
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { macro_expand_expr(i, macros); } },
        ExprKind::Field(base, _) => macro_expand_expr(base, macros),
        ExprKind::StructLit(_, fs) => for (_, v) in fs { macro_expand_expr(v, macros); },
        ExprKind::If { cond, then, els } => { macro_expand_expr(cond, macros); macro_expand_block(then, macros); if let Some(x) = els { macro_expand_block(x, macros); } }
        ExprKind::Match { subject, arms } => {
            macro_expand_expr(subject, macros);
            for arm in arms { if let Some(g) = &mut arm.guard { macro_expand_expr(g, macros); } macro_expand_block(&mut arm.body, macros); }
        }
        ExprKind::CallValue { callee, args } => { macro_expand_expr(callee, macros); for a in args { macro_expand_expr(a, macros); } }
        ExprKind::ClosureNew { captures, .. } => for c in captures { macro_expand_expr(c, macros); },
        ExprKind::Borrow { inner, .. } => macro_expand_expr(inner, macros),
        ExprKind::Ok(x) | ExprKind::Err(x) | ExprKind::Some(x) | ExprKind::Try(x) => macro_expand_expr(x, macros),
        ExprKind::TryOr { inner, default } => { macro_expand_expr(inner, macros); macro_expand_expr(default, macros); }
        ExprKind::TryBlock { body, catches, fin } => {
            macro_expand_block(body, macros);
            for ca in catches { macro_expand_block(&mut ca.body, macros); }
            if let Some(f) = fin { macro_expand_block(f, macros); }
        }
        ExprKind::EnumLit(_, _, args) => for a in args { macro_expand_expr(a, macros); },
        ExprKind::ListComp { expr, iter, cond, .. } => { macro_expand_expr(expr, macros); macro_expand_expr(iter, macros); if let Some(c) = cond { macro_expand_expr(c, macros); } }
        _ => {}
    }
    // 展开宏调用：Call(名, 实参) 且名在宏表
    if let ExprKind::Call(name, args) = &e.kind {
        if let Some((params, body)) = macros.get(name) {
            if params.len() == args.len() {
                let mut subst = std::collections::HashMap::new();
                for (p, a) in params.iter().zip(args.iter()) {
                    subst.insert(p.clone(), a.clone());
                }
                let mut expanded = body.clone();
                macro_subst(&mut expanded, &subst);
                e.kind = expanded.kind;
                e.ty = expanded.ty;
                // 展开后再递归展开结果（深度 +1，超限即停）
                MACRO_DEPTH.with(|d| d.set(d.get() + 1));
                macro_expand_expr(e, macros);
                MACRO_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            }
        }
    }
}

fn macro_subst(e: &mut Expr, subst: &std::collections::HashMap<String, Expr>) {
    if let ExprKind::Ident(n) = &e.kind {
        if let Some(r) = subst.get(n) {
            e.kind = r.kind.clone();
            e.ty = r.ty.clone();
            return;
        }
    }
    match &mut e.kind {
        ExprKind::Unary(_, a) => macro_subst(a, subst),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => { macro_subst(a, subst); macro_subst(b, subst); }
        ExprKind::Call(_, args) => for a in args { macro_subst(a, subst); },
        ExprKind::CallNamed(_, named) => for (_, a) in named { macro_subst(a, subst); },
        ExprKind::ArrayLit(xs) | ExprKind::TupleLit(xs) => for x in xs { macro_subst(x, subst); },
        ExprKind::Borrow { inner, .. } => macro_subst(inner, subst),
        ExprKind::Field(b, _) => macro_subst(b, subst),
        ExprKind::StructLit(_, fs) => for (_, v) in fs { macro_subst(v, subst); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { macro_subst(i, subst); } },
        ExprKind::Ok(x) | ExprKind::Err(x) | ExprKind::Some(x) | ExprKind::Try(x) => macro_subst(x, subst),
        ExprKind::EnumLit(_, _, args) => for a in args { macro_subst(a, subst); },
        ExprKind::TryOr { inner, default } => { macro_subst(inner, subst); macro_subst(default, subst); }
        ExprKind::Slice(a, b, c) => { macro_subst(a, subst); macro_subst(b, subst); macro_subst(c, subst); }
        ExprKind::If { cond, then, els } => {
            macro_subst(cond, subst);
            for s in then.iter_mut() { macro_subst_stmt(s, subst); }
            if let Some(x) = els { for s in x.iter_mut() { macro_subst_stmt(s, subst); } }
        }
        _ => {}
    }
}

fn macro_subst_stmt(s: &mut Stmt, subst: &std::collections::HashMap<String, Expr>) {
    match s {
        Stmt::Let { value, .. } | Stmt::Const { value, .. } => macro_subst(value, subst),
        Stmt::Assign { value, index, .. } => { macro_subst(value, subst); if let Some(i) = index { macro_subst(i, subst); } }
        Stmt::FieldAssign { value, .. } => macro_subst(value, subst),
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => macro_subst(e, subst),
        Stmt::If { cond, then, els, .. } => { macro_subst(cond, subst); for s in then.iter_mut() { macro_subst_stmt(s, subst); } if let Some(e) = els { for s in e.iter_mut() { macro_subst_stmt(s, subst); } } }
        Stmt::While { cond, body, .. } => { macro_subst(cond, subst); for s in body.iter_mut() { macro_subst_stmt(s, subst); } }
        Stmt::DoWhile { body, cond, .. } => { for s in body.iter_mut() { macro_subst_stmt(s, subst); } macro_subst(cond, subst); }
        Stmt::Block(inner) => for s in inner.iter_mut() { macro_subst_stmt(s, subst); },
        Stmt::Throw(e, _) => macro_subst(e, subst),
        Stmt::ForRange { from, to, body, .. } => { macro_subst(from, subst); macro_subst(to, subst); for s in body.iter_mut() { macro_subst_stmt(s, subst); } }
        Stmt::ForEach { iter, body, .. } => { macro_subst(iter, subst); for s in body.iter_mut() { macro_subst_stmt(s, subst); } }
        _ => {}
    }
}


