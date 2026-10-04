//! 优化 pass：内联 + 常量折叠 + 死代码消除（AST 级，双后端共享）。
//!
//! 分两段调用（在 frontend 里）：
//!   - inline_and_fold：sema(1) 之后（sema(2) 会重推断类型）
//!   - dead_code：sema(2) 之后（类型检查已过，删除安全）

use crate::ast::*;

/// 第一段：内联 + 常量折叠。
pub fn inline_and_fold(prog: &mut Program) {
    inline_simple(prog);
    for item in prog.items.iter_mut() {
        match item {
            Item::Fn(f) => optimize_block(&mut f.body),
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods.iter_mut() { optimize_block(&mut m.body); }
            }
            _ => {}
        }
    }
    // 常量传播 + 无用代码消除（在折叠之后，暴露更多常量）
    for item in prog.items.iter_mut() {
        match item {
            Item::Fn(f) => { propagate_block(&mut f.body); optimize_block(&mut f.body); }
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods.iter_mut() { propagate_block(&mut m.body); optimize_block(&mut m.body); }
            }
            _ => {}
        }
    }
}

/// 递归收集块及其子块中被赋值（Assign）的变量名。
fn collect_assigned(b: &Block, out: &mut std::collections::HashSet<String>) {
    for s in b {
        if let Stmt::Assign { name, .. } = s { out.insert(name.clone()); }
        // 遍历驱动：所有含块变体（含 DoWhile/Labeled/Try/LocalFn）统一走 each_block，
        // 新增变体只需在 ast::Stmt::each_block 补一处。
        s.each_block(&mut |blk| collect_assigned(blk, out));
    }
}

/// 常量传播：不可变局部变量若绑定到字面量，把其使用处替换为字面量。
fn propagate_block(b: &mut Block) {
    use std::collections::HashMap;
    // 先收集本块内"不可变 + 字面量"的绑定
    // 收集块及子块内被赋值的变量名（这些不能传播）
    let mut assigned: std::collections::HashSet<String> = std::collections::HashSet::new();
    collect_assigned(b, &mut assigned);
    let mut consts: HashMap<String, Expr> = HashMap::new();
    for s in b.iter() {
        if let Stmt::Let { name, value, .. } = s {
            if !assigned.contains(name) && is_literal(value) {
                consts.insert(name.clone(), value.clone());
            }
        }
    }
    if consts.is_empty() { return; }
    // 替换：先处理本块后续语句，再递归子块（用同一映射）
    // 简化：直接对每条语句做替换（变量在被绑定后才可用，但字面量替换语义等价）
    for s in b.iter_mut() {
        propagate_stmt(s, &consts);
    }
}

fn propagate_stmt(s: &mut Stmt, consts: &std::collections::HashMap<String, Expr>) {
    match s {
        Stmt::Let { value, .. } | Stmt::Const { value, .. } => subst_consts(value, consts),
        Stmt::Assign { value, index, .. } => { subst_consts(value, consts); if let Some(i) = index { subst_consts(i, consts); } }
        Stmt::FieldAssign { value, .. } => subst_consts(value, consts),
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => subst_consts(e, consts),
        Stmt::If { cond, then, els, .. } => { subst_consts(cond, consts); for st in then.iter_mut() { propagate_stmt(st, consts); } if let Some(e) = els { for st in e.iter_mut() { propagate_stmt(st, consts); } } }
        Stmt::While { cond, body, .. } => { subst_consts(cond, consts); for st in body.iter_mut() { propagate_stmt(st, consts); } }
        Stmt::ForRange { from, to, body, .. } => { subst_consts(from, consts); subst_consts(to, consts); for st in body.iter_mut() { propagate_stmt(st, consts); } }
        Stmt::ForEach { iter, body, .. } => { subst_consts(iter, consts); for st in body.iter_mut() { propagate_stmt(st, consts); } }
        Stmt::Block(inner) => for st in inner.iter_mut() { propagate_stmt(st, consts); },
        Stmt::Throw(e, _) => subst_consts(e, consts),
        _ => {}
    }
}

