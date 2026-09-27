//! 整数范围分析：为 codegen 判定 Add/Sub/Mul 是否可静态证明不溢出。
//!
//! 安全第一：默认无界 [i64::MIN, i64::MAX]；只有能证明的运算才标记安全。
//! 流敏感（作用域 + 赋值），循环用保守固定点。

use crate::ast::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug)]
pub struct Range { pub lo: i64, pub hi: i64 }

impl Range {
    pub fn unknown() -> Range { Range { lo: i64::MIN, hi: i64::MAX } }
    pub fn exact(v: i64) -> Range { Range { lo: v, hi: v } }
    /// 是否为"有限已知区间"。[MIN,MAX] 视为无界（未知）。
    pub fn is_known(&self) -> bool { self.lo <= self.hi && !(self.lo == i64::MIN && self.hi == i64::MAX) }
    fn join(&self, o: &Range) -> Range { Range { lo: self.lo.min(o.lo), hi: self.hi.max(o.hi) } }
    fn add(&self, o: &Range) -> Range {
        match (self.lo.checked_add(o.lo), self.hi.checked_add(o.hi)) {
            (Some(l), Some(h)) => Range { lo: l, hi: h },
            _ => Range::unknown(),
        }
    }
    fn sub(&self, o: &Range) -> Range {
        match (self.lo.checked_sub(o.hi), self.hi.checked_sub(o.lo)) {
            (Some(l), Some(h)) => Range { lo: l, hi: h },
            _ => Range::unknown(),
        }
    }
    fn mul(&self, o: &Range) -> Range {
        let cands = [self.lo.checked_mul(o.lo), self.lo.checked_mul(o.hi), self.hi.checked_mul(o.lo), self.hi.checked_mul(o.hi)];
        let (mut lo, mut hi) = (i64::MAX, i64::MIN);
        for c in cands { match c { Some(v) => { lo = lo.min(v); hi = hi.max(v); } None => return Range::unknown() } }
        Range { lo, hi }
    }
}

/// 键用 (行号, 表达式地址)，codegen 与 range 遍历同一 AST，地址一致。
pub struct Analysis { safe: HashSet<usize>, safe_stmt: HashSet<usize> }
impl Analysis {
    pub fn is_safe(&self, e: &Expr) -> bool { self.safe.contains(&(e as *const Expr as usize)) }
    pub fn is_stmt_safe(&self, s: &Stmt) -> bool { self.safe_stmt.contains(&(s as *const Stmt as usize)) }
}

struct Ctx { scopes: Vec<HashMap<String, Range>>, safe: HashSet<usize>, safe_stmt: HashSet<usize> }

impl Ctx {
    fn lookup(&self, n: &str) -> Range {
        for s in self.scopes.iter().rev() { if let Some(r) = s.get(n) { return *r; } }
        Range::unknown()
    }
    fn set(&mut self, n: &str, r: Range) { if let Some(s) = self.scopes.last_mut() { s.insert(n.to_string(), r); } }
}

pub fn analyze(prog: &Program) -> Analysis {
    let mut ctx = Ctx { scopes: vec![HashMap::new()], safe: HashSet::new(), safe_stmt: HashSet::new() };
    for item in &prog.items {
        match item {
            Item::Fn(f) => { ctx.scopes = vec![HashMap::new()]; analyze_block(&mut ctx, &f.body); }
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods { ctx.scopes = vec![HashMap::new()]; analyze_block(&mut ctx, &m.body); }
            }
            _ => {}
        }
    }
    Analysis { safe: ctx.safe, safe_stmt: ctx.safe_stmt }
}

fn analyze_block(ctx: &mut Ctx, b: &Block) { for s in b { analyze_stmt(ctx, s); } }

