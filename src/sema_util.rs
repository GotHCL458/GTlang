//! sema 的通用工具：常量数值转换、AST 遍历、类型 join、名字建议。
//!
//! 从 sema.rs 拆出（原文件超 50KB）。

use super::*;

pub(crate) fn as_num(v: &Value) -> Result<f64, String> {
    match v {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        _ => Err("不是数值".into()),
    }
}

// ============================================================
// AST 遍历
// ============================================================

pub(crate) fn each_expr_block(b: &Block, f: &mut impl FnMut(&Expr)) {
    for s in b {
        each_expr_stmt(s, f);
    }
}

pub(crate) fn each_expr_stmt(s: &Stmt, f: &mut impl FnMut(&Expr)) {
    match s {
        Stmt::Let { value, .. } => each_expr(value, f),
        Stmt::Assign { index, value, .. } => {
            if let Some(i) = index {
                each_expr(i, f);
            }
            each_expr(value, f);
        }
        Stmt::Expr(e) => each_expr(e, f),
        Stmt::If { cond, then, els, .. } => {
            each_expr(cond, f);
            each_expr_block(then, f);
            if let Some(e) = els {
                each_expr_block(e, f);
            }
        }
        Stmt::While { cond, body, .. } => {
            each_expr(cond, f);
            each_expr_block(body, f);
        }
        Stmt::DoWhile { body, cond, .. } => {
            each_expr_block(body, f);
            each_expr(cond, f);
        }
        Stmt::ForRange { from, to, body, .. } => {
            each_expr(from, f);
            each_expr(to, f);
            each_expr_block(body, f);
        }
        Stmt::ForEach { iter, body, .. } => {
            each_expr(iter, f);
            each_expr_block(body, f);
        }
        Stmt::Return(Some(e), _) => each_expr(e, f),
        Stmt::Block(b) => each_expr_block(b, f),
        Stmt::LocalFn(_) => {}
        _ => {}
    }
}

pub(crate) fn each_expr(e: &Expr, f: &mut impl FnMut(&Expr)) {
    f(e);
    match &e.kind {
        ExprKind::Unary(_, a) => each_expr(a, f),
        ExprKind::Binary(_, a, b) => {
            each_expr(a, f);
            each_expr(b, f);
        }
        ExprKind::Call(_, args) => {
            for a in args {
                each_expr(a, f);
            }
        }
        ExprKind::CallValue { callee, args } => {
            each_expr(callee, f);
            for a in args {
                each_expr(a, f);
            }
        }
        ExprKind::Index(a, b) => {
            each_expr(a, f);
            each_expr(b, f);
        }
        ExprKind::ArrayLit(items) => {
            for a in items {
                each_expr(a, f);
            }
        }
        ExprKind::Interp(parts) => {
            for p in parts {
                if let StrPart::Expr(inner) = p {
                    each_expr(inner, f);
                }
            }
        }
        ExprKind::If { cond, then, els } => {
            each_expr(cond, f);
            each_expr_block(then, f);
            if let Some(b) = els {
                each_expr_block(b, f);
            }
        }
        ExprKind::CallNamed(_, named) => {
            for (_, a) in named { each_expr(a, f); }
        }
        ExprKind::Slice(a, lo, hi) => {
            each_expr(a, f);
            each_expr(lo, f);
            each_expr(hi, f);
        }
        ExprKind::TupleLit(items) => {
            for a in items { each_expr(a, f); }
        }
        ExprKind::ListComp { expr, iter, cond, .. } => {
            each_expr(expr, f);
            each_expr(iter, f);
            if let Some(c) = cond { each_expr(c, f); }
        }
        ExprKind::Field(a, _) => each_expr(a, f),
        ExprKind::StructLit(_, fields) => {
            for (_, a) in fields { each_expr(a, f); }
        }
        ExprKind::EnumLit(_, _, args) => {
            for a in args { each_expr(a, f); }
        }
        ExprKind::DynBox { value, .. } => each_expr(value, f),
        ExprKind::Match { subject, arms } => {
            each_expr(subject, f);
            for arm in arms {
                if let Some(p) = &arm.pat { each_expr(p, f); }
                if let Some((lo, hi)) = &arm.range { each_expr(lo, f); each_expr(hi, f); }
                if let Some(g) = &arm.guard { each_expr(g, f); }
                for s in &arm.body { each_expr_stmt(s, f); }
            }
        }
        ExprKind::Closure { body, .. } => each_expr(body, f),
        ExprKind::MethodOn { recv, args, .. } => {
            each_expr(recv, f);
            for a in args { each_expr(a, f); }
        }
        ExprKind::ClosureNew { captures, .. } => {
            for c in captures { each_expr(c, f); }
        }
        ExprKind::Borrow { inner, .. } => each_expr(inner, f),
        ExprKind::Ok(a) | ExprKind::Err(a) | ExprKind::Try(a) | ExprKind::Some(a) => each_expr(a, f),
        ExprKind::None => {}
        ExprKind::TryBlock { body, catches, fin } => {
            each_expr_block(body, f);
            for c in catches {
                for s in &c.body { each_expr_stmt(s, f); }
            }
            if let Some(b) = fin { each_expr_block(b, f); }
        }
        _ => {}
    }
}