fn is_literal(e: &Expr) -> bool {
    // 仅传播 Copy 类型字面量；字符串有 move 语义，传播会掩盖所有权错误
    matches!(e.kind, ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_))
}

fn subst_consts(e: &mut Expr, consts: &std::collections::HashMap<String, Expr>) {
    match &mut e.kind {
        ExprKind::Ident(n) => {
            if let Some(r) = consts.get(n) { e.kind = r.kind.clone(); e.ty = r.ty.clone(); }
        }
        ExprKind::Unary(_, a) => subst_consts(a, consts),
        ExprKind::Binary(_, a, b) => { subst_consts(a, consts); subst_consts(b, consts); }
        ExprKind::Call(_, args) => for a in args { subst_consts(a, consts); },
        ExprKind::CallValue { callee, args } => { subst_consts(callee, consts); for a in args { subst_consts(a, consts); } }
        ExprKind::Index(a, b) => { subst_consts(a, consts); subst_consts(b, consts); }
        ExprKind::ArrayLit(xs) => for x in xs { subst_consts(x, consts); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { subst_consts(i, consts); } },
        ExprKind::Field(base, _) => subst_consts(base, consts),
        ExprKind::StructLit(_, fs) => for (_, v) in fs { subst_consts(v, consts); },
        ExprKind::If { cond, then, els } => { subst_consts(cond, consts); for st in then.iter_mut() { propagate_stmt(st, consts); } if let Some(x) = els { for st in x.iter_mut() { propagate_stmt(st, consts); } } }
        _ => {}
    }
}

/// 第二段：死代码消除。
pub fn dead_code(prog: &mut Program) {
    dead_fn_elim(prog);
}

/// 默认参数：把调用点缺失的实参补上默认值（AST 级，codegen/jit 无需改）。
pub fn apply_defaults(prog: &mut Program) {
    use std::collections::HashMap;
    // 收集 函数名 → (参数名列表, 默认值列表)
    let mut defs: HashMap<String, Vec<Option<Expr>>> = HashMap::new();
    for item in &prog.items {
        if let Item::Fn(f) = item {
            defs.insert(f.name.clone(), f.params.iter().map(|p| p.default.clone()).collect());
        }
    }
    if defs.is_empty() { return; }
    for item in prog.items.iter_mut() {
        match item {
            Item::Fn(f) => patch_block(&mut f.body, &defs),
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods.iter_mut() { patch_block(&mut m.body, &defs); }
            }
            _ => {}
        }
    }
}

fn patch_block(b: &mut Block, defs: &std::collections::HashMap<String, Vec<Option<Expr>>>) {
    for s in b.iter_mut() { patch_stmt(s, defs); }
}

fn patch_stmt(s: &mut Stmt, defs: &std::collections::HashMap<String, Vec<Option<Expr>>>) {
    match s {
        Stmt::Let { value, .. } | Stmt::Const { value, .. } => patch_expr(value, defs),
        Stmt::Assign { value, index, .. } => { patch_expr(value, defs); if let Some(i) = index { patch_expr(i, defs); } }
        Stmt::FieldAssign { value, .. } => patch_expr(value, defs),
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => patch_expr(e, defs),
        Stmt::If { cond, then, els, .. } => { patch_expr(cond, defs); patch_block(then, defs); if let Some(e) = els { patch_block(e, defs); } }
        Stmt::While { cond, body, .. } => { patch_expr(cond, defs); patch_block(body, defs); }
        Stmt::DoWhile { body, cond, .. } => { patch_block(body, defs); patch_expr(cond, defs); }
        Stmt::ForRange { from, to, body, els, .. } => { patch_expr(from, defs); patch_expr(to, defs); patch_block(body, defs); if let Some(e) = els { patch_block(e, defs); } }
        Stmt::ForEach { iter, body, els, .. } => { patch_expr(iter, defs); patch_block(body, defs); if let Some(e) = els { patch_block(e, defs); } }
        Stmt::Block(inner) => patch_block(inner, defs),
        Stmt::Throw(e, _) => patch_expr(e, defs),
        Stmt::Try { body, catches, fin, .. } => {
            patch_block(body, defs);
            for ca in catches { patch_block(&mut ca.body, defs); }
            if let Some(f) = fin { patch_block(f, defs); }
        }
        _ => {}
    }
}