fn analyze_stmt(ctx: &mut Ctx, s: &Stmt) {
    match s {
        Stmt::Let { name, value, .. } | Stmt::Const { name, value, .. } => { let r = range_of(ctx, value); ctx.set(name, r); }
        Stmt::Assign { name, value, index, op, .. } => {
            let rv = range_of(ctx, value);
            if index.is_none() {
                let r = match op {
                    None => rv,
                    Some(o) => {
                        let cur = ctx.lookup(name);
                        let res = match o {
                            BinOp::Add => cur.add(&rv),
                            BinOp::Sub => cur.sub(&rv),
                            BinOp::Mul => cur.mul(&rv),
                            _ => Range::unknown(),
                        };
                        if matches!(o, BinOp::Add | BinOp::Sub | BinOp::Mul) && res.is_known() {
                            ctx.safe_stmt.insert(s as *const Stmt as usize);
                        }
                        res
                    }
                };
                ctx.set(name, r);
            }
        }
        Stmt::FieldAssign { .. } => {}
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => { let _ = range_of(ctx, e); }
        Stmt::If { cond, then, els, .. } => {
            let _ = range_of(ctx, cond);
            let mut t = ctx.scopes.clone();
            let mut e2 = ctx.scopes.clone();
            { let saved = std::mem::replace(&mut ctx.scopes, t); analyze_block(ctx, then); t = std::mem::replace(&mut ctx.scopes, saved); }
            if let Some(els) = els { let saved = std::mem::replace(&mut ctx.scopes, e2); analyze_block(ctx, els); e2 = std::mem::replace(&mut ctx.scopes, saved); }
            let n = t.len().min(e2.len());
            for i in 0..n {
                let merged: Vec<(String, Range)> = t[i].iter()
                    .filter_map(|(k, vr)| e2[i].get(k).map(|er| (k.clone(), vr.join(er))))
                    .collect();
                for (k, j) in merged { t[i].insert(k, j); }
            }
            ctx.scopes = t;
        }
        Stmt::While { cond, body, .. } => { let _ = range_of(ctx, cond); analyze_loop(ctx, body); }
        Stmt::ForRange { var, from, to, body, .. } => {
            let fr = range_of(ctx, from);
            let tr = range_of(ctx, to);
            ctx.scopes.push(HashMap::new());
            if fr.is_known() && tr.is_known() && fr.lo == fr.hi && tr.lo == tr.hi {
                let hi = tr.lo.saturating_sub(1);
                if fr.lo <= hi { ctx.set(var, Range { lo: fr.lo, hi }); }
            }
            analyze_loop(ctx, body);
            ctx.scopes.pop();
        }
        Stmt::ForEach { body, .. } => { analyze_loop(ctx, body); }
        Stmt::Block(inner) => { ctx.scopes.push(HashMap::new()); analyze_block(ctx, inner); ctx.scopes.pop(); }
        Stmt::Throw(e, _) => { let _ = range_of(ctx, e); }
        Stmt::Try { body, catches, fin, .. } => {
            analyze_block(ctx, body);
            for ca in catches { analyze_block(ctx, &ca.body); }
            if let Some(f) = fin { analyze_block(ctx, f); }
        }
        _ => {}
    }
}

fn analyze_loop(ctx: &mut Ctx, body: &Block) {
    let mut modified: HashSet<String> = HashSet::new();
    collect_modified(body, &mut modified);
    // 不从 unknown 起：保留进入循环时的值（如 `s := 0` 的 [0,0]），
    // 迭代中通过 join 逐轮扩大，收敛到不动点。
    let mut stable = false;
    let mut iter = 0;
    while !stable && iter < 8 {
        iter += 1;
        let before: Vec<(String, Range)> = modified.iter().map(|m| (m.clone(), ctx.lookup(m))).collect();
        analyze_block(ctx, body);
        stable = true;
        for (m, old) in &before {
            let new = ctx.lookup(m);
            let joined = old.join(&new);
            if joined.lo != old.lo || joined.hi != old.hi { stable = false; ctx.set(m, joined); }
        }
    }
}

fn collect_modified(b: &Block, out: &mut HashSet<String>) {
    for s in b {
        match s {
            Stmt::Assign { name, .. } => { out.insert(name.clone()); }
            Stmt::If { then, els, .. } => { collect_modified(then, out); if let Some(e) = els { collect_modified(e, out); } }
            Stmt::While { body, .. } | Stmt::ForRange { body, .. } | Stmt::ForEach { body, .. } => collect_modified(body, out),
            Stmt::Block(inner) => collect_modified(inner, out),
            _ => {}
        }
    }
}

fn range_of(ctx: &mut Ctx, e: &Expr) -> Range {
    match &e.kind {
        ExprKind::Int(v) => Range::exact(*v),
        ExprKind::Bool(_) => Range::exact(0),
        ExprKind::Ident(n) => ctx.lookup(n),
        ExprKind::Unary(UnOp::Neg, a) => {
            let r = range_of(ctx, a);
            if r.is_known() { match (r.hi.checked_neg(), r.lo.checked_neg()) { (Some(l), Some(h)) => Range { lo: l, hi: h }, _ => Range::unknown() } } else { Range::unknown() }
        }
        ExprKind::Unary(_, a) => { let _ = range_of(ctx, a); Range::unknown() }
        ExprKind::Binary(op, a, b) => {
            let ra = range_of(ctx, a);
            let rb = range_of(ctx, b);
            let res = match op { BinOp::Add => ra.add(&rb), BinOp::Sub => ra.sub(&rb), BinOp::Mul => ra.mul(&rb), _ => Range::unknown() };
            if matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) && res.is_known() { ctx.safe.insert(e as *const Expr as usize); }
            res
        }
        ExprKind::Call(_, args) | ExprKind::ArrayLit(args) => { for a in args { let _ = range_of(ctx, a); } Range::unknown() }
        ExprKind::Index(a, b) => { let _ = range_of(ctx, a); let _ = range_of(ctx, b); Range::unknown() }
        ExprKind::Interp(parts) => { for p in parts { if let StrPart::Expr(i) = p { let _ = range_of(ctx, i); } } Range::unknown() }
        ExprKind::Field(base, _) => { let _ = range_of(ctx, base); Range::unknown() }
        ExprKind::StructLit(_, fs) => { for (_, v) in fs { let _ = range_of(ctx, v); } Range::unknown() }
        _ => Range::unknown(),
    }
}
