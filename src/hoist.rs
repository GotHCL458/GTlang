//! 嵌套函数"提升"：把函数内声明的 `fn` 移到顶层。
//!
//! 语义（对齐 Rust 的嵌套 fn）：嵌套函数**不捕获**外层局部变量，但可以：
//!   - 调用外层函数、其它顶层函数、标准库/内置；
//!   - 递归调用自身；
//!   - 调用同一外层函数里声明的其它嵌套函数。
//!
//! 实现：为每个嵌套函数生成唯一顶层名 `<外层>__<内层>`，并把该函数体内对
//! 内层函数名的调用改写为唯一名。处理完后 `Stmt::LocalFn` 从块中移除。

use std::collections::HashMap;

use crate::ast::*;

/// 对整份程序做降级：嵌套函数提升 + impl 方法展平。
pub fn hoist(prog: &mut Program) {
    let mut hoisted: Vec<FnDef> = Vec::new();
    // `类型.方法` → `类型__方法`（全局生效）
    let mut impl_scope: HashMap<String, String> = HashMap::new();
    // 闭包提升计数器
    let mut closure_counter: usize = 0;
    // 提升出来的闭包函数
    let mut closures: Vec<FnDef> = Vec::new();

    // trait 名 → 默认方法 (方法名, 参数名+类型, 返回类型, body)
    let mut trait_defaults: HashMap<String, Vec<(String, Vec<(String, Ty)>, Ty, Block)>> = HashMap::new();
    for item in &prog.items {
        if let Item::Trait(t) = item {
            trait_defaults.insert(t.name.clone(), t.defaults.clone());
        }
    }

    let mut new_items: Vec<Item> = Vec::new();
    for item in prog.items.drain(..) {
        match item {
            Item::Fn(mut f) => {
                let mut scope: HashMap<String, String> = HashMap::new();
                hoist_block(&mut f.body, &f.name, &mut scope, &mut hoisted);
                if !scope.is_empty() {
                    rewrite_calls_block(&mut f.body, &scope);
                }
                lift_closures_in_fn(&mut f, &mut closure_counter, &mut closures);
                // 参数名也可能是闭包（高阶函数）：把参数名当作闭包变量参与转换
                let mut cv: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
                convert_closure_calls(&mut f.body, &mut cv);
                new_items.push(Item::Fn(f));
            }
            // 泛型 impl（`impl[T] ...`）：方法保留为泛型函数，由 mono 单态化；不在此展平
            Item::TraitImpl { type_params, trait_name, ty, methods, line } if !type_params.is_empty() => {
                for mut m in methods {
                    m.type_params = type_params.clone();
                    let mut scope: HashMap<String, String> = HashMap::new();
                    hoist_block(&mut m.body, &m.name, &mut scope, &mut hoisted);
                    if !scope.is_empty() { rewrite_calls_block(&mut m.body, &scope); }
                    lift_closures_in_fn(&mut m, &mut closure_counter, &mut closures);
                    let mut cv: Vec<String> = m.params.iter().map(|p| p.name.clone()).collect();
                    convert_closure_calls(&mut m.body, &mut cv);
                    new_items.push(Item::Fn(m));
                }
                new_items.push(Item::TraitImpl { type_params, trait_name, ty, methods: Vec::new(), line });
            }
            // trait impl 与 impl 同样展平方法
            Item::TraitImpl { type_params: _, trait_name, ty, methods, line } => {
                let impl_names: Vec<String> = methods.iter().map(|m| m.name.clone()).collect();
                for mut m in methods {
                    impl_scope
                        .insert(format!("{}.{}", ty, m.name), format!("{}__{}", ty, m.name));
                    m.name = format!("{}__{}", ty, m.name);
                    if let Some(first) = m.params.first_mut() {
                        if first.name == "self" && first.ty.is_none() {
                            first.ty = Some(Ty::Struct(ty.clone()));
                        }
                    }
                    let mut scope: HashMap<String, String> = HashMap::new();
                    hoist_block(&mut m.body, &m.name, &mut scope, &mut hoisted);
                    if !scope.is_empty() {
                        rewrite_calls_block(&mut m.body, &scope);
                    }
                    lift_closures_in_fn(&mut m, &mut closure_counter, &mut closures);
                    let mut cv: Vec<String> = m.params.iter().map(|p| p.name.clone()).collect();
                    convert_closure_calls(&mut m.body, &mut cv);
                    new_items.push(Item::Fn(m));
                }
                // trait 默认方法：impl 未实现的补上（展平为 `类型__方法`）
                if let Some(defs) = trait_defaults.get(&trait_name).cloned() {
                    for (mname, pnames, ret, body) in defs {
                        if impl_names.contains(&mname) { continue; }
                        let uniq = format!("{}__{}", ty, mname);
                        impl_scope.insert(format!("{}.{}", ty, mname), uniq.clone());
                        let mut params: Vec<Param> = pnames.iter().map(|(n, t)| {
                            let ty2 = if n == "self" { Ty::Struct(ty.clone()) } else { t.clone() };
                            Param { name: n.clone(), ty: Some(ty2), default: None, line }
                        }).collect();
                        if params.is_empty() { params.push(Param { name: "self".into(), ty: Some(Ty::Struct(ty.clone())), default: None, line }); }
                        else if params[0].name == "self" { params[0].ty = Some(Ty::Struct(ty.clone())); }
                        let mut b2 = body.clone();
                        let mut scope: HashMap<String, String> = HashMap::new();
                        hoist_block(&mut b2, &uniq, &mut scope, &mut hoisted);
                        if !scope.is_empty() { rewrite_calls_block(&mut b2, &scope); }
                        new_items.push(Item::Fn(FnDef { name: uniq, type_params: Vec::new(), params, ret: Some(ret.clone()), ret_ty: ret, body: b2, line, is_pub: false, bounds: Vec::new() }));
                    }
                }
                // 保留 trait 实现关系（供 mono 的 where 约束校验）
                new_items.push(Item::TraitImpl { type_params: Vec::new(), trait_name, ty, methods: Vec::new(), line });
            }
            // 泛型 impl（`impl[T] 容器[T]`）：方法保留为泛型函数
            Item::Impl { type_params, ty: _, methods, line: _ } if !type_params.is_empty() => {
                for mut m in methods {
                    m.type_params = type_params.clone();
                    let mut scope: HashMap<String, String> = HashMap::new();
                    hoist_block(&mut m.body, &m.name, &mut scope, &mut hoisted);
                    if !scope.is_empty() { rewrite_calls_block(&mut m.body, &scope); }
                    lift_closures_in_fn(&mut m, &mut closure_counter, &mut closures);
                    let mut cv: Vec<String> = m.params.iter().map(|p| p.name.clone()).collect();
                    convert_closure_calls(&mut m.body, &mut cv);
                    new_items.push(Item::Fn(m));
                }
            }
            // impl 方法展平为顶层函数 `类型__方法`
            Item::Impl { type_params: _, ty, methods, line: _ } => {
                // 先收集本 impl 的原始方法名（供 `self.方法()` 改写用；下面会改名）
                let mnames: std::collections::HashSet<String> = methods.iter().map(|x| x.name.clone()).collect();
                for mut m in methods {
                    impl_scope
                        .insert(format!("{}.{}", ty, m.name), format!("{}__{}", ty, m.name));
                    m.name = format!("{}__{}", ty, m.name);
                    // 首个参数若是 `self`，其类型即本结构体
                    if let Some(first) = m.params.first_mut() {
                        if first.name == "self" && first.ty.is_none() {
                            first.ty = Some(Ty::Struct(ty.clone()));
                        }
                    }
                    let mut scope: HashMap<String, String> = HashMap::new();
                    hoist_block(&mut m.body, &m.name, &mut scope, &mut hoisted);
                    if !scope.is_empty() {
                        rewrite_calls_block(&mut m.body, &scope);
                    }
                    // impl 内自调用：`self.方法(...)` → `类型__方法(self, ...)`
                    rewrite_self_calls_in_block(&mut m.body, &ty, &mnames);
                    lift_closures_in_fn(&mut m, &mut closure_counter, &mut closures);
                    let mut cv: Vec<String> = m.params.iter().map(|p| p.name.clone()).collect();
                    convert_closure_calls(&mut m.body, &mut cv);
                    new_items.push(Item::Fn(m));
                }
            }
            other => new_items.push(other),
        }
    }

    // 提升出来的函数可能还含嵌套，逐层处理
    let mut queue = hoisted;
    while let Some(mut f) = queue.pop() {
        let mut scope: HashMap<String, String> = HashMap::new();
        hoist_block(&mut f.body, &f.name, &mut scope, &mut queue);
        if !scope.is_empty() {
            rewrite_calls_block(&mut f.body, &scope);
        }
        new_items.push(Item::Fn(f));
    }

    // 加入提升出来的闭包函数
    for c in closures {
        new_items.push(Item::Fn(c));
    }

    // 全局把 `类型.方法(...)` 调用改写为 `类型__方法`
    if !impl_scope.is_empty() {
        for item in &mut new_items {
            if let Item::Fn(f) = item {
                rewrite_calls_block(&mut f.body, &impl_scope);
            }
        }
    }
    prog.items = new_items;
}