fn patch_expr(e: &mut Expr, defs: &std::collections::HashMap<String, Vec<Option<Expr>>>) {
    // 先递归子表达式
    match &mut e.kind {
        ExprKind::Unary(_, a) => patch_expr(a, defs),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => { patch_expr(a, defs); patch_expr(b, defs); }
        ExprKind::Call(_, args) => for a in args { patch_expr(a, defs); },
        ExprKind::CallValue { callee, args } => { patch_expr(callee, defs); for a in args { patch_expr(a, defs); } }
        ExprKind::ArrayLit(xs) => for x in xs { patch_expr(x, defs); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { patch_expr(i, defs); } },
        ExprKind::Field(base, _) => patch_expr(base, defs),
        ExprKind::StructLit(_, fs) => for (_, v) in fs { patch_expr(v, defs); },
        ExprKind::If { cond, then, els } => { patch_expr(cond, defs); patch_block(then, defs); if let Some(x) = els { patch_block(x, defs); } }
        ExprKind::Match { subject, arms } => {
            patch_expr(subject, defs);
            for arm in arms {
                if let Some(p) = &mut arm.pat { patch_expr(p, defs); }
                if let Some((lo, hi)) = &mut arm.range { patch_expr(lo, defs); patch_expr(hi, defs); }
                if let Some(g) = &mut arm.guard { patch_expr(g, defs); }
                patch_block(&mut arm.body, defs);
            }
        }
        _ => {}
    }
    // 补默认值
    if let ExprKind::Call(name, args) = &mut e.kind {
        if let Some(defaults) = defs.get(name) {
            if args.len() < defaults.len() {
                for d in &defaults[args.len()..] {
                    if let Some(dv) = d { args.push(dv.clone()); }
                }
            }
        }
    }
}

/// 兼容入口：两段一起。
pub fn optimize(prog: &mut Program) {
    inline_and_fold(prog);
    dead_code(prog);
}

// ============================================================
// 内联
// ============================================================

fn inline_simple(prog: &mut Program) {
    use std::collections::HashMap;
    for iter in 0..8 {
        let mut cands: HashMap<String, (Vec<String>, Block)> = HashMap::new();
        for item in &prog.items {
            if let Item::Fn(f) = item {
                if !f.type_params.is_empty() || f.is_pub { continue; }
                if f.body.is_empty() || f.body.len() > 6 { continue; }
                if !matches!(f.body.last(), Some(Stmt::Expr(_))) { continue; }
                let names: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
                cands.insert(f.name.clone(), (names, f.body.clone()));
            }
        }
        if cands.is_empty() { break; }
        let mut changed = false;
        for item in prog.items.iter_mut() {
            match item {
                Item::Fn(f) => changed |= inline_block(&mut f.body, &cands, iter),
                Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                    for m in methods.iter_mut() { changed |= inline_block(&mut m.body, &cands, iter); }
                }
                _ => {}
            }
        }
        if !changed { break; }
    }
}

fn inline_block(b: &mut Block, cands: &std::collections::HashMap<String, (Vec<String>, Block)>, iter: usize) -> bool {
    let mut changed = false;
    for s in b.iter_mut() {
        match s {
            Stmt::Let { value, .. } | Stmt::Const { value, .. } => changed |= inline_expr(value, cands, iter),
            Stmt::Assign { value, .. } | Stmt::FieldAssign { value, .. } => changed |= inline_expr(value, cands, iter),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => changed |= inline_expr(e, cands, iter),
            Stmt::If { cond, then, els, .. } => { changed |= inline_expr(cond, cands, iter); changed |= inline_block(then, cands, iter); if let Some(e) = els { changed |= inline_block(e, cands, iter); } }
            Stmt::While { cond, body, .. } => { changed |= inline_expr(cond, cands, iter); changed |= inline_block(body, cands, iter); }
            Stmt::ForRange { from, to, body, .. } => { changed |= inline_expr(from, cands, iter); changed |= inline_expr(to, cands, iter); changed |= inline_block(body, cands, iter); }
            Stmt::ForEach { iter: it, body, .. } => { changed |= inline_expr(it, cands, iter); changed |= inline_block(body, cands, iter); }
            Stmt::Block(inner) => changed |= inline_block(inner, cands, iter),
            Stmt::Throw(e, _) => changed |= inline_expr(e, cands, iter),
            _ => {}
        }
    }
    changed
}

