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

thread_local! {
    /// 当前正在 lift 的函数"可见的外层变量"（函数参数 + 函数体 let/const）。
    /// 供 collect_free_vars_expr 判断闭包体内的 `Call(name)` 的 name 是否为自由变量。
    static OUTER_VARS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
#[path = "hoist_tests.rs"]
mod hoist_tests;

/// 判断函数体是否直接返回闭包字面量（用于识别"返回闭包的函数"）。
fn block_returns_closure(b: &Block) -> bool {
    for s in b {
        match s {
            Stmt::Return(Some(e), _) => {
                if is_closure_expr(e) {
                    return true;
                }
            }
            Stmt::If { then, els, .. } => {
                if block_returns_closure(then) { return true; }
                if let Some(e) = els { if block_returns_closure(e) { return true; } }
            }
            Stmt::Block(inner) => { if block_returns_closure(inner) { return true; } }
            _ => {}
        }
    }
    false
}

/// 把"值位置出现的全局函数名"（如 `组合(加一, ...)` 里的 `加一`）转成
/// 无捕获闭包构造 `ClosureNew { fn_name, captures: [] }`，使函数名可作一等值。
/// `Call` 的 callee 是字符串（不是 Ident），因此不会误转普通调用。
/// 收集函数体内的局部绑定名（参数 + let/const/:=/for 变量），
/// 用于在“函数作一等值”改写时避免把与函数同名的变量误当函数引用。
fn collect_local_names(f: &FnDef) -> std::collections::HashSet<String> {
    let mut s = std::collections::HashSet::new();
    for p in &f.params { s.insert(p.name.clone()); }
    fn blk(b: &Block, s: &mut std::collections::HashSet<String>) {
        for st in b {
            match st {
                Stmt::Let { name, .. } | Stmt::Const { name, .. } => { s.insert(name.clone()); }
                Stmt::ForRange { var, body, .. } | Stmt::ForEach { var, body, .. } => { s.insert(var.clone()); blk(body, s); }
                Stmt::If { then, els, .. } => { blk(then, s); if let Some(e) = els { blk(e, s); } }
                Stmt::While { body, .. } | Stmt::Block(body) => blk(body, s),
                Stmt::Try { body, catches, fin, .. } => { blk(body, s); for c in catches { blk(&c.body, s); } if let Some(z) = fin { blk(z, s); } }
                Stmt::Labeled { inner, .. } => { let one: Block = vec![(**inner).clone()]; blk(&one, s); }
                Stmt::LocalFn(lf) => { s.insert(lf.name.clone()); blk(&lf.body, s); }
                _ => {}
            }
        }
    }
    blk(&f.body, &mut s);
    s
}
fn convert_fn_refs(prog: &mut Program, fn_names: &[String]) {
    for item in &mut prog.items {
        if let Item::Fn(f) = item {
            let locals = collect_local_names(f); convert_fn_refs_block(&mut f.body, fn_names, &locals);
        }
    }
}

fn convert_fn_refs_block(b: &mut Block, fn_names: &[String], locals: &std::collections::HashSet<String>) {
    for s in b.iter_mut() {
        match s {
            Stmt::Let { value, .. } | Stmt::Const { value, .. } | Stmt::Assign { value, .. } | Stmt::FieldAssign { value, .. } => convert_fn_refs_expr(value, fn_names, locals),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => convert_fn_refs_expr(e, fn_names, locals),
            Stmt::If { cond, then, els, .. } => { convert_fn_refs_expr(cond, fn_names, locals); convert_fn_refs_block(then, fn_names, locals); if let Some(e) = els { convert_fn_refs_block(e, fn_names, locals); } }
            Stmt::While { cond, body, .. } => { convert_fn_refs_expr(cond, fn_names, locals); convert_fn_refs_block(body, fn_names, locals); }
            Stmt::ForRange { from, to, body, .. } => { convert_fn_refs_expr(from, fn_names, locals); convert_fn_refs_expr(to, fn_names, locals); convert_fn_refs_block(body, fn_names, locals); }
            Stmt::ForEach { iter, body, .. } => { convert_fn_refs_expr(iter, fn_names, locals); convert_fn_refs_block(body, fn_names, locals); }
            Stmt::Block(inner) => convert_fn_refs_block(inner, fn_names, locals),
            // go f(args)：实参里可能有函数名作一等值。
            Stmt::Go { args, .. } => for a in args.iter_mut() { convert_fn_refs_expr(a, fn_names, locals); },
            Stmt::Throw(e, _) => convert_fn_refs_expr(e, fn_names, locals),
            Stmt::Labeled { inner, .. } => {
                let mut blk: Block = vec![(**inner).clone()];
                convert_fn_refs_block(&mut blk, fn_names, locals);
                if let Some(x) = blk.into_iter().next() { **inner = x; }
            }
            Stmt::Try { body, catches, fin, .. } => {
                convert_fn_refs_block(body, fn_names, locals);
                for c in catches.iter_mut() { convert_fn_refs_block(&mut c.body, fn_names, locals); }
                if let Some(f) = fin { convert_fn_refs_block(f, fn_names, locals); }
            }
            Stmt::LocalFn(f) => convert_fn_refs_block(&mut f.body, fn_names, locals),
            _ => {}
        }
    }
}

fn convert_fn_refs_expr(e: &mut Expr, fn_names: &[String], locals: &std::collections::HashSet<String>) {
    // 先递归（不含 CallValue 的 callee——那是闭包变量调用，不应转成函数引用）
    match &mut e.kind {
        ExprKind::Call(name, args) => {
            // fn_addr(f)：参数保持原样（需要裸函数名/地址，不转闭包）
            if name != "fn_addr" {
                for a in args { convert_fn_refs_expr(a, fn_names, locals); }
            }
        }
        ExprKind::CallValue { args, .. } => for a in args { convert_fn_refs_expr(a, fn_names, locals); },
        ExprKind::Unary(_, a) => convert_fn_refs_expr(a, fn_names, locals),
        ExprKind::Binary(_, a, b) => { convert_fn_refs_expr(a, fn_names, locals); convert_fn_refs_expr(b, fn_names, locals); }
        ExprKind::Index(a, b) => { convert_fn_refs_expr(a, fn_names, locals); convert_fn_refs_expr(b, fn_names, locals); }
        ExprKind::ArrayLit(xs) => for x in xs { convert_fn_refs_expr(x, fn_names, locals); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { convert_fn_refs_expr(i, fn_names, locals); } },
        ExprKind::TupleLit(xs) => for x in xs { convert_fn_refs_expr(x, fn_names, locals); },
        ExprKind::Slice(a, b, c) => { convert_fn_refs_expr(a, fn_names, locals); convert_fn_refs_expr(b, fn_names, locals); convert_fn_refs_expr(c, fn_names, locals); }
        ExprKind::Field(b, _) => convert_fn_refs_expr(b, fn_names, locals),
        ExprKind::StructLit(_, fs) => for (_, v) in fs { convert_fn_refs_expr(v, fn_names, locals); },
        ExprKind::EnumLit(_, _, args) => for a in args { convert_fn_refs_expr(a, fn_names, locals); },
        ExprKind::DynBox { value, .. } => convert_fn_refs_expr(value, fn_names, locals),
        ExprKind::If { cond, then, els } => {
            convert_fn_refs_expr(cond, fn_names, locals);
            convert_fn_refs_block(then, fn_names, locals);
            if let Some(e) = els { convert_fn_refs_block(e, fn_names, locals); }
        }
        ExprKind::Match { subject, arms } => {
            convert_fn_refs_expr(subject, fn_names, locals);
            for arm in arms {
                if let Some(p) = &mut arm.pat { convert_fn_refs_expr(p, fn_names, locals); }
                if let Some(g) = &mut arm.guard { convert_fn_refs_expr(g, fn_names, locals); }
                convert_fn_refs_block(&mut arm.body, fn_names, locals);
            }
        }
        ExprKind::Borrow { inner, .. } => convert_fn_refs_expr(inner, fn_names, locals),
        ExprKind::Ok(a) | ExprKind::Err(a) | ExprKind::Try(a) | ExprKind::Some(a) => convert_fn_refs_expr(a, fn_names, locals),
        ExprKind::MethodOn { recv, args, .. } => { convert_fn_refs_expr(recv, fn_names, locals); for a in args { convert_fn_refs_expr(a, fn_names, locals); } }
        ExprKind::ClosureNew { captures, .. } => for c in captures { convert_fn_refs_expr(c, fn_names, locals); },
        _ => {}
    }
    // fn_addr(f)：参数是函数名时**不转闭包**（需要的是裸函数地址，不是 [fn_ptr, env] 块）
    if let ExprKind::Call(name, args) = &mut e.kind {
        if name == "fn_addr" {
            for a in args.iter_mut() {
                // 递归处理子表达式即可，保持 Ident 原样
                convert_fn_refs_expr(a, fn_names, locals);
            }
            return;
        }
    }
    // 值位置的函数名 → 无捕获闭包
    if let ExprKind::Ident(n) = &e.kind {
        if fn_names.iter().any(|f| f == n) && !locals.contains(n) {
            e.kind = ExprKind::ClosureNew { fn_name: n.clone(), captures: vec![] };
        }
    }
}

/// 对整份程序做降级：嵌套函数提升 + impl 方法展平。
pub fn hoist(prog: &mut Program) {
    // 返回闭包的函数名集合（供 convert_closure_calls 识别 f := 该函数(...)）
    let ret_closure_fns: Vec<String> = prog
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Fn(f) if block_returns_closure(&f.body) => Some(f.name.clone()),
            _ => None,
        })
        .collect();
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
                convert_closure_calls(&mut f.body, &mut cv, &ret_closure_fns);
                new_items.push(Item::Fn(f));
            }
            // 泛型 impl（`impl[T] ...`）：方法保留为泛型函数，由 mono 单态化；不在此展平
            Item::TraitImpl { type_params, trait_name, ty, methods, assoc_bind, line } if !type_params.is_empty() => {
                for mut m in methods {
                    m.type_params = type_params.clone();
                    let mut scope: HashMap<String, String> = HashMap::new();
                    hoist_block(&mut m.body, &m.name, &mut scope, &mut hoisted);
                    if !scope.is_empty() { rewrite_calls_block(&mut m.body, &scope); }
                    lift_closures_in_fn(&mut m, &mut closure_counter, &mut closures);
                    let mut cv: Vec<String> = m.params.iter().map(|p| p.name.clone()).collect();
                    convert_closure_calls(&mut m.body, &mut cv, &ret_closure_fns);
                    new_items.push(Item::Fn(m));
                }
                new_items.push(Item::TraitImpl { type_params, trait_name, ty, methods: Vec::new(), assoc_bind, line });
            }
            // trait impl 与 impl 同样展平方法
            Item::TraitImpl { type_params: _, trait_name, ty, methods, assoc_bind, line } => {
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
                    convert_closure_calls(&mut m.body, &mut cv, &ret_closure_fns);
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
                        // 默认方法体内的 `self.方法`（含父 trait 的方法）降级为 `类型__方法`
                        rewrite_self_calls_in_block(&mut b2, &ty, &std::collections::HashSet::new());
                        new_items.push(Item::Fn(FnDef { name: uniq, type_params: Vec::new(), params, ret: Some(ret.clone()), ret_ty: ret, body: b2, line, is_pub: false, bounds: Vec::new(), attrs: Vec::new() }));
                    }
                }
                // 保留 trait 实现关系（供 mono 的 where 约束校验）
                new_items.push(Item::TraitImpl { type_params: Vec::new(), trait_name, ty, methods: Vec::new(), assoc_bind, line });
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
                    convert_closure_calls(&mut m.body, &mut cv, &ret_closure_fns);
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
                    convert_closure_calls(&mut m.body, &mut cv, &ret_closure_fns);
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

    // 加入提升出来的闭包函数（对它们的函数体也做闭包调用改写：
    // 闭包体里 `g(f(x))` 的 g/f 是"捕获参数"，需转成间接调用）
    for mut c in closures {
        let mut cv: Vec<String> = c.params.iter().map(|p| p.name.clone()).collect();
        convert_closure_calls(&mut c.body, &mut cv, &ret_closure_fns);
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

    // 值位置出现的全局函数名 → 无捕获闭包（函数作一等值）
    let mut fn_names: Vec<String> = Vec::new();
    for item in &prog.items {
        if let Item::Fn(f) = item { fn_names.push(f.name.clone()); }
    }
    convert_fn_refs(prog, &fn_names);
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
    // 改写 `self.方法(...)`：无条件降级为 `类型__方法`（含父 trait 的方法）
    let _ = mnames;
    if let ExprKind::Call(name, args) = &mut e.kind {
        if let Some(rest) = name.strip_prefix("self.") {
            let self_expr = Expr::new(ExprKind::Ident("self".to_string()), e.line);
            let mut new_args = vec![self_expr];
            new_args.extend(args.drain(..));
            *args = new_args;
            *name = format!("{}__{}", ty, rest);
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
            Stmt::Defer(e, _) => rewrite_calls_expr(e, scope),
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
    // 记录本函数的可见变量（参数 + 体内 let/const），供闭包捕获分析用。
    let saved = OUTER_VARS.with(|v| v.borrow().clone());
    let mut visible: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
    collect_visible_names_block(&f.body, &mut visible);
    OUTER_VARS.with(|v| *v.borrow_mut() = visible);
    lift_closures_in_block(&mut f.body, counter, out);
    OUTER_VARS.with(|v| *v.borrow_mut() = saved);
}

/// 收集块中所有 let/const 名（不递归进嵌套闭包/函数）。
fn collect_visible_names_block(b: &Block, out: &mut Vec<String>) {
    for s in b {
        match s {
            Stmt::Let { name, value, .. } => { collect_visible_names_expr(value, out); out.push(name.clone()); }
            Stmt::Const { name, value, .. } => { collect_visible_names_expr(value, out); out.push(name.clone()); }
            Stmt::Assign { value, .. } | Stmt::FieldAssign { value, .. } => collect_visible_names_expr(value, out),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => collect_visible_names_expr(e, out),
            Stmt::If { cond, then, els, .. } => { collect_visible_names_expr(cond, out); collect_visible_names_block(then, out); if let Some(e) = els { collect_visible_names_block(e, out); } }
            Stmt::While { cond, body, .. } => { collect_visible_names_expr(cond, out); collect_visible_names_block(body, out); }
            Stmt::DoWhile { body, cond, .. } => { collect_visible_names_block(body, out); collect_visible_names_expr(cond, out); }
            Stmt::ForRange { var, from, to, body, els, .. } => { collect_visible_names_expr(from, out); collect_visible_names_expr(to, out); out.push(var.clone()); collect_visible_names_block(body, out); if let Some(e) = els { collect_visible_names_block(e, out); } }
            Stmt::ForEach { var, iter, body, els, .. } => { collect_visible_names_expr(iter, out); out.push(var.clone()); collect_visible_names_block(body, out); if let Some(e) = els { collect_visible_names_block(e, out); } }
            Stmt::Block(inner) => collect_visible_names_block(inner, out),
            Stmt::Throw(e, _) => collect_visible_names_expr(e, out),
            _ => {}
        }
    }
}

fn collect_visible_names_expr(_e: &Expr, _out: &mut Vec<String>) {
    // 表达式内部不引入新的可见变量（闭包参数由闭包自己处理）
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
        // 先递归提升 body 里的嵌套闭包（否则内层 Closure 会残留到生成函数里）
        let mut body = (**body).clone();
        // 提升 body 里的嵌套闭包时，外层闭包的 params 也是"可见变量"（供其捕获）
        let saved_outer = OUTER_VARS.with(|v| v.borrow().clone());
        OUTER_VARS.with(|v| {
            let mut cur = v.borrow_mut();
            for p in &params { if !cur.contains(p) { cur.push(p.clone()); } }
        });
        lift_closures_in_expr(&mut body, counter, out);
        OUTER_VARS.with(|v| *v.borrow_mut() = saved_outer);
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
            attrs: Vec::new(),
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
        ExprKind::Call(callee, args) => {
            // callee 是"外层可见变量"（函数参数/局部）→ 自由变量（闭包捕获）
            let is_outer = OUTER_VARS.with(|v| v.borrow().iter().any(|n| n == callee));
            if is_outer && !out.contains(callee) {
                out.push(callee.clone());
            }
            for a in args { collect_free_vars_expr(a, bound, out); }
        }
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
            // 以下语句里的表达式也可能引用自由变量（闭包捕获需要）。
            Stmt::Go { args, .. } => for a in args { collect_free_vars_expr(a, &bound, out); },
            Stmt::Throw(e, _) => collect_free_vars_expr(e, &bound, out),
            Stmt::Try { body, catches, fin, .. } => {
                collect_free_vars_block(body, &bound, out);
                for c in catches { collect_free_vars_block(&c.body, &bound, out); }
                if let Some(f) = fin { collect_free_vars_block(f, &bound, out); }
            }
            Stmt::LocalFn(_) => { /* 内层函数不递归（不引入外层自由变量） */ }
            _ => {}
        }
    }
}