/// 找出块中直接声明的 `fn`，登记唯一名，递归处理其体，并从块中移除。
/// 把方法体里的 `self.方法(...)` 改写为 `类型__方法(self, ...)`（仅当方法属于本类型）。
fn rewrite_self_calls_in_block(b: &mut Block, ty: &str, mnames: &std::collections::HashSet<String>) {
    for s in b.iter_mut() {
        rewrite_self_calls_in_stmt(s, ty, mnames);
    }
}

fn rewrite_self_calls_in_stmt(s: &mut Stmt, ty: &str, mnames: &std::collections::HashSet<String>) {
    match s {
        Stmt::Let { value, .. } | Stmt::Const { value, .. } => rewrite_self_calls_in_expr(value, ty, mnames),
        Stmt::Assign { value, index, .. } => { rewrite_self_calls_in_expr(value, ty, mnames); if let Some(i) = index { rewrite_self_calls_in_expr(i, ty, mnames); } }
        Stmt::FieldAssign { value, .. } => rewrite_self_calls_in_expr(value, ty, mnames),
        Stmt::Expr(e) | Stmt::Throw(e, _) => rewrite_self_calls_in_expr(e, ty, mnames),
        Stmt::Return(Some(e), _) => rewrite_self_calls_in_expr(e, ty, mnames),
        Stmt::If { cond, then, els, .. } => { rewrite_self_calls_in_expr(cond, ty, mnames); rewrite_self_calls_in_block(then, ty, mnames); if let Some(e) = els { rewrite_self_calls_in_block(e, ty, mnames); } }
        Stmt::While { cond, body, .. } => { rewrite_self_calls_in_expr(cond, ty, mnames); rewrite_self_calls_in_block(body, ty, mnames); }
        Stmt::DoWhile { body, cond, .. } => { rewrite_self_calls_in_block(body, ty, mnames); rewrite_self_calls_in_expr(cond, ty, mnames); }
        Stmt::ForRange { from, to, body, els, .. } => { rewrite_self_calls_in_expr(from, ty, mnames); rewrite_self_calls_in_expr(to, ty, mnames); rewrite_self_calls_in_block(body, ty, mnames); if let Some(e) = els { rewrite_self_calls_in_block(e, ty, mnames); } }
        Stmt::ForEach { iter, body, els, .. } => { rewrite_self_calls_in_expr(iter, ty, mnames); rewrite_self_calls_in_block(body, ty, mnames); if let Some(e) = els { rewrite_self_calls_in_block(e, ty, mnames); } }
        Stmt::Block(inner) => rewrite_self_calls_in_block(inner, ty, mnames),
        Stmt::Try { body, catches, fin, .. } => {
            rewrite_self_calls_in_block(body, ty, mnames);
            for ca in catches { if let Some(g) = &mut ca.guard { rewrite_self_calls_in_expr(g, ty, mnames); } rewrite_self_calls_in_block(&mut ca.body, ty, mnames); }
            if let Some(f) = fin { rewrite_self_calls_in_block(f, ty, mnames); }
        }
        Stmt::Go { args, .. } => for a in args { rewrite_self_calls_in_expr(a, ty, mnames); },
        _ => {}
    }
}

