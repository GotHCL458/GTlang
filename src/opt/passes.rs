//! 额外优化 pass：if 常量条件剪枝、无用变量删除、公共子表达式消除、循环不变量提升。

use crate::ast::*;
use std::collections::HashMap;

/// if 常量条件剪枝：`if true {A} else {B}` → A；`if false {A} else {B}` → B。
pub fn const_if_prune(prog: &mut Program) {
    for item in prog.items.iter_mut() {
        match item {
            Item::Fn(f) => prune_block(&mut f.body),
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods.iter_mut() { prune_block(&mut m.body); }
            }
            _ => {}
        }
    }
}

fn prune_block(b: &mut Block) {
    for s in b.iter_mut() {
        match s {
            Stmt::If { cond, then, els, .. } => {
                prune_block(then);
                if let Some(e) = els { prune_block(e); }
                if let ExprKind::Bool(c) = cond.kind {
                    if c {
                        let taken = std::mem::take(then);
                        *s = Stmt::Block(taken);
                        if let Stmt::Block(inner) = s { prune_block(inner); }
                    } else if let Some(mut e) = els.take() {
                        prune_block(&mut e);
                        *s = Stmt::Block(e);
                        if let Stmt::Block(inner) = s { prune_block(inner); }
                    } else {
                        *s = Stmt::Block(Vec::new());
                    }
                }
            }
            Stmt::While { cond, body, .. } => {
                prune_block(body);
                if let ExprKind::Bool(false) = cond.kind { *s = Stmt::Block(Vec::new()); }
            }
            Stmt::Block(inner) => prune_block(inner),
            Stmt::ForRange { body, els, .. } | Stmt::ForEach { body, els, .. } => {
                prune_block(body);
                if let Some(e) = els { prune_block(e); }
            }
            _ => {}
        }
    }
}

/// 无用变量删除：局部 `let x = <纯表达式>` 若 x 从未被使用 → 删除。
pub fn remove_unused_lets(prog: &mut Program) {
    for item in prog.items.iter_mut() {
        match item {
            Item::Fn(f) => remove_unused_block(&mut f.body),
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods.iter_mut() { remove_unused_block(&mut m.body); }
            }
            _ => {}
        }
    }
}

fn remove_unused_block(b: &mut Block) {
    // 先递归子块
    for s in b.iter_mut() {
        match s {
            Stmt::If { then, els, .. } => { remove_unused_block(then); if let Some(e) = els { remove_unused_block(e); } }
            Stmt::While { body, .. } => remove_unused_block(body),
            Stmt::Block(inner) => remove_unused_block(inner),
            Stmt::ForRange { body, els, .. } | Stmt::ForEach { body, els, .. } => {
                remove_unused_block(body);
                if let Some(e) = els { remove_unused_block(e); }
            }
            _ => {}
        }
    }
    // 收集本块（含子块）所有被读取的标识符
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    for s in b.iter() { collect_used_stmt(s, &mut used); }
    // 删除：`let x = <纯>` 且 x 不在 used 中
    b.retain(|s| {
        if let Stmt::Let { name, value, .. } = s {
            if !used.contains(name) && is_pure(value) { return false; }
        }
        true
    });
}

fn collect_used_stmt(s: &Stmt, out: &mut std::collections::HashSet<String>) {
    match s {
        Stmt::Let { value, .. } | Stmt::Const { value, .. } => collect_used_expr(value, out),
        Stmt::Assign { name, index, value, .. } => {
            // 赋值目标名也视为"使用"（保守：不删被赋值的变量，避免把重赋值变成未定义）。
            out.insert(name.clone());
            if let Some(i) = index { collect_used_expr(i, out); }
            collect_used_expr(value, out);
        }
        Stmt::FieldAssign { obj, value, .. } => { out.insert(obj.clone()); collect_used_expr(value, out); }
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => collect_used_expr(e, out),
        Stmt::If { cond, then, els, .. } => {
            collect_used_expr(cond, out);
            for st in then { collect_used_stmt(st, out); }
            if let Some(e) = els { for st in e { collect_used_stmt(st, out); } }
        }
        Stmt::While { cond, body, .. } => { collect_used_expr(cond, out); for st in body { collect_used_stmt(st, out); } }
        Stmt::DoWhile { body, cond, .. } => { for st in body { collect_used_stmt(st, out); } collect_used_expr(cond, out); }
        Stmt::Block(inner) => for st in inner { collect_used_stmt(st, out); },
        Stmt::ForRange { from, to, body, els, .. } => {
            collect_used_expr(from, out); collect_used_expr(to, out);
            for st in body { collect_used_stmt(st, out); }
            if let Some(e) = els { for st in e { collect_used_stmt(st, out); } }
        }
        Stmt::ForEach { iter, body, els, .. } => {
            collect_used_expr(iter, out);
            for st in body { collect_used_stmt(st, out); }
            if let Some(e) = els { for st in e { collect_used_stmt(st, out); } }
        }
        Stmt::Throw(e, _) => collect_used_expr(e, out),
        Stmt::Defer(e, _) => collect_used_expr(e, out),
        Stmt::Labeled { inner, .. } => collect_used_stmt(inner, out),
        Stmt::LocalFn(fd) => for st in &fd.body { collect_used_stmt(st, out); },
        Stmt::Go { func, args, .. } => { out.insert(func.clone()); for a in args { collect_used_expr(a, out); } }
        Stmt::Try { body, catches, fin, .. } => {
            for st in body { collect_used_stmt(st, out); }
            for ca in catches {
                if let Some(b) = &ca.binding { out.insert(b.clone()); }
                if let Some(g) = &ca.guard { collect_used_expr(g, out); }
                for st in &ca.body { collect_used_stmt(st, out); }
            }
            if let Some(f) = fin { for st in f { collect_used_stmt(st, out); } }
        }
        // 兜底：任何未显式覆盖的语句，递归其子块，避免遗漏导致误删。
        other => { other.each_block(&mut |blk| { for st in blk { collect_used_stmt(st, out); } }); }
    }
}

