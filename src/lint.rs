//! GTLang 静态检查（供 `gtc --lint` 使用）。
//!
//! 检查项：未用函数、不可达代码、空 if 分支、常量条件（if true/while false）、自比较。

use std::collections::{HashMap, HashSet};

use crate::ast::*;

/// 返回 (行号, 消息) 列表。
pub fn lint(prog: &Program) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut defined_fns: HashMap<String, usize> = HashMap::new();
    let mut used_fns: HashSet<String> = HashSet::new();
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                defined_fns.insert(f.name.clone(), f.line);
                collect_used(&f.body, &mut used_fns);
                check_block(&f.body, &mut out);
                check_unused_bindings(f, &mut out);
            }
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods {
                    collect_used(&m.body, &mut used_fns);
                    check_block(&m.body, &mut out);
                }
            }
            _ => {}
        }
    }
    for (name, line) in &defined_fns {
        if name == "main" { continue; }
        if name.contains("__") { continue; }
        if !used_fns.contains(name) {
            out.push((*line, format!("函数 '{}' 从未被调用", name)));
        }
    }
    out.sort_by_key(|(l, _)| *l);
    out
}

fn check_block(b: &Block, out: &mut Vec<(usize, String)>) {
    let mut terminated = false;
    for s in b {
        if terminated {
            out.push((stmt_line(s), "不可达代码（前面的语句已终止）".into()));
            break;
        }
        match s {
            Stmt::Return(..) | Stmt::Break(..) | Stmt::Continue(..) | Stmt::Throw(..) => terminated = true,
            Stmt::Block(inner) => check_block(inner, out),
            Stmt::If { cond, then, els, .. } => {
                if let ExprKind::Bool(v) = &cond.kind {
                    out.push((cond.line, format!("if {} 是常量条件", v)));
                }
                if then.is_empty() && els.as_ref().map(|e| e.is_empty()).unwrap_or(true) {
                    out.push((stmt_line(s), "if 两个分支都为空".into()));
                }
                check_block(then, out);
                if let Some(e) = els { check_block(e, out); }
            }
            Stmt::While { cond, body, .. } => {
                if let ExprKind::Bool(false) = &cond.kind {
                    out.push((cond.line, "while false 恒不执行".into()));
                }
                check_block(body, out);
                check_loop_body(body, out);
            }
            Stmt::DoWhile { body, .. } => { check_block(body, out); check_loop_body(body, out); }
            Stmt::ForRange { body, els, .. } => { check_block(body, out); check_loop_body(body, out); if let Some(e) = els { check_block(e, out); } }
            Stmt::ForEach { body, els, .. } => { check_block(body, out); check_loop_body(body, out); if let Some(e) = els { check_block(e, out); } }
            Stmt::Try { body, catches, fin, .. } => {
                check_block(body, out);
                for c in catches { check_block(&c.body, out); }
                if let Some(f) = fin { check_block(f, out); }
            }
            Stmt::Expr(e) => check_expr(e, out),
            Stmt::Let { value, .. } | Stmt::Const { value, .. } | Stmt::Assign { value, .. } => check_expr(value, out),
            _ => {}
        }
    }
}

/// 检查循环体：`s = s + x` / `s += x`（字符串累积）在循环里会 O(n²) 复制 + 泄漏。
fn check_loop_body(b: &Block, out: &mut Vec<(usize, String)>) {
    // 收集本块内的字符串变量（初值为字符串字面量/拼接）
    let mut str_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
    for s in b {
        if let Stmt::Let { name, value, .. } | Stmt::Const { name, value, .. } = s {
            if is_string_expr(value) { str_vars.insert(name.clone()); }
        }
    }
    for s in b {
        if let Stmt::Assign { name, op, value, .. } = s {
            let is_self_append = match op {
                Some(BinOp::Add) => true,
                None => matches!(&value.kind, ExprKind::Binary(BinOp::Add, l, _) if matches!(&l.kind, ExprKind::Ident(n) if n == name)),
                _ => false,
            };
            if is_self_append && (str_vars.contains(name) || is_string_expr(value)) {
                out.push((stmt_line(s), format!("循环里 '{} = {} + x' 会反复复制字符串（O(n²) + 内存增长）；建议用 sb_new/sb_push_str/sb_finish", name, name)));
            }
        }
        let mut visit = |blk: &Block| check_loop_body(blk, out);
        s.each_block(&mut visit);
    }
}