fn rewrite_self_calls_in_expr(e: &mut Expr, ty: &str, mnames: &std::collections::HashSet<String>) {
    // 递归子表达式
    match &mut e.kind {
        ExprKind::Unary(_, a) => rewrite_self_calls_in_expr(a, ty, mnames),
        ExprKind::Binary(_, a, b) => { rewrite_self_calls_in_expr(a, ty, mnames); rewrite_self_calls_in_expr(b, ty, mnames); }
        ExprKind::Call(_, args) => for a in args { rewrite_self_calls_in_expr(a, ty, mnames); },
        ExprKind::CallValue { callee, args } => { rewrite_self_calls_in_expr(callee, ty, mnames); for a in args { rewrite_self_calls_in_expr(a, ty, mnames); } }
        ExprKind::Index(a, b) => { rewrite_self_calls_in_expr(a, ty, mnames); rewrite_self_calls_in_expr(b, ty, mnames); }
        ExprKind::Slice(a, b, c) => { rewrite_self_calls_in_expr(a, ty, mnames); rewrite_self_calls_in_expr(b, ty, mnames); rewrite_self_calls_in_expr(c, ty, mnames); }
        ExprKind::ArrayLit(xs) | ExprKind::TupleLit(xs) => for a in xs { rewrite_self_calls_in_expr(a, ty, mnames); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { rewrite_self_calls_in_expr(i, ty, mnames); } },
        ExprKind::If { cond, then, els } => { rewrite_self_calls_in_expr(cond, ty, mnames); rewrite_self_calls_in_block(then, ty, mnames); if let Some(x) = els { rewrite_self_calls_in_block(x, ty, mnames); } }
        ExprKind::Field(base, _) => rewrite_self_calls_in_expr(base, ty, mnames),
        ExprKind::StructLit(_, fields) => for (_, v) in fields { rewrite_self_calls_in_expr(v, ty, mnames); },
        ExprKind::EnumLit(_, _, payload) => for a in payload { rewrite_self_calls_in_expr(a, ty, mnames); },
        ExprKind::DynBox { value, .. } => rewrite_self_calls_in_expr(value, ty, mnames),
        ExprKind::Ok(x) | ExprKind::Err(x) | ExprKind::Some(x) | ExprKind::Try(x) | ExprKind::Borrow { inner: x, .. } => rewrite_self_calls_in_expr(x, ty, mnames),
        ExprKind::Closure { body, .. } => rewrite_self_calls_in_expr(body, ty, mnames),
        ExprKind::ClosureNew { captures, .. } => for c in captures { rewrite_self_calls_in_expr(c, ty, mnames); },
        ExprKind::TryBlock { body, catches, fin } => {
            rewrite_self_calls_in_block(body, ty, mnames);
            for ca in catches { if let Some(g) = &mut ca.guard { rewrite_self_calls_in_expr(g, ty, mnames); } rewrite_self_calls_in_block(&mut ca.body, ty, mnames); }
            if let Some(f) = fin { rewrite_self_calls_in_block(f, ty, mnames); }
        }
        ExprKind::Match { subject, arms } => {
            rewrite_self_calls_in_expr(subject, ty, mnames);
            for arm in arms { if let Some(p) = &mut arm.pat { rewrite_self_calls_in_expr(p, ty, mnames); } if let Some(g) = &mut arm.guard { rewrite_self_calls_in_expr(g, ty, mnames); } rewrite_self_calls_in_block(&mut arm.body, ty, mnames); }
        }
        _ => {}
    }
    // 改写 `self.方法(...)`
    if let ExprKind::Call(name, args) = &mut e.kind {
        if let Some(rest) = name.strip_prefix("self.") {
            if mnames.contains(rest) {
                let self_expr = Expr::new(ExprKind::Ident("self".to_string()), e.line);
                let mut new_args = vec![self_expr];
                new_args.extend(args.drain(..));
                *args = new_args;
                *name = format!("{}__{}", ty, rest);
            }
        }
    }
}