fn collect_used_expr(e: &Expr, out: &mut std::collections::HashSet<String>) {
    match &e.kind {
        ExprKind::Ident(n) => { out.insert(n.clone()); }
        ExprKind::Unary(_, a) | ExprKind::Field(a, _) | ExprKind::Try(a) | ExprKind::Ok(a) | ExprKind::Err(a) | ExprKind::Some(a) | ExprKind::Borrow { inner: a, .. } => collect_used_expr(a, out),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => { collect_used_expr(a, out); collect_used_expr(b, out); }
        ExprKind::Call(_, args) | ExprKind::ArrayLit(args) | ExprKind::TupleLit(args) => for a in args { collect_used_expr(a, out); },
        ExprKind::CallValue { callee, args } => { collect_used_expr(callee, out); for a in args { collect_used_expr(a, out); } }
        ExprKind::MethodOn { recv, args, .. } => { collect_used_expr(recv, out); for a in args { collect_used_expr(a, out); } }
        ExprKind::StructLit(_, fs) => for (_, v) in fs { collect_used_expr(v, out); },
        ExprKind::EnumLit(_, _, args) => for a in args { collect_used_expr(a, out); },
        ExprKind::Slice(a, b, c) => { collect_used_expr(a, out); collect_used_expr(b, out); collect_used_expr(c, out); }
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { collect_used_expr(i, out); } }
        ExprKind::If { cond, then, els } => {
            collect_used_expr(cond, out);
            for st in then { collect_used_stmt(st, out); }
            if let Some(e) = els { for st in e { collect_used_stmt(st, out); } }
        }
        ExprKind::Closure { body, .. } => collect_used_expr(body, out),
        ExprKind::ClosureNew { captures, .. } => for c in captures { collect_used_expr(c, out); },
        ExprKind::DynBox { value, .. } => collect_used_expr(value, out),
        ExprKind::Match { subject, arms } => {
            collect_used_expr(subject, out);
            for arm in arms {
                if let Some(p) = &arm.pat { collect_used_expr(p, out); }
                if let Some((lo, hi)) = &arm.range { collect_used_expr(lo, out); collect_used_expr(hi, out); }
                if let Some(g) = &arm.guard { collect_used_expr(g, out); }
                for st in &arm.body { collect_used_stmt(st, out); }
            }
        }
        ExprKind::ListComp { expr: ex, iter, cond, .. } => {
            collect_used_expr(ex, out); collect_used_expr(iter, out);
            if let Some(c) = cond { collect_used_expr(c, out); }
        }
        ExprKind::EnumLit(_, _, args) => for a in args { collect_used_expr(a, out); },
        ExprKind::Comptime(b) => for st in b { collect_used_stmt(st, out); },
        ExprKind::TryBlock { body, catches, fin } => {
            for st in body { collect_used_stmt(st, out); }
            for ca in catches { for st in &ca.body { collect_used_stmt(st, out); } }
            if let Some(f) = fin { for st in f { collect_used_stmt(st, out); } }
        }
        // 兜底：任何未显式覆盖的表达式，递归其子表达式 + 子块，避免遗漏导致误删。
        _ => {
            crate::ast::each_child_expr(e, &mut |c: &Expr| collect_used_expr(c, out));
            crate::ast::each_expr_block_inner(e, &mut |blk: &Block| { for st in blk { collect_used_stmt(st, out); } });
        }
    }
}

fn is_pure(e: &Expr) -> bool {
    matches!(e.kind, ExprKind::Ident(_) | ExprKind::Int(_) | ExprKind::Float(_)
        | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Decimal(_))
}

/// 公共子表达式消除（CSE）：块内 `let a = <expr>` 与 `let b = <同一 expr>` 且 expr 纯 → b 复用 a。
/// 保守：只对纯表达式（Ident/字面量）做，避免副作用重复。
pub fn cse(prog: &mut Program) {
    for item in prog.items.iter_mut() {
        match item {
            Item::Fn(f) => cse_block(&mut f.body),
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods.iter_mut() { cse_block(&mut m.body); }
            }
            _ => {}
        }
    }
}

