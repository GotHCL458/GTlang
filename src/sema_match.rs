//! sema 的 match 穷尽性检查。
//!
//! 从 sema.rs 拆出（原文件超 50KB）。

use super::*;

/// match 穷尽性检查：返回 None 表示穷尽，Some(消息) 表示不穷尽。
/// 规则：有 _ 通配 → 穷尽；bool 需 true+false；enum 需全部变体；
///       Option 需 Some+None；Result 需 Ok+Err；整数/字符串 → 必须有 _（否则不穷尽）。
pub(crate) fn check_match_exhaustive(st: &Ty, arms: &[MatchArm], ctx: &Ctx) -> Option<String> {
    let mut has_wild = false;
    let mut enum_vars: Vec<String> = Vec::new();
    let mut has_true = false;
    let mut has_false = false;
    let mut has_some = false;
    let mut has_none = false;
    let mut has_ok = false;
    let mut has_err = false;
    let mut has_value = false; // 整数/字符串字面量
    for arm in arms {
        // 带守卫的 arm 不算穷尽（可能不匹配）
        if arm.guard.is_some() { continue; }
        if arm.pat.is_none() { has_wild = true; continue; }
        let p = arm.pat.as_ref().unwrap();
        match &p.kind {
            ExprKind::EnumLit(en, var, _) => {
                if en == "None" { has_none = true; }
                else if en == "Some" { has_some = true; }
                else if en == "Ok" { has_ok = true; }
                else if en == "Err" { has_err = true; }
                else { enum_vars.push(format!("{}::{}", en, var)); }
            }
            // `Some(v)` / `Ok(v)` / `Err(e)` 在 parser 中是 Call
            ExprKind::Call(n, _) => match n.as_str() {
                "Some" => has_some = true,
                "None" => has_none = true,
                "Ok" => has_ok = true,
                "Err" => has_err = true,
                _ => {}
            },
            ExprKind::Some(_) => has_some = true,
            ExprKind::None => has_none = true,
            ExprKind::Ok(_) => has_ok = true,
            ExprKind::Err(_) => has_err = true,
            ExprKind::Bool(b) => { if *b { has_true = true; } else { has_false = true; } }
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Str(_) => has_value = true,
            _ => {}
        }
    }
    if has_wild { return None; }
    let zh = crate::lang::is_zh();
    match st {
        Ty::Bool => {
            if has_true && has_false { None } else { Some(if zh { "match 不穷尽：bool 需覆盖 true 和 false".into() } else { "match not exhaustive: bool needs true and false".into() }) }
        }
        // 注意：类型系统用 Ty::Struct 表示具名类型（含 enum），所以这里两者都查 ctx.enums。
        Ty::Enum(name) | Ty::Struct(name) => {
            if let Some(vs) = ctx.enums.get(name) {
                let all: Vec<String> = vs.iter().map(|(n, _)| n.clone()).collect();
                let missing: Vec<String> = all.iter().filter(|v| !enum_vars.iter().any(|ev| ev.ends_with(&format!("::{}", v)))).cloned().collect();
                if missing.is_empty() { None } else { Some(if zh { format!("match 不穷尽：缺少变体 {}（或用 _ 兜底）", missing.join("、")) } else { format!("match not exhaustive: missing {}", missing.join(", ")) }) }
            } else { None }
        }
        Ty::Option(_) => {
            if has_some && has_none { None } else { Some(if zh { "match 不穷尽：Option 需覆盖 Some 和 None（或用 _ 兜底）".into() } else { "match not exhaustive: Option needs Some and None".into() }) }
        }
        Ty::Result(_, _) => {
            if has_ok && has_err { None } else { Some(if zh { "match 不穷尽：Result 需覆盖 Ok 和 Err（或用 _ 兜底）".into() } else { "match not exhaustive: Result needs Ok and Err".into() }) }
        }
        Ty::I64 | Ty::F64 | Ty::Str => {
            // 整数/浮点/字符串是"无限"主体：字面量分支永远无法穷尽，
            // 必须显式 _ 兜底（空 match 同样不穷尽）。
            let _ = has_value;
            Some(if zh { "match 不穷尽：非枚举主体需用 _ 兜底".into() } else { "match not exhaustive: non-enum subject needs _".into() })
        }
        _ => None,
    }
}