fn hoist_block(
    b: &mut Block,
    outer: &str,
    scope: &mut HashMap<String, String>,
    out: &mut Vec<FnDef>,
) {
    let mut kept: Block = Vec::new();
    for s in b.drain(..) {
        hoist_stmt(s, outer, scope, out, &mut kept);
    }
    *b = kept;
}

/// 处理单条语句：若是嵌套 fn 则提升，否则原样保留（但递归进入其子块）。
fn hoist_stmt(
    s: Stmt,
    outer: &str,
    scope: &mut HashMap<String, String>,
    out: &mut Vec<FnDef>,
    kept: &mut Block,
) {
    match s {
        Stmt::LocalFn(mut f) => {
            let orig = f.name.clone();
            let uniq = format!("{}__{}", outer, orig);
            scope.insert(orig.clone(), uniq.clone());
            f.name = uniq.clone();
            let mut inner: HashMap<String, String> = HashMap::new();
            hoist_block(&mut f.body, &uniq, &mut inner, out);
            // 该函数体内：内层嵌套名 + 自身的递归引用 都要改写
            inner.insert(orig, uniq.clone());
            rewrite_calls_block(&mut f.body, &inner);
            out.push(f);
        }
        Stmt::If { cond, mut then, mut els, line } => {
            hoist_block(&mut then, outer, scope, out);
            if let Some(e) = els.as_mut() {
                hoist_block(e, outer, scope, out);
            }
            kept.push(Stmt::If { cond, then, els, line });
        }
        Stmt::While { cond, mut body, line } => {
            hoist_block(&mut body, outer, scope, out);
            kept.push(Stmt::While { cond, body, line });
        }
        Stmt::ForRange { var, from, to, mut body, els, line } => {
            hoist_block(&mut body, outer, scope, out);
            let mut els = els.clone();
            if let Some(e) = els.as_mut() { hoist_block(e, outer, scope, out); }
            kept.push(Stmt::ForRange { var, from, to, body, els, line });
        }
        Stmt::ForEach { var, iter, mut body, els, line } => {
            hoist_block(&mut body, outer, scope, out);
            let mut els = els.clone();
            if let Some(e) = els.as_mut() { hoist_block(e, outer, scope, out); }
            kept.push(Stmt::ForEach { var, iter, body, els, line });
        }
        Stmt::Block(mut inner) => {
            hoist_block(&mut inner, outer, scope, out);
            kept.push(Stmt::Block(inner));
        }
        Stmt::FieldAssign { obj, field, op, value, line } => {
            kept.push(Stmt::FieldAssign { obj, field, op, value, line });
        }
        other => kept.push(other),
    }
}

// ============================================================
// 引用改写：把对本层嵌套函数名的调用换成唯一名
// ============================================================