fn inline_expr(e: &mut Expr, cands: &std::collections::HashMap<String, (Vec<String>, Block)>, iter: usize) -> bool {
    let mut changed = false;
    match &mut e.kind {
        ExprKind::Unary(_, a) => changed |= inline_expr(a, cands, iter),
        ExprKind::Binary(_, a, b) => { changed |= inline_expr(a, cands, iter); changed |= inline_expr(b, cands, iter); }
        ExprKind::Call(_, args) => for a in args { changed |= inline_expr(a, cands, iter); },
        ExprKind::CallValue { callee, args } => { changed |= inline_expr(callee, cands, iter); for a in args { changed |= inline_expr(a, cands, iter); } }
        ExprKind::Index(a, b) => { changed |= inline_expr(a, cands, iter); changed |= inline_expr(b, cands, iter); }
        ExprKind::ArrayLit(xs) => for x in xs { changed |= inline_expr(x, cands, iter); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { changed |= inline_expr(i, cands, iter); } },
        ExprKind::Field(base, _) => changed |= inline_expr(base, cands, iter),
        ExprKind::StructLit(_, fs) => for (_, v) in fs { changed |= inline_expr(v, cands, iter); },
        _ => {}
    }
    if let ExprKind::Call(name, args) = &e.kind {
        if let Some((params, body)) = cands.get(name) {
            if params.len() == args.len() {
                let line = e.line;
                if let Some(ne) = build_inline(params, body, args, iter, line) {
                    e.kind = ne;
                    e.ty = Ty::Unknown;
                    return true;
                }
            }
        }
    }
    changed
}

fn build_inline(params: &[String], body: &Block, args: &[Expr], iter: usize, line: usize) -> Option<ExprKind> {
    let mut locals: std::collections::HashSet<String> = std::collections::HashSet::new();
    collect_locals(body, &mut locals);
    let pfx = format!("__inl{}_", iter);
    let mut rename: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for l in &locals {
        if !params.contains(l) { rename.insert(l.clone(), format!("{}{}", pfx, l)); }
    }
    let mut nb = body.clone();
    let subst: std::collections::HashMap<String, Expr> = params.iter().cloned().zip(args.iter().cloned()).collect();
    rename_block(&mut nb, &rename, &subst)?;
    if nb.len() == 1 {
        if let Stmt::Expr(e) = &nb[0] { return Some(e.kind.clone()); }
    }
    let cond = Expr::new(ExprKind::Bool(true), line);
    Some(ExprKind::If { cond: Box::new(cond), then: nb, els: None })
}

fn collect_locals(b: &Block, out: &mut std::collections::HashSet<String>) {
    for s in b {
        match s {
            Stmt::Let { name, .. } | Stmt::Const { name, .. } => { out.insert(name.clone()); }
            Stmt::ForEach { var, .. } | Stmt::ForRange { var, .. } => { out.insert(var.clone()); }
            _ => {}
        }
        s.each_block(&mut |blk| collect_locals(blk, out));
    }
}