/// 把「对闭包变量的命名调用」`f(args)` 改写为间接调用 `CallValue(Ident(f), args)`。
/// 依据：同一作用域内 `f := <闭包>` 的定义（语法层判定，无需类型信息）。
fn convert_closure_calls(b: &mut Block, closure_vars: &mut Vec<String>, rcf: &[String]) {
    for s in b.iter_mut() {
        match s {
            Stmt::Let { name, value, .. } => {
                convert_closure_calls_expr(value, closure_vars, rcf);
                // 值可能是闭包：闭包字面量、返回闭包的调用、调用闭包变量得到的值、间接调用结果
                let is_closure_val = is_closure_expr(value)
                    || is_call_to_ret_closure(value, rcf)
                    || is_call_to_closure_var(value, closure_vars)
                    || matches!(value.kind, ExprKind::CallValue { .. });
                if is_closure_val {
                    closure_vars.push(name.clone());
                }
            }
            Stmt::Const { value, .. } => convert_closure_calls_expr(value, closure_vars, rcf),
            Stmt::Assign { value, .. } => convert_closure_calls_expr(value, closure_vars, rcf),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => convert_closure_calls_expr(e, closure_vars, rcf),
            Stmt::If { cond, then, els, .. } => {
                convert_closure_calls_expr(cond, closure_vars, rcf);
                convert_closure_calls(then, &mut closure_vars.clone(), rcf);
                if let Some(e) = els { convert_closure_calls(e, &mut closure_vars.clone(), rcf); }
            }
            Stmt::While { cond, body, .. } => {
                convert_closure_calls_expr(cond, closure_vars, rcf);
                convert_closure_calls(body, &mut closure_vars.clone(), rcf);
            }
            Stmt::ForRange { from, to, body, .. } => {
                convert_closure_calls_expr(from, closure_vars, rcf);
                convert_closure_calls_expr(to, closure_vars, rcf);
                convert_closure_calls(body, &mut closure_vars.clone(), rcf);
            }
            Stmt::ForEach { iter, body, .. } => {
                convert_closure_calls_expr(iter, closure_vars, rcf);
                convert_closure_calls(body, &mut closure_vars.clone(), rcf);
            }
            Stmt::Block(inner) => convert_closure_calls(inner, closure_vars, rcf),
            Stmt::FieldAssign { value, .. } => convert_closure_calls_expr(value, closure_vars, rcf),
            // go f(args)：实参里可能含闭包调用（如 go 工(ch, f(i))）。
            Stmt::Go { args, .. } => for a in args.iter_mut() { convert_closure_calls_expr(a, closure_vars, rcf); },
            Stmt::Throw(e, _) => convert_closure_calls_expr(e, closure_vars, rcf),
            Stmt::Labeled { inner, .. } => {
                let mut blk: Block = vec![(**inner).clone()];
                convert_closure_calls(&mut blk, closure_vars, rcf);
                if let Some(x) = blk.into_iter().next() { **inner = x; }
            }
            Stmt::Try { body, catches, fin, .. } => {
                convert_closure_calls(body, &mut closure_vars.clone(), rcf);
                for c in catches.iter_mut() { convert_closure_calls(&mut c.body, &mut closure_vars.clone(), rcf); }
                if let Some(f) = fin { convert_closure_calls(f, &mut closure_vars.clone(), rcf); }
            }
            Stmt::LocalFn(f) => convert_closure_calls(&mut f.body, &mut closure_vars.clone(), rcf),
            _ => {}
        }
    }
}