fn rewrite_calls_block(b: &mut Block, scope: &HashMap<String, String>) {
    for s in b.iter_mut() {
        match s {
            Stmt::Let { value, .. } => rewrite_calls_expr(value, scope),
            Stmt::Const { value, .. } => rewrite_calls_expr(value, scope),
            Stmt::Throw(e, _) => rewrite_calls_expr(e, scope),
            Stmt::Asm { .. } => {}
            Stmt::Go { args, .. } => for a in args.iter_mut() { rewrite_calls_expr(a, scope); },
            Stmt::Labeled { inner, .. } => { let mut blk: Block = vec![(**inner).clone()]; rewrite_calls_block(&mut blk, scope); *inner = Box::new(blk.into_iter().next().unwrap()); }
            Stmt::Try { body, catches, fin, .. } => {
                rewrite_calls_block(body, scope);
                for ca in catches {
                    if let Some(g) = &mut ca.guard { rewrite_calls_expr(g, scope); }
                    rewrite_calls_block(&mut ca.body, scope);
                }
                if let Some(f) = fin { rewrite_calls_block(f, scope); }
            }
            Stmt::Assign { value, index, .. } => {
                rewrite_calls_expr(value, scope);
                if let Some(ix) = index {
                    rewrite_calls_expr(ix, scope);
                }
            }
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => rewrite_calls_expr(e, scope),
            Stmt::If { cond, then, els, .. } => {
                rewrite_calls_expr(cond, scope);
                rewrite_calls_block(then, scope);
                if let Some(e) = els {
                    rewrite_calls_block(e, scope);
                }
            }
            Stmt::While { cond, body, .. } => {
                rewrite_calls_expr(cond, scope);
                rewrite_calls_block(body, scope);
            }
            Stmt::DoWhile { body, cond, .. } => {
                rewrite_calls_block(body, scope);
                rewrite_calls_expr(cond, scope);
            }
            Stmt::ForRange { from, to, body, .. } => {
                rewrite_calls_expr(from, scope);
                rewrite_calls_expr(to, scope);
                rewrite_calls_block(body, scope);
            }
            Stmt::ForEach { iter, body, .. } => {
                rewrite_calls_expr(iter, scope);
                rewrite_calls_block(body, scope);
            }
            Stmt::Block(inner) => rewrite_calls_block(inner, scope),
            Stmt::LocalFn(f) => rewrite_calls_block(&mut f.body, scope),
            Stmt::FieldAssign { value, .. } => rewrite_calls_expr(value, scope),
            Stmt::Return(None, _) | Stmt::Break(..) | Stmt::Continue(..) => {}
        }
    }
}

fn rewrite_calls_expr(e: &mut Expr, scope: &HashMap<String, String>) {
    match &mut e.kind {
        ExprKind::CallNamed(_, named) => { for (_, v) in named.iter_mut() { rewrite_calls_expr(v, scope); } }
        ExprKind::TupleLit(items) => for v in items.iter_mut() { rewrite_calls_expr(v, scope); },
        ExprKind::Slice(b, lo, hi) => { rewrite_calls_expr(b, scope); rewrite_calls_expr(lo, scope); rewrite_calls_expr(hi, scope); },
        ExprKind::ListComp { expr, iter, cond, .. } => { rewrite_calls_expr(expr, scope); rewrite_calls_expr(iter, scope); if let Some(c) = cond { rewrite_calls_expr(c, scope); } },
        ExprKind::EnumLit(_, _, args) => for a in args.iter_mut() { rewrite_calls_expr(a, scope); },
        ExprKind::DynBox { value, .. } => rewrite_calls_expr(value, scope),
        ExprKind::MethodOn { recv, args, .. } => { rewrite_calls_expr(recv, scope); for a in args.iter_mut() { rewrite_calls_expr(a, scope); } }
        ExprKind::Call(name, args) => {
            if let Some(uniq) = scope.get(name) {
                *name = uniq.clone();
            }
            for a in args.iter_mut() {
                rewrite_calls_expr(a, scope);
            }
        }
        ExprKind::Ident(_) | ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_)
        | ExprKind::Str(_) => {}
        ExprKind::Field(base, _) => rewrite_calls_expr(base, scope),
        ExprKind::StructLit(_, fields) => {
            for (_, v) in fields.iter_mut() {
                rewrite_calls_expr(v, scope);
            }
        }
        ExprKind::Unary(_, a) => rewrite_calls_expr(a, scope),
        ExprKind::Binary(_, a, b) => {
            rewrite_calls_expr(a, scope);
            rewrite_calls_expr(b, scope);
        }
        ExprKind::Index(a, b) => {
            rewrite_calls_expr(a, scope);
            rewrite_calls_expr(b, scope);
        }
        ExprKind::ArrayLit(items) => {
            for it in items.iter_mut() {
                rewrite_calls_expr(it, scope);
            }
        }
        ExprKind::Interp(parts) => {
            for p in parts.iter_mut() {
                if let StrPart::Expr(inner) = p {
                    rewrite_calls_expr(inner, scope);
                }
            }
        }
        ExprKind::If { cond, then, els } => {
            rewrite_calls_expr(cond, scope);
            rewrite_calls_block(then, scope);
            if let Some(e) = els {
                rewrite_calls_block(e, scope);
            }
        }
        ExprKind::Match { subject, arms } => {
            rewrite_calls_expr(subject, scope);
            for arm in arms.iter_mut() {
                if let Some(p) = arm.pat.as_mut() {
                    rewrite_calls_expr(p, scope);
                }
                if let Some(g) = arm.guard.as_mut() {
                    rewrite_calls_expr(g, scope);
                }
                rewrite_calls_block(&mut arm.body, scope);
            }
        }
        ExprKind::Closure { body, .. } => rewrite_calls_expr(body, scope),
        ExprKind::CallValue { callee, args } => {
            rewrite_calls_expr(callee, scope);
            for a in args.iter_mut() {
                rewrite_calls_expr(a, scope);
            }
        }
        ExprKind::ClosureNew { captures, .. } => {
            for c in captures.iter_mut() {
                rewrite_calls_expr(c, scope);
            }
        }
        ExprKind::Borrow { inner, .. } => rewrite_calls_expr(inner, scope),
        ExprKind::Ok(inner) | ExprKind::Err(inner) | ExprKind::Some(inner) | ExprKind::Try(inner) => {
            rewrite_calls_expr(inner, scope);
        }
        ExprKind::None => {}
        ExprKind::TryBlock { body, catches, fin } => {
            rewrite_calls_block(body, scope);
            for ca in catches {
                if let Some(g) = &mut ca.guard { rewrite_calls_expr(g, scope); }
                rewrite_calls_block(&mut ca.body, scope);
            }
            if let Some(f) = fin { rewrite_calls_block(f, scope); }
        }
    }
}