fn rename_block(b: &mut Block, rename: &std::collections::HashMap<String, String>, subst: &std::collections::HashMap<String, Expr>) -> Option<()> {
    for s in b.iter_mut() {
        match s {
            Stmt::Let { name, value, .. } | Stmt::Const { name, value, .. } => {
                rename_expr(value, rename, subst)?;
                if let Some(n) = rename.get(name) { *name = n.clone(); }
            }
            Stmt::Assign { name, index, value, .. } => {
                rename_expr(value, rename, subst)?;
                if let Some(i) = index { rename_expr(i, rename, subst)?; }
                if let Some(n) = rename.get(name) { *name = n.clone(); }
            }
            Stmt::FieldAssign { value, .. } => rename_expr(value, rename, subst)?,
            Stmt::Expr(e) => rename_expr(e, rename, subst)?,
            Stmt::If { cond, then, els, .. } => {
                rename_expr(cond, rename, subst)?;
                rename_block(then, rename, subst)?;
                if let Some(e) = els { rename_block(e, rename, subst)?; }
            }
            Stmt::While { cond, body, .. } => { rename_expr(cond, rename, subst)?; rename_block(body, rename, subst)?; }
            Stmt::ForRange { var, from, to, body, .. } => {
                rename_expr(from, rename, subst)?; rename_expr(to, rename, subst)?;
                rename_block(body, rename, subst)?;
                if let Some(n) = rename.get(var) { *var = n.clone(); }
            }
            Stmt::ForEach { var, iter, body, .. } => {
                rename_expr(iter, rename, subst)?;
                rename_block(body, rename, subst)?;
                if let Some(n) = rename.get(var) { *var = n.clone(); }
            }
            Stmt::Block(inner) => rename_block(inner, rename, subst)?,
            _ => return None,
        }
    }
    Some(())
}

fn rename_expr(e: &mut Expr, rename: &std::collections::HashMap<String, String>, subst: &std::collections::HashMap<String, Expr>) -> Option<()> {
    match &mut e.kind {
        ExprKind::Ident(n) => {
            if let Some(r) = subst.get(n) { e.kind = r.kind.clone(); e.ty = r.ty.clone(); }
            else if let Some(r) = rename.get(n) { *n = r.clone(); }
        }
        ExprKind::Unary(_, a) => rename_expr(a, rename, subst)?,
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => { rename_expr(a, rename, subst)?; rename_expr(b, rename, subst)?; }
        ExprKind::Call(name, args) => {
            // 内联替换：`recv.方法(...)` 的 recv 若在 subst 中，需一并替换；
            // 实参是简单标识符才能安全替换，否则放弃内联（返回 None）。
            if let Some(dot) = name.find('.') {
                let recv = name[..dot].to_string();
                if let Some(rep) = subst.get(&recv) {
                    if let ExprKind::Ident(rn) = &rep.kind {
                        *name = format!("{}{}", rn, &name[dot..]);
                    } else {
                        return None;
                    }
                } else if let Some(r) = rename.get(&recv) {
                    *name = format!("{}{}", r, &name[dot..]);
                }
            }
            for a in args { rename_expr(a, rename, subst)?; }
        }
        ExprKind::CallValue { callee, args } => { rename_expr(callee, rename, subst)?; for a in args { rename_expr(a, rename, subst)?; } }
        ExprKind::ArrayLit(xs) => for x in xs { rename_expr(x, rename, subst)?; },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { rename_expr(i, rename, subst)?; } },
        ExprKind::Field(base, _) => rename_expr(base, rename, subst)?,
        ExprKind::StructLit(_, fs) => for (_, v) in fs { rename_expr(v, rename, subst)?; },
        ExprKind::If { cond, then, els } => { rename_expr(cond, rename, subst)?; rename_block(then, rename, subst)?; if let Some(x) = els { rename_block(x, rename, subst)?; } }
        ExprKind::Match { subject, arms } => {
            rename_expr(subject, rename, subst)?;
            for arm in arms {
                if let Some(p) = &mut arm.pat { rename_expr(p, rename, subst)?; }
                if let Some(g) = &mut arm.guard { rename_expr(g, rename, subst)?; }
                rename_block(&mut arm.body, rename, subst)?;
            }
        }
        ExprKind::ClosureNew { captures, .. } => for c in captures { rename_expr(c, rename, subst)?; },
        ExprKind::Closure { body, .. } => rename_expr(body, rename, subst)?,
        _ => {}
    }
    Some(())
}

// ============================================================
// 常量折叠
// ============================================================