fn is_closure_expr(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::ClosureNew { .. } | ExprKind::Closure { .. } => true,
        // 值位置出现的全局函数名（convert_fn_refs 会把它变成 ClosureNew）
        ExprKind::Ident(_) => true,
        // if/match 分支都产闭包 → 整体是闭包
        ExprKind::If { then, els, .. } => {
            let then_ok = matches!(then.last(), Some(Stmt::Expr(x)) if is_closure_expr(x));
            let els_ok = els.as_ref().map_or(false, |b| matches!(b.last(), Some(Stmt::Expr(x)) if is_closure_expr(x)));
            then_ok && els_ok
        }
        // match 的每个分支都产闭包 → 整体是闭包
        ExprKind::Match { arms, .. } => !arms.is_empty()
            && arms.iter().all(|a| matches!(a.body.last(), Some(Stmt::Expr(x)) if is_closure_expr(x))),
        _ => false,
    }
}

/// `e` 是否为"调用某个闭包变量"（如 `m3 := 造乘(3)` 里的 `造乘(3)`）。
fn is_call_to_closure_var(e: &Expr, cv: &[String]) -> bool {
    if let ExprKind::Call(name, _) = &e.kind {
        return cv.iter().any(|c| c == name);
    }
    false
}

/// `e` 是否为"调用某个返回闭包的函数"，如 `造加法(10)`。
fn is_call_to_ret_closure(e: &Expr, rcf: &[String]) -> bool {
    if let ExprKind::Call(name, _) = &e.kind {
        return rcf.iter().any(|f| f == name);
    }
    false
}