// ============================================================
// 闭包提升
// ============================================================

/// 把函数体内所有 `Closure` 提升为顶层函数，并替换为 `ClosureNew`。
fn lift_closures_in_fn(
    f: &mut FnDef,
    counter: &mut usize,
    out: &mut Vec<FnDef>,
) {
    lift_closures_in_block(&mut f.body, counter, out);
}

fn lift_closures_in_block(b: &mut Block, counter: &mut usize, out: &mut Vec<FnDef>) {
    for s in b.iter_mut() {
        lift_closures_in_stmt(s, counter, out);
    }
}

fn lift_closures_in_stmt(s: &mut Stmt, counter: &mut usize, out: &mut Vec<FnDef>) {
    match s {
        Stmt::Let { value, .. } => lift_closures_in_expr(value, counter, out),
        Stmt::Const { value, .. } => lift_closures_in_expr(value, counter, out),
        Stmt::Assign { value, .. } => lift_closures_in_expr(value, counter, out),
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => lift_closures_in_expr(e, counter, out),
        Stmt::If { cond, then, els, .. } => {
            lift_closures_in_expr(cond, counter, out);
            lift_closures_in_block(then, counter, out);
            if let Some(e) = els { lift_closures_in_block(e, counter, out); }
        }
        Stmt::While { cond, body, .. } => {
            lift_closures_in_expr(cond, counter, out);
            lift_closures_in_block(body, counter, out);
        }
        Stmt::ForRange { from, to, body, .. } => {
            lift_closures_in_expr(from, counter, out);
            lift_closures_in_expr(to, counter, out);
            lift_closures_in_block(body, counter, out);
        }
        Stmt::ForEach { iter, body, .. } => {
            lift_closures_in_expr(iter, counter, out);
            lift_closures_in_block(body, counter, out);
        }
        Stmt::Block(inner) => lift_closures_in_block(inner, counter, out),
        Stmt::FieldAssign { value, .. } => lift_closures_in_expr(value, counter, out),
        Stmt::LocalFn(f) => lift_closures_in_fn(f, counter, out),
        _ => {}
    }
}