fn optimize_block(b: &mut Block) {
    for s in b.iter_mut() {
        match s {
            Stmt::Let { value, .. } | Stmt::Const { value, .. } => fold(value),
            Stmt::Assign { value, .. } | Stmt::FieldAssign { value, .. } => fold(value),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => fold(e),
            Stmt::If { cond, then, els, .. } => { fold(cond); optimize_block(then); if let Some(e) = els { optimize_block(e); } }
            Stmt::While { cond, body, .. } => { fold(cond); optimize_block(body); }
            Stmt::ForRange { from, to, body, .. } => { fold(from); fold(to); optimize_block(body); }
            Stmt::ForEach { iter, body, .. } => { fold(iter); optimize_block(body); }
            Stmt::Block(inner) => optimize_block(inner),
            Stmt::Throw(e, _) => fold(e),
            _ => {}
        }
    }
}

fn fold(e: &mut Expr) {
    match &mut e.kind {
        ExprKind::Unary(_, a) => fold(a),
        ExprKind::Binary(_, a, b) => { fold(a); fold(b); }
        ExprKind::Call(_, args) => for a in args { fold(a); },
        ExprKind::Index(a, b) => { fold(a); fold(b); }
        ExprKind::ArrayLit(xs) => for x in xs { fold(x); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { fold(i); } },
        ExprKind::If { cond, then, els } => { fold(cond); optimize_block(then); if let Some(x) = els { optimize_block(x); } }
        ExprKind::Field(base, _) => fold(base),
        ExprKind::StructLit(_, fs) => for (_, v) in fs { fold(v); },
        _ => {}
    }
    if let Some(v) = const_fold_expr(e) { e.kind = v; }
}

fn const_fold_expr(e: &Expr) -> Option<ExprKind> {
    match &e.kind {
        ExprKind::Binary(op, a, b) => {
            let (ta, va) = lit_of(a)?;
            let (tb, vb) = lit_of(b)?;
            let r = fold_binop(*op, &ta, &va, &tb, &vb)?;
            Some(lit_expr(r, e.line).kind)
        }
        ExprKind::Unary(op, a) => {
            let (_t, v) = lit_of(a)?;
            match (op, &v) {
                (UnOp::Neg, LitVal::Int(i)) => Some(lit_expr(LitVal::Int(-i), e.line).kind),
                (UnOp::Neg, LitVal::Float(f)) => Some(lit_expr(LitVal::Float(-f), e.line).kind),
                (UnOp::Not, LitVal::Bool(b)) => Some(lit_expr(LitVal::Bool(!b), e.line).kind),
                (UnOp::BitNot, LitVal::Int(i)) => Some(lit_expr(LitVal::Int(!i), e.line).kind),
                _ => None,
            }
        }
        _ => None,
    }
}

enum LitVal { Int(i64), Float(f64), Bool(bool), Str(String) }

fn lit_of(e: &Expr) -> Option<(Ty, LitVal)> {
    match &e.kind {
        ExprKind::Int(v) => Some((Ty::I64, LitVal::Int(*v))),
        ExprKind::Float(v) => Some((Ty::F64, LitVal::Float(*v))),
        ExprKind::Bool(v) => Some((Ty::Bool, LitVal::Bool(*v))),
        ExprKind::Str(s) => Some((Ty::Str, LitVal::Str(s.clone()))),
        _ => None,
    }
}

fn lit_expr(v: LitVal, line: usize) -> Expr {
    let kind = match v {
        LitVal::Int(i) => ExprKind::Int(i),
        LitVal::Float(f) => ExprKind::Float(f),
        LitVal::Bool(b) => ExprKind::Bool(b),
        LitVal::Str(s) => ExprKind::Str(s),
    };
    Expr::new(kind, line)
}