/// 启发式：表达式是否为字符串（字面量 / 字符串拼接 / str() 调用）
fn is_string_expr(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Str(_) | ExprKind::Interp(_) => true,
        ExprKind::Binary(BinOp::Add, a, b) => is_string_expr(a) || is_string_expr(b),
        ExprKind::Call(n, _) => n == "str" || n == "string" || n == "format" || n == "join",
        _ => false,
    }
}
fn check_expr(e: &Expr, out: &mut Vec<(usize, String)>) {
    match &e.kind {
        ExprKind::Binary(op, a, b) => {
            if *op == BinOp::Eq || *op == BinOp::Ne {
                if let (ExprKind::Ident(x), ExprKind::Ident(y)) = (&a.kind, &b.kind) {
                    if x == y {
                        out.push((e.line, format!("{} 与自身比较", x)));
                    }
                }
            }
            check_expr(a, out);
            check_expr(b, out);
        }
        ExprKind::Unary(_, a) => check_expr(a, out),
        ExprKind::Call(_, args) => for a in args { check_expr(a, out); },
        ExprKind::CallNamed(_, named) => for (_, a) in named { check_expr(a, out); },
        ExprKind::Index(a, b) => { check_expr(a, out); check_expr(b, out); }
        ExprKind::Field(a, _) => check_expr(a, out),
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { check_expr(i, out); } },
        ExprKind::If { cond, then, els } => {
            check_expr(cond, out);
            check_block(then, out);
            if let Some(x) = els { check_block(x, out); }
        }
        _ => {}
    }
}

fn collect_used(b: &Block, out: &mut HashSet<String>) {
    for s in b { collect_stmt(s, out); }
}

fn collect_stmt(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Let { value, .. } | Stmt::Const { value, .. } => collect_expr(value, out),
        Stmt::Assign { value, index, .. } => { collect_expr(value, out); if let Some(i) = index { collect_expr(i, out); } }
        Stmt::FieldAssign { value, .. } => collect_expr(value, out),
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => collect_expr(e, out),
        Stmt::If { cond, then, els, .. } => { collect_expr(cond, out); collect_used(then, out); if let Some(e) = els { collect_used(e, out); } }
        Stmt::While { cond, body, .. } => { collect_expr(cond, out); collect_used(body, out); }
        Stmt::DoWhile { body, cond, .. } => { collect_used(body, out); collect_expr(cond, out); }
        Stmt::ForRange { from, to, body, els, .. } => { collect_expr(from, out); collect_expr(to, out); collect_used(body, out); if let Some(e) = els { collect_used(e, out); } }
        Stmt::ForEach { iter, body, els, .. } => { collect_expr(iter, out); collect_used(body, out); if let Some(e) = els { collect_used(e, out); } }
        Stmt::Block(inner) => collect_used(inner, out),
        Stmt::Throw(e, _) => collect_expr(e, out),
        Stmt::Go { func, args, .. } => { out.insert(func.clone()); for a in args { collect_expr(a, out); } }
        Stmt::Try { body, catches, fin, .. } => {
            collect_used(body, out);
            for c in catches { collect_used(&c.body, out); }
            if let Some(f) = fin { collect_used(f, out); }
        }
        _ => {}
    }
}

fn collect_expr(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Call(name, args) => { out.insert(name.clone()); for a in args { collect_expr(a, out); } }
        ExprKind::CallNamed(name, named) => { out.insert(name.clone()); for (_, a) in named { collect_expr(a, out); } }
        ExprKind::Unary(_, a) | ExprKind::Field(a, _) | ExprKind::Borrow { inner: a, .. }
        | ExprKind::Ok(a) | ExprKind::Err(a) | ExprKind::Some(a) | ExprKind::Try(a) => collect_expr(a, out),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => { collect_expr(a, out); collect_expr(b, out); }
        ExprKind::Slice(a, b, c) => { collect_expr(a, out); collect_expr(b, out); collect_expr(c, out); }
        ExprKind::ArrayLit(xs) | ExprKind::TupleLit(xs) => for x in xs { collect_expr(x, out); }
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { collect_expr(i, out); } }
        ExprKind::StructLit(_, fs) => for (_, v) in fs { collect_expr(v, out); }
        ExprKind::If { cond, then, els } => { collect_expr(cond, out); collect_used(then, out); if let Some(x) = els { collect_used(x, out); } }
        ExprKind::Match { subject, arms } => {
            collect_expr(subject, out);
            for a in arms { if let Some(g) = &a.guard { collect_expr(g, out); } collect_used(&a.body, out); }
        }
        ExprKind::CallValue { callee, args } => { collect_expr(callee, out); for a in args { collect_expr(a, out); } }
        ExprKind::ClosureNew { captures, .. } => for c in captures { collect_expr(c, out); }
        ExprKind::TryBlock { body, catches, fin } => {
            collect_used(body, out);
            for c in catches { collect_used(&c.body, out); }
            if let Some(f) = fin { collect_used(f, out); }
        }
        ExprKind::ListComp { expr, iter, cond, .. } => { collect_expr(expr, out); collect_expr(iter, out); if let Some(c) = cond { collect_expr(c, out); } }
        ExprKind::EnumLit(_, _, args) => for a in args { collect_expr(a, out); },
        _ => {}
    }
}