fn lift_closures_in_expr(e: &mut Expr, counter: &mut usize, out: &mut Vec<FnDef>) {
    // 先递归子表达式
    match &mut e.kind {
        ExprKind::Unary(_, a) => lift_closures_in_expr(a, counter, out),
        ExprKind::Binary(_, a, b) => {
            lift_closures_in_expr(a, counter, out);
            lift_closures_in_expr(b, counter, out);
        }
        ExprKind::Call(_, args) => for a in args { lift_closures_in_expr(a, counter, out); },
        ExprKind::CallValue { callee, args } => {
            lift_closures_in_expr(callee, counter, out);
            for a in args { lift_closures_in_expr(a, counter, out); }
        }
        ExprKind::Index(a, b) => {
            lift_closures_in_expr(a, counter, out);
            lift_closures_in_expr(b, counter, out);
        }
        ExprKind::ArrayLit(xs) => for x in xs { lift_closures_in_expr(x, counter, out); },
        ExprKind::Interp(parts) => for p in parts {
            if let StrPart::Expr(i) = p { lift_closures_in_expr(i, counter, out); }
        },
        ExprKind::If { cond, then, els } => {
            lift_closures_in_expr(cond, counter, out);
            lift_closures_in_block(then, counter, out);
            if let Some(e) = els { lift_closures_in_block(e, counter, out); }
        }
        ExprKind::Match { subject, arms } => {
            lift_closures_in_expr(subject, counter, out);
            for arm in arms {
                if let Some(p) = &mut arm.pat { lift_closures_in_expr(p, counter, out); }
                if let Some(g) = &mut arm.guard { lift_closures_in_expr(g, counter, out); }
                lift_closures_in_block(&mut arm.body, counter, out);
            }
        }
        ExprKind::Field(base, _) => lift_closures_in_expr(base, counter, out),
        ExprKind::StructLit(_, fields) => for (_, v) in fields { lift_closures_in_expr(v, counter, out); },
        _ => {}
    }
    // 若本身是闭包，提升它
    if let ExprKind::Closure { params, param_tys, ret_ty, body, line } = &e.kind {
        let params = params.clone();
        let param_tys = param_tys.clone();
        let ret_ty = ret_ty.clone();
        let body = (**body).clone();
        let line = *line;
        let name = format!("__closure_{}", *counter);
        *counter += 1;
        // 自由变量：body 里引用且不是 params 的标识符
        let mut free = Vec::new();
        collect_free_vars_expr(&body, &params, &mut free);
        free.sort();
        free.dedup();
        // 构造顶层函数：__closure_N(cap0, cap1, ..., p0, p1, ...)
        // 捕获变量作为**前置参数**（捕获按值快照）；调用时先传捕获值。
        let new_body = body.clone();
        let mut fparams: Vec<Param> = Vec::new();
        for fv in &free {
            fparams.push(Param { name: fv.clone(), ty: None, default: None, line });
        }
        for (i, p) in params.iter().enumerate() {
            let ty = param_tys.get(i).cloned().flatten();
            fparams.push(Param { name: p.clone(), ty, default: None, line });
        }
        let fn_def = FnDef {
            name: name.clone(),
            type_params: Vec::new(),
            params: fparams,
            ret: ret_ty.clone(),
            ret_ty: ret_ty.unwrap_or(Ty::Unknown),
            body: vec![Stmt::Expr(new_body)],
            line,
            is_pub: false,
            bounds: Vec::new(),
        };
        out.push(fn_def);
        // 替换为 ClosureNew
        let captures: Vec<Expr> = free
            .iter()
            .map(|fv| Expr::new(ExprKind::Ident(fv.clone()), line))
            .collect();
        e.kind = ExprKind::ClosureNew { fn_name: name, captures };
    }
}

/// 收集表达式里的自由变量（不在 bound 中的标识符）
fn collect_free_vars_expr(e: &Expr, bound: &[String], out: &mut Vec<String>) {
    match &e.kind {
        ExprKind::Ident(n) => {
            if !bound.contains(n) && !n.starts_with("__") {
                out.push(n.clone());
            }
        }
        ExprKind::Unary(_, a) => collect_free_vars_expr(a, bound, out),
        ExprKind::Binary(_, a, b) => {
            collect_free_vars_expr(a, bound, out);
            collect_free_vars_expr(b, bound, out);
        }
        ExprKind::Call(_, args) => for a in args { collect_free_vars_expr(a, bound, out); },
        ExprKind::CallValue { callee, args } => {
            collect_free_vars_expr(callee, bound, out);
            for a in args { collect_free_vars_expr(a, bound, out); }
        }
        ExprKind::Index(a, b) => {
            collect_free_vars_expr(a, bound, out);
            collect_free_vars_expr(b, bound, out);
        }
        ExprKind::ArrayLit(xs) => for x in xs { collect_free_vars_expr(x, bound, out); },
        ExprKind::Interp(parts) => for p in parts {
            if let StrPart::Expr(i) = p { collect_free_vars_expr(i, bound, out); }
        },
        ExprKind::If { cond, then, els } => {
            collect_free_vars_expr(cond, bound, out);
            collect_free_vars_block(then, bound, out);
            if let Some(e) = els { collect_free_vars_block(e, bound, out); }
        }
        ExprKind::Match { subject, arms } => {
            collect_free_vars_expr(subject, bound, out);
            for arm in arms {
                if let Some(p) = &arm.pat { collect_free_vars_expr(p, bound, out); }
                if let Some(g) = &arm.guard { collect_free_vars_expr(g, bound, out); }
                collect_free_vars_block(&arm.body, bound, out);
            }
        }
        ExprKind::Field(base, _) => collect_free_vars_expr(base, bound, out),
        ExprKind::StructLit(_, fields) => for (_, v) in fields { collect_free_vars_expr(v, bound, out); },
        _ => {}
    }
}