fn fold_binop(op: BinOp, ta: &Ty, a: &LitVal, tb: &Ty, b: &LitVal) -> Option<LitVal> {
    use BinOp::*;
    if op == Add {
        if let (LitVal::Str(x), LitVal::Str(y)) = (a, b) { return Some(LitVal::Str(format!("{}{}", x, y))); }
    }
    if let (LitVal::Int(x), LitVal::Int(y)) = (a, b) {
        let (x, y) = (*x, *y);
        return Some(match op {
            // 溢出则不折叠（保留运行时检查，避免绕过安全检查）
            Add => LitVal::Int(x.checked_add(y)?),
            Sub => LitVal::Int(x.checked_sub(y)?),
            Mul => LitVal::Int(x.checked_mul(y)?),
            Div | FloorDiv if y != 0 => LitVal::Int(x.wrapping_div(y)),
            Rem if y != 0 => LitVal::Int(x.wrapping_rem(y)),
            Eq => LitVal::Bool(x == y),
            Ne => LitVal::Bool(x != y),
            Lt => LitVal::Bool(x < y),
            Le => LitVal::Bool(x <= y),
            Gt => LitVal::Bool(x > y),
            Ge => LitVal::Bool(x >= y),
            BitAnd => LitVal::Int(x & y),
            BitOr => LitVal::Int(x | y),
            BitXor => LitVal::Int(x ^ y),
            Shl => LitVal::Int(x.wrapping_shl(y as u32)),
            Shr => LitVal::Int(x.wrapping_shr(y as u32)),
            _ => return None,
        });
    }
    let fx = as_f(a);
    let fy = as_f(b);
    if let (Some(x), Some(y)) = (fx, fy) {
        if ta == &Ty::F64 || tb == &Ty::F64 {
            return Some(match op {
                Add => LitVal::Float(x + y),
                Sub => LitVal::Float(x - y),
                Mul => LitVal::Float(x * y),
                Div => LitVal::Float(x / y),
                FloorDiv => LitVal::Float((x / y).floor()),
                Rem => LitVal::Float(x % y),
                Eq => LitVal::Bool(x == y),
                Ne => LitVal::Bool(x != y),
                Lt => LitVal::Bool(x < y),
                Le => LitVal::Bool(x <= y),
                Gt => LitVal::Bool(x > y),
                Ge => LitVal::Bool(x >= y),
                _ => return None,
            });
        }
    }
    if let (LitVal::Bool(x), LitVal::Bool(y)) = (a, b) {
        return Some(match op {
            And => LitVal::Bool(*x && *y),
            Or => LitVal::Bool(*x || *y),
            Eq => LitVal::Bool(x == y),
            Ne => LitVal::Bool(x != y),
            _ => return None,
        });
    }
    None
}

fn as_f(v: &LitVal) -> Option<f64> {
    match v {
        LitVal::Int(i) => Some(*i as f64),
        LitVal::Float(f) => Some(*f),
        _ => None,
    }
}

// ============================================================
// 死代码消除
// ============================================================

fn dead_fn_elim(prog: &mut Program) {
    use std::collections::HashSet;
    if !prog.cblock.trim().is_empty() { return; }
    let mut called: HashSet<String> = HashSet::new();
    for item in &prog.items {
        match item {
            Item::Fn(f) => collect_calls_block(&f.body, &mut called),
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods { collect_calls_block(&m.body, &mut called); }
            }
            _ => {}
        }
    }
    prog.items.retain(|item| match item {
        Item::Fn(f) => {
            f.is_pub
                || f.name == "main"
                || called.contains(&f.name)
                || f.name.contains("__")
                || f.name.starts_with("gt_")
        }
        _ => true,
    });
}

fn collect_calls_block(b: &Block, out: &mut std::collections::HashSet<String>) {
    for s in b {
        match s {
            Stmt::Let { value, .. } | Stmt::Const { value, .. } => collect_calls(value, out),
            Stmt::Assign { value, .. } | Stmt::FieldAssign { value, .. } => collect_calls(value, out),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => collect_calls(e, out),
            Stmt::If { cond, .. } => collect_calls(cond, out),
            Stmt::While { cond, .. } => collect_calls(cond, out),
            Stmt::DoWhile { cond, .. } => collect_calls(cond, out),
            Stmt::ForRange { from, to, .. } => { collect_calls(from, out); collect_calls(to, out); }
            Stmt::ForEach { iter, .. } => collect_calls(iter, out),
            // go 引用函数名，视为"被调用"（避免死代码消除误删）
            Stmt::Go { func, args, .. } => { out.insert(func.clone()); for a in args { collect_calls(a, out); } }
            Stmt::Throw(e, _) => collect_calls(e, out),
            _ => {}
        }
        // 遍历驱动：所有含块变体（含 ForRange/ForEach 的 els、Labeled、LocalFn）统一走 each_block
        s.each_block(&mut |blk| collect_calls_block(blk, out));
    }
}