fn expr_key(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(n) => Some(format!("id:{}", n)),
        ExprKind::Int(v) => Some(format!("i:{}", v)),
        ExprKind::Bool(b) => Some(format!("b:{}", b)),
        ExprKind::Str(s) => Some(format!("s:{}", s)),
        ExprKind::Binary(op, a, b) => Some(format!("{}:{}{}", "bin", expr_key(a)?, format!("{:?}{}", op, expr_key(b)?))),
        _ => None,
    }
}

fn cse_block(b: &mut Block) {
    for s in b.iter_mut() {
        match s {
            Stmt::If { then, els, .. } => { cse_block(then); if let Some(e) = els { cse_block(e); } }
            Stmt::While { body, .. } => cse_block(body),
            Stmt::Block(inner) => cse_block(inner),
            Stmt::ForRange { body, els, .. } | Stmt::ForEach { body, els, .. } => {
                cse_block(body);
                if let Some(e) = els { cse_block(e); }
            }
            _ => {}
        }
    }
    let mut seen: HashMap<String, String> = HashMap::new();
    for s in b.iter_mut() {
        // CSE 只处理不可变 let（let mut 的值可能改变，复用会错）。
        if let Stmt::Let { name, value, mutable, .. } = s {
            if *mutable { continue; }
            if let Some(k) = expr_key(value) {
                if let Some(prev) = seen.get(&k) {
                    *value = Expr::new(ExprKind::Ident(prev.clone()), value.line);
                } else {
                    seen.insert(k, name.clone());
                }
            }
        }
    }
}

/// 循环不变量提升（简单版）：`for ... { let x = <纯且不依赖循环变量> ... }` → 提到循环外。
/// 保守：仅当 let 的初值是纯表达式、且其名不在循环体被赋值/依赖循环变量时提升。
pub fn licm(prog: &mut Program) {
    for item in prog.items.iter_mut() {
        match item {
            Item::Fn(f) => licm_block(&mut f.body),
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods.iter_mut() { licm_block(&mut m.body); }
            }
            _ => {}
        }
    }
}

fn licm_block(b: &mut Block) {
    for s in b.iter_mut() {
        match s {
            Stmt::Block(inner) => licm_block(inner),
            _ => {}
        }
    }
    // 对 for/while 循环：把体内"纯 + 名字未被赋值"的 let 提到循环前
    let mut i = 0;
    while i < b.len() {
        let mut hoisted: Vec<Stmt> = Vec::new();
        let mut is_loop = false;
        {
            let s = &mut b[i];
            let (body, var): (&mut Block, Option<&str>) = match s {
                Stmt::ForRange { var, body, .. } => { is_loop = true; (body, Some(var.as_str())) }
                Stmt::ForEach { var, body, .. } => { is_loop = true; (body, Some(var.as_str())) }
                Stmt::While { body, .. } => { is_loop = true; (body, None) }
                _ => { i += 1; continue; }
            };
            if is_loop {
                let loopvar = var.map(|v| v.to_string());
                // 循环体内被赋值的名字
                let mut assigned: std::collections::HashSet<String> = std::collections::HashSet::new();
                for st in body.iter() { collect_assigned_local(st, &mut assigned); }
                let mut keep: Block = Vec::new();
                for st in body.drain(..) {
                    let mut do_hoist = false;
                    if let Stmt::Let { name, value, .. } = &st {
                        let is_const_val = matches!(value.kind, ExprKind::Ident(_) | ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_));
                        let uses_loopvar = if let Some(lv) = &loopvar { expr_uses(value, lv) } else { false };
                        if is_const_val && !assigned.contains(name) && !uses_loopvar {
                            do_hoist = true;
                        }
                    }
                    if do_hoist { hoisted.push(st); } else { keep.push(st); }
                }
                *body = keep;
            }
        }
        if !hoisted.is_empty() {
            for h in hoisted.into_iter().rev() { b.insert(i, h); }
            i += 1; // 跳过提升后的语句（可能仍想对其后的循环处理，但简单起见前进）
        }
        i += 1;
    }
}

fn expr_uses(e: &Expr, name: &str) -> bool {
    let mut used = std::collections::HashSet::new();
    collect_used_expr(e, &mut used);
    used.contains(name)
}

fn collect_assigned_local(s: &Stmt, out: &mut std::collections::HashSet<String>) {
    match s {
        Stmt::Assign { name, .. } => { out.insert(name.clone()); }
        Stmt::FieldAssign { .. } => {}
        Stmt::If { then, els, .. } => {
            for st in then { collect_assigned_local(st, out); }
            if let Some(e) = els { for st in e { collect_assigned_local(st, out); } }
        }
        Stmt::While { body, .. } => for st in body { collect_assigned_local(st, out); },
        Stmt::Block(inner) => for st in inner { collect_assigned_local(st, out); },
        Stmt::ForRange { body, els, .. } | Stmt::ForEach { body, els, .. } => {
            for st in body { collect_assigned_local(st, out); }
            if let Some(e) = els { for st in e { collect_assigned_local(st, out); } }
        }
        _ => {}
    }
}