fn convert_closure_calls_expr(e: &mut Expr, closure_vars: &[String], rcf: &[String]) {
    if let ExprKind::Call(name, args) = &mut e.kind {
        if closure_vars.contains(name) {
            let callee = Expr::new(ExprKind::Ident(name.clone()), e.line);
            let args = std::mem::take(args);
            e.kind = ExprKind::CallValue { callee: Box::new(callee), args };
        }
    }
    match &mut e.kind {
        ExprKind::Call(_, args) => for a in args { convert_closure_calls_expr(a, closure_vars, rcf); },
        ExprKind::CallValue { callee, args } => {
            convert_closure_calls_expr(callee, closure_vars, rcf);
            for a in args { convert_closure_calls_expr(a, closure_vars, rcf); }
        }
        ExprKind::Unary(_, a) => convert_closure_calls_expr(a, closure_vars, rcf),
        ExprKind::Binary(_, a, b) => {
            convert_closure_calls_expr(a, closure_vars, rcf);
            convert_closure_calls_expr(b, closure_vars, rcf);
        }
        ExprKind::Index(a, b) => {
            convert_closure_calls_expr(a, closure_vars, rcf);
            convert_closure_calls_expr(b, closure_vars, rcf);
        }
        ExprKind::ArrayLit(xs) => for x in xs { convert_closure_calls_expr(x, closure_vars, rcf); },
        ExprKind::Interp(parts) => for p in parts {
            if let StrPart::Expr(i) = p { convert_closure_calls_expr(i, closure_vars, rcf); }
        },
        ExprKind::ClosureNew { captures, .. } => for c in captures { convert_closure_calls_expr(c, closure_vars, rcf); },
        ExprKind::CallNamed(_, named) => for (_, a) in named { convert_closure_calls_expr(a, closure_vars, rcf); },
        ExprKind::Slice(a, lo, hi) => {
            convert_closure_calls_expr(a, closure_vars, rcf);
            convert_closure_calls_expr(lo, closure_vars, rcf);
            convert_closure_calls_expr(hi, closure_vars, rcf);
        }
        ExprKind::TupleLit(items) => for a in items { convert_closure_calls_expr(a, closure_vars, rcf); },
        ExprKind::ListComp { expr, iter, cond, .. } => {
            convert_closure_calls_expr(expr, closure_vars, rcf);
            convert_closure_calls_expr(iter, closure_vars, rcf);
            if let Some(c) = cond { convert_closure_calls_expr(c, closure_vars, rcf); }
        }
        ExprKind::Field(a, _) => convert_closure_calls_expr(a, closure_vars, rcf),
        ExprKind::StructLit(_, fields) => for (_, a) in fields { convert_closure_calls_expr(a, closure_vars, rcf); },
        ExprKind::EnumLit(_, _, args) => for a in args { convert_closure_calls_expr(a, closure_vars, rcf); },
        ExprKind::DynBox { value, .. } => convert_closure_calls_expr(value, closure_vars, rcf),
        ExprKind::If { cond, then, els } => {
            convert_closure_calls_expr(cond, closure_vars, rcf);
            convert_closure_calls(then, &mut closure_vars.to_vec(), rcf);
            if let Some(b) = els { convert_closure_calls(b, &mut closure_vars.to_vec(), rcf); }
        }
        ExprKind::Match { subject, arms } => {
            convert_closure_calls_expr(subject, closure_vars, rcf);
            for arm in arms {
                if let Some(p) = &mut arm.pat { convert_closure_calls_expr(p, closure_vars, rcf); }
                if let Some((lo, hi)) = &mut arm.range {
                    convert_closure_calls_expr(lo, closure_vars, rcf);
                    convert_closure_calls_expr(hi, closure_vars, rcf);
                }
                if let Some(g) = &mut arm.guard { convert_closure_calls_expr(g, closure_vars, rcf); }
                convert_closure_calls(&mut arm.body, &mut closure_vars.to_vec(), rcf);
            }
        }
        ExprKind::Closure { body, .. } => convert_closure_calls_expr(body, closure_vars, rcf),
        ExprKind::MethodOn { recv, args, .. } => {
            convert_closure_calls_expr(recv, closure_vars, rcf);
            for a in args { convert_closure_calls_expr(a, closure_vars, rcf); }
        }
        ExprKind::Borrow { inner, .. } => convert_closure_calls_expr(inner, closure_vars, rcf),
        ExprKind::Ok(a) | ExprKind::Err(a) | ExprKind::Try(a) | ExprKind::Some(a) => convert_closure_calls_expr(a, closure_vars, rcf),
        ExprKind::None => {}
        ExprKind::TryBlock { body, catches, fin } => {
            convert_closure_calls(body, &mut closure_vars.to_vec(), rcf);
            for c in catches { convert_closure_calls(&mut c.body, &mut closure_vars.to_vec(), rcf); }
            if let Some(b) = fin { convert_closure_calls(b, &mut closure_vars.to_vec(), rcf); }
        }
        _ => {}
    }
}