fn collect_calls(e: &Expr, out: &mut std::collections::HashSet<String>) {
    match &e.kind {
        ExprKind::Call(name, args) => {
            out.insert(name.clone());
            for a in args { collect_calls(a, out); }
            // serve_fn(port, handler)：第二参数是函数名（字符串字面量式引用），
            // 视为"被调用"，避免死代码消除误删处理函数。
            if name == "serve_fn" {
                if let Some(Expr { kind: ExprKind::Ident(fname), .. }) = args.get(1) {
                    out.insert(fname.clone());
                }
            }
        }
        ExprKind::CallValue { callee, args } => { collect_calls(callee, out); for a in args { collect_calls(a, out); } }
        ExprKind::Unary(_, a) => collect_calls(a, out),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => { collect_calls(a, out); collect_calls(b, out); }
        ExprKind::ArrayLit(xs) => for x in xs { collect_calls(x, out); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { collect_calls(i, out); } },
        ExprKind::Field(base, _) => collect_calls(base, out),
        ExprKind::StructLit(_, fs) => for (_, v) in fs { collect_calls(v, out); },
        ExprKind::If { cond, then, els } => {
            collect_calls(cond, out); collect_calls_block(then, out);
            if let Some(x) = els { collect_calls_block(x, out); }
        }
        ExprKind::Match { subject, arms } => {
            collect_calls(subject, out);
            for arm in arms {
                if let Some(p) = &arm.pat { collect_calls(p, out); }
                if let Some(g) = &arm.guard { collect_calls(g, out); }
                collect_calls_block(&arm.body, out);
            }
        }
        ExprKind::Try(inner) => collect_calls(inner, out),
        ExprKind::Ok(inner) | ExprKind::Err(inner) | ExprKind::Some(inner) => collect_calls(inner, out),
        ExprKind::Closure { body, .. } => collect_calls(body, out),
        ExprKind::ClosureNew { fn_name, captures } => {
            // 闭包体函数名（含"函数作一等值"的无捕获闭包）视为被引用，避免误删
            out.insert(fn_name.clone());
            for c in captures { collect_calls(c, out); }
        }
        ExprKind::TryBlock { body, catches, fin } => {
            collect_calls_block(body, out);
            for ca in catches { collect_calls_block(&ca.body, out); }
            if let Some(f) = fin { collect_calls_block(f, out); }
        }
        // 以下分支此前遗漏，会导致死代码消除误删"仅出现在这些位置的函数"。
        ExprKind::MethodOn { recv, args, .. } => { collect_calls(recv, out); for a in args { collect_calls(a, out); } }
        ExprKind::Borrow { inner, .. } => collect_calls(inner, out),
        ExprKind::DynBox { value, .. } => collect_calls(value, out),
        ExprKind::TupleLit(xs) => for x in xs { collect_calls(x, out); },
        ExprKind::Slice(a, b, c) => { collect_calls(a, out); collect_calls(b, out); collect_calls(c, out); }
        ExprKind::ListComp { expr, iter, cond, .. } => { collect_calls(expr, out); collect_calls(iter, out); if let Some(c) = cond { collect_calls(c, out); } }
        ExprKind::EnumLit(_, _, args) => for a in args { collect_calls(a, out); },
        _ => {}
    }
}


#[path = "opt_lower.rs"]
mod opt_lower;
pub(crate) use opt_lower::*;

#[path = "opt_tests.rs"]
#[cfg(test)]
mod opt_tests;