fn stmt_line(s: &Stmt) -> usize {
    match s {
        Stmt::Let { line, .. } | Stmt::Const { line, .. } | Stmt::Assign { line, .. }
        | Stmt::FieldAssign { line, .. } | Stmt::If { line, .. } | Stmt::While { line, .. }
        | Stmt::DoWhile { line, .. } | Stmt::ForRange { line, .. } | Stmt::ForEach { line, .. }
        | Stmt::Return(_, line) | Stmt::Break(_, line) | Stmt::Continue(_, line)
        | Stmt::Labeled { line, .. } | Stmt::Throw(_, line) | Stmt::Go { line, .. } => *line,
        Stmt::Expr(e) => e.line,
        Stmt::Block(b) => b.first().map(stmt_line).unwrap_or(0),
        Stmt::Try { line, .. } => *line,
        _ => 0,
    }
}

/// 检查未用变量/参数（保守：仅当标识符在函数体内"从未出现"时报告）。
fn check_unused_bindings(f: &FnDef, out: &mut Vec<(usize, String)>) {
    // 收集所有标识符出现（读/写）
    let mut seen: HashSet<String> = HashSet::new();
    collect_idents_block(&f.body, &mut seen);
    // 参数
    for p in &f.params {
        if !seen.contains(&p.name) {
            out.push((f.line, format!("参数 '{}' 从未使用", p.name)));
        }
    }
    // let / := / const 定义的局部变量
    let mut locals: Vec<(String, usize)> = Vec::new();
    collect_locals_block(&f.body, &mut locals);
    for (name, line) in locals {
        if !seen.contains(&name) {
            out.push((line, format!("变量 '{}' 从未使用", name)));
        }
    }
}

fn collect_idents_block(b: &Block, out: &mut HashSet<String>) {
    for s in b { collect_idents_stmt(s, out); }
}

fn collect_idents_stmt(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        // 定义（let/:=/const）不算"使用"；赋值（Assign）算"使用"（写也要读旧值？—— 保守：算使用）
        Stmt::Let { value, .. } | Stmt::Const { value, .. } => collect_idents_expr(value, out),
        Stmt::Assign { name, value, index, .. } => { out.insert(name.clone()); collect_idents_expr(value, out); if let Some(i) = index { collect_idents_expr(i, out); } }
        Stmt::FieldAssign { obj, value, .. } => { out.insert(obj.clone()); collect_idents_expr(value, out); }
        Stmt::Expr(e) | Stmt::Return(Some(e), _) | Stmt::Throw(e, _) => collect_idents_expr(e, out),
        Stmt::Return(None, _) => {}
        Stmt::If { cond, then, els, .. } => { collect_idents_expr(cond, out); collect_idents_block(then, out); if let Some(e) = els { collect_idents_block(e, out); } }
        Stmt::While { cond, body, .. } => { collect_idents_expr(cond, out); collect_idents_block(body, out); }
        Stmt::DoWhile { body, cond, .. } => { collect_idents_block(body, out); collect_idents_expr(cond, out); }
        Stmt::ForRange { var, from, to, body, els, .. } => { out.insert(var.clone()); collect_idents_expr(from, out); collect_idents_expr(to, out); collect_idents_block(body, out); if let Some(e) = els { collect_idents_block(e, out); } }
        Stmt::ForEach { var, iter, body, els, .. } => { out.insert(var.clone()); collect_idents_expr(iter, out); collect_idents_block(body, out); if let Some(e) = els { collect_idents_block(e, out); } }
        Stmt::Block(inner) => collect_idents_block(inner, out),
        Stmt::Go { func, args, .. } => { out.insert(func.clone()); for a in args { collect_idents_expr(a, out); } }
        Stmt::Try { body, catches, fin, .. } => {
            collect_idents_block(body, out);
            for c in catches { collect_idents_block(&c.body, out); }
            if let Some(f) = fin { collect_idents_block(f, out); }
        }
        Stmt::Labeled { inner, .. } => collect_idents_stmt(inner, out),
        _ => {}
    }
}