fn collect_free_vars_block(b: &Block, bound: &[String], out: &mut Vec<String>) {
    let mut bound = bound.to_vec();
    for s in b {
        match s {
            Stmt::Let { name, value, .. } => {
                collect_free_vars_expr(value, &bound, out);
                bound.push(name.clone());
            }
            Stmt::Const { value, .. } => collect_free_vars_expr(value, &bound, out),
            Stmt::Assign { value, .. } => collect_free_vars_expr(value, &bound, out),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => collect_free_vars_expr(e, &bound, out),
            Stmt::If { cond, then, els, .. } => {
                collect_free_vars_expr(cond, &bound, out);
                collect_free_vars_block(then, &bound, out);
                if let Some(e) = els { collect_free_vars_block(e, &bound, out); }
            }
            Stmt::While { cond, body, .. } => {
                collect_free_vars_expr(cond, &bound, out);
                collect_free_vars_block(body, &bound, out);
            }
            Stmt::ForRange { var, from, to, body, .. } => {
                collect_free_vars_expr(from, &bound, out);
                collect_free_vars_expr(to, &bound, out);
                let mut b2 = bound.clone();
                b2.push(var.clone());
                collect_free_vars_block(body, &b2, out);
            }
            Stmt::ForEach { var, iter, body, .. } => {
                collect_free_vars_expr(iter, &bound, out);
                let mut b2 = bound.clone();
                b2.push(var.clone());
                collect_free_vars_block(body, &b2, out);
            }
            Stmt::Block(inner) => collect_free_vars_block(inner, &bound, out),
            Stmt::FieldAssign { value, .. } => collect_free_vars_expr(value, &bound, out),
            _ => {}
        }
    }
}

/// 把「对闭包变量的命名调用」`f(args)` 改写为间接调用 `CallValue(Ident(f), args)`。
/// 依据：同一作用域内 `f := <闭包>` 的定义（语法层判定，无需类型信息）。
fn convert_closure_calls(b: &mut Block, closure_vars: &mut Vec<String>) {
    for s in b.iter_mut() {
        match s {
            Stmt::Let { name, value, .. } => {
                convert_closure_calls_expr(value, closure_vars);
                if is_closure_expr(value) {
                    closure_vars.push(name.clone());
                }
            }
            Stmt::Const { value, .. } => convert_closure_calls_expr(value, closure_vars),
            Stmt::Assign { value, .. } => convert_closure_calls_expr(value, closure_vars),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => convert_closure_calls_expr(e, closure_vars),
            Stmt::If { cond, then, els, .. } => {
                convert_closure_calls_expr(cond, closure_vars);
                convert_closure_calls(then, &mut closure_vars.clone());
                if let Some(e) = els { convert_closure_calls(e, &mut closure_vars.clone()); }
            }
            Stmt::While { cond, body, .. } => {
                convert_closure_calls_expr(cond, closure_vars);
                convert_closure_calls(body, &mut closure_vars.clone());
            }
            Stmt::ForRange { from, to, body, .. } => {
                convert_closure_calls_expr(from, closure_vars);
                convert_closure_calls_expr(to, closure_vars);
                convert_closure_calls(body, &mut closure_vars.clone());
            }
            Stmt::ForEach { iter, body, .. } => {
                convert_closure_calls_expr(iter, closure_vars);
                convert_closure_calls(body, &mut closure_vars.clone());
            }
            Stmt::Block(inner) => convert_closure_calls(inner, closure_vars),
            Stmt::FieldAssign { value, .. } => convert_closure_calls_expr(value, closure_vars),
            _ => {}
        }
    }
}

fn is_closure_expr(e: &Expr) -> bool {
    matches!(e.kind, ExprKind::ClosureNew { .. } | ExprKind::Closure { .. })
}

fn convert_closure_calls_expr(e: &mut Expr, closure_vars: &[String]) {
    if let ExprKind::Call(name, args) = &mut e.kind {
        if closure_vars.contains(name) {
            let callee = Expr::new(ExprKind::Ident(name.clone()), e.line);
            let args = std::mem::take(args);
            e.kind = ExprKind::CallValue { callee: Box::new(callee), args };
        }
    }
    match &mut e.kind {
        ExprKind::Call(_, args) => for a in args { convert_closure_calls_expr(a, closure_vars); },
        ExprKind::CallValue { callee, args } => {
            convert_closure_calls_expr(callee, closure_vars);
            for a in args { convert_closure_calls_expr(a, closure_vars); }
        }
        ExprKind::Unary(_, a) => convert_closure_calls_expr(a, closure_vars),
        ExprKind::Binary(_, a, b) => {
            convert_closure_calls_expr(a, closure_vars);
            convert_closure_calls_expr(b, closure_vars);
        }
        ExprKind::Index(a, b) => {
            convert_closure_calls_expr(a, closure_vars);
            convert_closure_calls_expr(b, closure_vars);
        }
        ExprKind::ArrayLit(xs) => for x in xs { convert_closure_calls_expr(x, closure_vars); },
        ExprKind::Interp(parts) => for p in parts {
            if let StrPart::Expr(i) = p { convert_closure_calls_expr(i, closure_vars); }
        },
        ExprKind::ClosureNew { captures, .. } => for c in captures { convert_closure_calls_expr(c, closure_vars); },
        _ => {}
    }
}