/// 通用类型 join：相同→同；Unknown→另一个；数值→提升；否则 Unknown。
pub(crate) fn type_join(a: &Ty, b: &Ty) -> Ty {
    if a == b { return a.clone(); }
    if *a == Ty::Unknown { return b.clone(); }
    if *b == Ty::Unknown { return a.clone(); }
    if a.is_num() && b.is_num() { return numeric_join(a, b); }
    // 容器：逐元素 join
    if let (Ty::List(x), Ty::List(y)) = (a, b) { return Ty::List(Box::new(type_join(x, y))); }
    if let (Ty::Set(x), Ty::Set(y)) = (a, b) { return Ty::Set(Box::new(type_join(x, y))); }
    if let (Ty::Option(x), Ty::Option(y)) = (a, b) { return Ty::Option(Box::new(type_join(x, y))); }
    Ty::Unknown
}

/// 找最近的已知变量名（编辑距离 ≤ 2）。
pub(crate) fn closest_name(name: &str, ctx: &Ctx) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for sc in &ctx.scopes {
        for k in sc.keys() {
            if k == name { continue; }
            let d = levenshtein(name, k);
            if d <= 2 && best.as_ref().map_or(true, |(bd, _)| d < *bd) {
                best = Some((d, k.clone()));
            }
        }
    }
    best.map(|(_, k)| k)
}

/// Levenshtein 编辑距离（字符级）。
pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 { return m; }
    if m == 0 { return n; }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = if a[i-1] == b[j-1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j-1] + 1).min(prev[j-1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}

/// 找最近的已知函数名（编辑距离 ≤ 2）。
pub(crate) fn closest_fn(name: &str, ctx: &Ctx) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for k in ctx.fns.keys() {
        if k == name { continue; }
        let d = levenshtein(name, k);
        if d <= 2 && best.as_ref().map_or(true, |(bd, _)| d < *bd) {
            best = Some((d, k.clone()));
        }
    }
    best.map(|(_, k)| k)
}

/// 从候选集中找最近的（编辑距离 ≤ 2）。
pub(crate) fn closest_of<'a, I: Iterator<Item = String>>(name: &str, cands: I) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for k in cands {
        if k == name { continue; }
        let d = levenshtein(name, &k);
        if d <= 2 && best.as_ref().map_or(true, |(bd, _)| d < *bd) {
            best = Some((d, k));
        }
    }
    best.map(|(_, k)| k)
}


/// 公开：候选集中是否有编辑距离 ≤ 2 的名字（供 type.rs 用）。
pub fn closest_name_pub(name: &str, cands: &[String]) -> bool {
    cands.iter().any(|k| k != name && levenshtein(name, k) <= 2)
}

#[cfg(test)]
mod util_tests {
    use super::*;
    use crate::types::Ty;

    #[test]
    fn levenshtein_basics() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("abc", "abd"), 1);
        assert_eq!(levenshtein("abc", "ab"), 1);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        // 中文按字符计
        assert_eq!(levenshtein("累加", "累减"), 1);
        assert_eq!(levenshtein("计数器", "计数"), 1);
    }

    #[test]
    fn closest_of_picks_nearest() {
        let cands = vec!["upper".to_string(), "lower".to_string(), "trim".to_string()];
        assert_eq!(closest_of("uppr", cands.clone().into_iter()), Some("upper".to_string()));
        assert_eq!(closest_of("lowr", cands.clone().into_iter()), Some("lower".to_string()));
        // 距离都 > 2 → None
        assert_eq!(closest_of("xyz", cands.clone().into_iter()), None);
        // 完全相等时跳过自己
        assert_eq!(closest_of("upper", cands.into_iter()), None);
    }

    #[test]
    fn closest_name_pub_checks_distance() {
        let cands = vec!["hello".to_string(), "world".to_string()];
        assert!(closest_name_pub("helo", &cands));
        assert!(!closest_name_pub("xxxxx", &cands));
        // 与自身相等不算
        assert!(!closest_name_pub("hello", &cands));
    }

    #[test]
    fn type_join_unknown_and_same() {
        assert_eq!(type_join(&Ty::I64, &Ty::I64), Ty::I64);
        assert_eq!(type_join(&Ty::Unknown, &Ty::Str), Ty::Str);
        assert_eq!(type_join(&Ty::Str, &Ty::Unknown), Ty::Str);
        assert_eq!(type_join(&Ty::Unknown, &Ty::Unknown), Ty::Unknown);
    }

    #[test]
    fn type_join_numeric_and_containers() {
        assert_eq!(type_join(&Ty::I64, &Ty::F64), Ty::F64);
        assert_eq!(type_join(&Ty::List(Box::new(Ty::I64)), &Ty::List(Box::new(Ty::F64))), Ty::List(Box::new(Ty::F64)));
        assert_eq!(type_join(&Ty::Set(Box::new(Ty::Unknown)), &Ty::Set(Box::new(Ty::I64))), Ty::Set(Box::new(Ty::I64)));
        assert_eq!(type_join(&Ty::Option(Box::new(Ty::I64)), &Ty::Option(Box::new(Ty::F64))), Ty::Option(Box::new(Ty::F64)));
        // 不同类型 → Unknown
        assert_eq!(type_join(&Ty::I64, &Ty::Str), Ty::Unknown);
        assert_eq!(type_join(&Ty::List(Box::new(Ty::I64)), &Ty::Set(Box::new(Ty::I64))), Ty::Unknown);
    }
}