fn collect_idents_expr(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Ident(n) => { out.insert(n.clone()); }
        ExprKind::Call(name, args) => { out.insert(name.clone()); for a in args { collect_idents_expr(a, out); } }
        ExprKind::CallNamed(name, named) => { out.insert(name.clone()); for (_, a) in named { collect_idents_expr(a, out); } }
        ExprKind::Unary(_, a) | ExprKind::Field(a, _) | ExprKind::Borrow { inner: a, .. }
        | ExprKind::Ok(a) | ExprKind::Err(a) | ExprKind::Some(a) | ExprKind::Try(a) => collect_idents_expr(a, out),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => { collect_idents_expr(a, out); collect_idents_expr(b, out); }
        ExprKind::Slice(a, b, c) => { collect_idents_expr(a, out); collect_idents_expr(b, out); collect_idents_expr(c, out); }
        ExprKind::ArrayLit(xs) | ExprKind::TupleLit(xs) => for x in xs { collect_idents_expr(x, out); }
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { collect_idents_expr(i, out); } }
        ExprKind::StructLit(_, fs) => for (_, v) in fs { collect_idents_expr(v, out); }
        ExprKind::If { cond, then, els } => { collect_idents_expr(cond, out); collect_idents_block(then, out); if let Some(x) = els { collect_idents_block(x, out); } }
        ExprKind::Match { subject, arms } => {
            collect_idents_expr(subject, out);
            for a in arms {
                if let Some(p) = &a.pat { collect_idents_expr(p, out); }
                if let Some(g) = &a.guard { collect_idents_expr(g, out); }
                collect_idents_block(&a.body, out);
            }
        }
        ExprKind::CallValue { callee, args } => { collect_idents_expr(callee, out); for a in args { collect_idents_expr(a, out); } }
        ExprKind::ClosureNew { captures, .. } => for c in captures { collect_idents_expr(c, out); }
        ExprKind::TryBlock { body, catches, fin } => {
            collect_idents_block(body, out);
            for c in catches { collect_idents_block(&c.body, out); }
            if let Some(f) = fin { collect_idents_block(f, out); }
        }
        ExprKind::ListComp { expr, var, iter, cond } => {
            out.insert(var.clone());
            collect_idents_expr(expr, out);
            collect_idents_expr(iter, out);
            if let Some(c) = cond { collect_idents_expr(c, out); }
        }
        ExprKind::EnumLit(_, _, args) => for a in args { collect_idents_expr(a, out); },
        _ => {}
    }
}

/// 收集局部变量定义（let / := / const）及其行号。
fn collect_locals_block(b: &Block, out: &mut Vec<(String, usize)>) {
    for s in b { collect_locals_stmt(s, out); }
}

fn collect_locals_stmt(s: &Stmt, out: &mut Vec<(String, usize)>) {
    match s {
        Stmt::Let { name, line, .. } | Stmt::Const { name, line, .. } => out.push((name.clone(), *line)),
        Stmt::Block(inner) => collect_locals_block(inner, out),
        Stmt::If { then, els, .. } => { collect_locals_block(then, out); if let Some(e) = els { collect_locals_block(e, out); } }
        Stmt::While { body, .. } | Stmt::DoWhile { body, .. } | Stmt::ForRange { body, .. } | Stmt::ForEach { body, .. } => collect_locals_block(body, out),
        Stmt::Try { body, catches, fin, .. } => {
            collect_locals_block(body, out);
            for c in catches { collect_locals_block(&c.body, out); }
            if let Some(f) = fin { collect_locals_block(f, out); }
        }
        _ => {}
    }
}

#[path = "tests/lint.rs"]
#[cfg(test)]
mod lint_tests;
