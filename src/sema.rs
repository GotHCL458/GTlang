//! 语义分析：常量求值、无标注形参推断、表达式类型推断与检查。
//!
//! 推断结果直接回写到 AST（`Expr::ty`、`Param::ty`、`FnDef::ret_ty`），
//! 使代码生成阶段只有一个类型来源。

use std::collections::HashMap;

use crate::ast::*;

#[path = "sema_const.rs"]
mod sema_const;
#[path = "sema_infer.rs"]
mod sema_infer;
#[path = "sema_match.rs"]
mod sema_match;
#[path = "sema_stmt.rs"]
mod sema_stmt;
#[path = "sema_util.rs"]
mod sema_util;
#[cfg(test)]
#[path = "sema_util_tests.rs"]
mod sema_util_tests;
use sema_const::*;
use sema_match::*;
use sema_stmt::*;
use sema_util::*;
pub use sema_util::closest_name_pub;

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

#[derive(Debug, Clone)]
pub struct ConstVal {
    pub ty: Ty,
    pub val: Value,
}

/// 语义分析产出：常量表（代码生成时内联）
#[derive(Debug, Default)]
pub struct Analysis {
    pub consts: HashMap<String, ConstVal>,
    /// trait 名 → 方法名列表（顺序即 vtable 索引）
    pub traits: HashMap<String, Vec<String>>,
    /// (类型, trait) → 该方法对应的展平名（`类型__方法`），顺序同 trait 方法表
    pub trait_impls: HashMap<(String, String), Vec<String>>,
}

#[derive(Debug, Clone)]
struct FnSig {
    params: Vec<Ty>,
    ret: Ty,
}

#[derive(Clone)]
struct VarInfo { ty: Ty, mutable: bool, explicit: bool }

struct Ctx {
    consts: HashMap<String, ConstVal>,
    fns: HashMap<String, FnSig>,
    scopes: Vec<HashMap<String, VarInfo>>,
    /// 正在推断返回类型的函数集合（防递归无限递归）
    pending_ret: Vec<String>,
    cur_ret: Ty,
    /// 结构体字段表：名字 → [(字段名, 类型)]
    structs: HashMap<String, Vec<(String, Ty)>>,
    /// 枚举变体表：名字 → [(变体名, 载荷类型)]
    enums: HashMap<String, Vec<(String, Vec<Ty>)>>,
    /// 当前循环嵌套深度（用于 break/continue 合法性检查）
    loop_depth: usize,
    /// 当前 try 块嵌套深度：> 0 时 `?`/`throw` 由 try 捕获（不必函数返回 Result）
    try_depth: usize,
    /// trait 名 → [(方法名, 参数类型, 返回类型)]
    trait_methods: HashMap<String, Vec<(String, Vec<Ty>, Ty)>>,
    /// 已导入的内置标准库模块名（未导入则不能调用其函数）
    imported_gtlib: Vec<String>,
    /// 当前函数的泛型约束：类型参数名 → trait 名（供 `x.方法()` 静态分发推断）
    generic_bounds: Vec<(String, String)>,
    /// 当前函数的形参名（无标注形参可能实为闭包/函数，调用时放行）
    param_names: Vec<String>,
    /// 闭包捕获参数类型：提升后的闭包函数名 → 各捕获值的类型
    /// （hoist 生成的捕获形参无标注，这里从调用点的 captures 表达式回填）
    capture_types: HashMap<String, Vec<Ty>>,
}

pub fn analyze(prog: &mut Program) -> Result<Analysis, Vec<String>> {
    let mut ctx = Ctx {
        consts: HashMap::new(),
        fns: HashMap::new(),
        scopes: vec![HashMap::new()],
        pending_ret: Vec::new(),
        cur_ret: Ty::Void,
        structs: HashMap::new(),
        enums: HashMap::new(),
        loop_depth: 0,
        try_depth: 0,
        trait_methods: HashMap::new(),
        imported_gtlib: prog.imported_gtlib.clone(),
        generic_bounds: Vec::new(),
        param_names: Vec::new(),
        capture_types: HashMap::new(),
    };

    let mut errors: Vec<String> = Vec::new();

    // ---------- 0. 收集结构体字段（含泛型；泛型字段类型为 Generic(T)） ----------
    for item in &prog.items {
        if let Item::Struct(s) = item {
            let fields: Vec<(String, Ty)> = s
                .fields
                .iter()
                .map(|(n, t, _)| (n.clone(), t.clone().unwrap_or(Ty::I64)))
                .collect();
            ctx.structs.insert(s.name.clone(), fields);
        }
    }

    // ---------- 0a. 收集枚举变体 ----------
    for item in &prog.items {
        if let Item::Enum(en) = item {
            ctx.enums.insert(en.name.clone(), en.variants.clone());
        }
    }

    // ---------- 0b. 注册 impl 方法为顶层函数（名字 `类型__方法`）----------
    //   （后端在 codegen/jit 里也按此规则查找）
    // trait 表：trait 名 → 方法名列表（顺序即 vtable 索引）
    let mut traits: HashMap<String, Vec<String>> = HashMap::new();
    for item in &prog.items {
        if let Item::Trait(t) = item {
            let mut names: Vec<String> = t.methods.iter().map(|(n, _, _)| n.clone()).collect();
            for (n, _, _, _) in &t.defaults { if !names.contains(n) { names.push(n.clone()); } }
            traits.insert(t.name.clone(), names);
            // 收集方法签名（供 dyn 方法调用推断）
            let mut sigs: Vec<(String, Vec<Ty>, Ty)> = Vec::new();
            for (n, ps, r) in &t.methods { sigs.push((n.clone(), ps.clone(), r.clone())); }
            for (n, ps, r, _) in &t.defaults {
                let pts: Vec<Ty> = ps.iter().map(|(_, t)| t.clone()).collect();
                sigs.push((n.clone(), pts, r.clone()));
            }
            ctx.trait_methods.insert(t.name.clone(), sigs);
        }
    }
    // 关联类型校验：impl 的 `type Item = X` 必须覆盖 trait 声明的全部关联类型
    {
        let mut trait_assoc: HashMap<String, Vec<String>> = HashMap::new();
        let mut trait_required_methods: HashMap<String, Vec<String>> = HashMap::new();
        for item in &prog.items {
            if let Item::Trait(t) = item {
                trait_assoc.insert(t.name.clone(), t.assoc.clone());
                // 无默认体的方法必须由 impl 提供
                trait_required_methods.insert(t.name.clone(), t.methods.iter().map(|(n, _, _)| n.clone()).collect());
            }
        }
        // trait 名 → 父 trait 列表
        let mut trait_supers: HashMap<String, Vec<String>> = HashMap::new();
        for item in &prog.items {
            if let Item::Trait(t) = item { trait_supers.insert(t.name.clone(), t.supers.clone()); }
        }
        for item in &prog.items {
            if let Item::TraitImpl { trait_name, ty, assoc_bind, line, .. } = item {
                // 父 trait 必须也被 impl
                if let Some(supers) = trait_supers.get(trait_name) {
                    for sup in supers {
                        let has = prog.items.iter().any(|it| matches!(it, Item::TraitImpl { trait_name: tn, ty: tty, .. } if tn == sup && tty == ty));
                        if !has {
                            errors.push(crate::lb!(line, "impl of '{}' for '{}' requires supertrait '{}'", "为 '{}' 实现 '{}' 要求它也实现父 trait '{}'", ty, trait_name, sup));
                        }
                    }
                }
                if let Some(declared) = trait_assoc.get(trait_name) {
                    for an in declared {
                        if !assoc_bind.iter().any(|(n, _)| n == an) {
                            errors.push(crate::lb!(line, "missing associated type '{}' in impl of '{}'", "实现 '{}' 时缺少关联类型 '{}'", an, trait_name));
                        }
                    }
                    for (n, _) in assoc_bind {
                        if !declared.contains(n) {
                            errors.push(crate::lb!(line, "associated type '{}' is not declared in trait '{}'", "关联类型 '{}' 未在 trait '{}' 中声明", n, trait_name));
                        }
                    }
                }
                // trait 方法必须全部实现（无默认体时）；查顶层 `类型__方法`
                if let Some(required) = trait_required_methods.get(trait_name) {
                    // 用 prog.items 里的顶层 `类型__方法` 判断（fns 尚未收集）
                    let defined: std::collections::HashSet<String> = prog.items.iter().filter_map(|it| if let Item::Fn(f) = it { Some(f.name.clone()) } else { None }).collect();
                    for m in required {
                        if !defined.contains(&format!("{}__{}", ty, m)) {
                            errors.push(crate::lb!(line, "missing method '{}' in impl of '{}'", "实现 '{}' 时缺少方法 '{}'", m, trait_name));
                        }
                    }
                }
            }
        }
    }
    // (类型, trait) → 展平方法名列表（顺序同 trait 方法表）
    let mut trait_impls: HashMap<(String, String), Vec<String>> = HashMap::new();
    for item in &prog.items {
        if let Item::TraitImpl { trait_name, ty, .. } = item {
            // hoist 已把方法展平为顶层 `类型__方法`，这里按 trait 方法表顺序生成 vtable
            let names = traits.get(trait_name).cloned().unwrap_or_default();
            let flat: Vec<String> = names.iter().map(|mn| format!("{}__{}", ty, mn)).collect();
            trait_impls.insert((ty.clone(), trait_name.clone()), flat);
        }
    }
    // 存进 ctx（供 Analysis）
    let collected_traits = traits;
    let collected_trait_impls = trait_impls;
    for item in &prog.items {
        if let Item::Impl { ty, methods, .. } = item {
            for m in methods {
                let full = format!("{}__{}", ty, m.name);
                let params = m
                    .params
                    .iter()
                    .map(|p| p.ty.clone().unwrap_or(Ty::Unknown))
                    .collect::<Vec<_>>();
                ctx.fns.insert(
                    full,
                    FnSig { params, ret: m.ret.clone().unwrap_or(Ty::Unknown) },
                );
            }
        }
    }

    // ---------- 1. 收集常量 ----------
    for item in &prog.items {
        if let Item::Const { name, ty, value, line, .. } = item {
            match eval_const(value, &ctx.consts) {
                Ok((vty, v)) => {
                    let final_ty = match ty {
                        Some(decl) => {
                            // 常量也要做类型注解检查，否则 `const X: f64 = "abc"` 会漏网
                            if let Err(why) =
                                check_annotation(&format!("常量 '{}'", name), decl, &vty)
                            {
                                errors.push(crate::lb!(line, "{}", "{}", why));
                            }
                            decl.clone()
                        }
                        None => vty,
                    };
                    ctx.consts.insert(name.clone(), ConstVal { ty: final_ty, val: v });
                }
                Err(e) => errors.push(crate::lb!(line, "cannot evaluate constant '{}': {}", "常量 '{}' 无法求值：{}", name, e)),
            }
        }
    }

    // ---------- 2. 收集函数签名 ----------
    for item in &prog.items {
        if let Item::Fn(f) = item {
            let params = f
                .params
                .iter()
                .map(|p| p.ty.clone().unwrap_or(Ty::Unknown))
                .collect::<Vec<_>>();
            ctx.fns.insert(
                f.name.clone(),
                FnSig { params, ret: f.ret.clone().unwrap_or(Ty::Unknown) },
            );
        }
    }

    // ---------- 2b. 注册内联 C 块里的函数（自动解析出的签名） ----------
    for cf in &prog.cfuncs {
        if ctx.fns.contains_key(&cf.name) {
            errors.push(format!(
                "内联 C 函数 '{}' 与已有函数重名（GTLang 与 C 共用同一个符号空间）",
                cf.name
            ));
            continue;
        }
        ctx.fns.insert(
            cf.name.clone(),
            FnSig { params: cf.params.clone(), ret: cf.ret.clone() },
        );
    }

    // ---------- 3. 从调用点推断无标注形参（迭代至稳定） ----------
    for _ in 0..4 {
        let mut changed = false;
        let mut hints: Vec<(String, usize, Ty)> = Vec::new();
        for item in &prog.items {
            if let Item::Fn(f) = item {
                each_expr_block(&f.body, &mut |e| {
                    if let ExprKind::Call(name, args) = &e.kind {
                        if let Some(sig) = ctx.fns.get(name) {
                            for (i, a) in args.iter().enumerate() {
                                if i >= sig.params.len() {
                                    break;
                                }
                                if sig.params[i] == Ty::Unknown {
                                    if let Some(t) = guess(a, &ctx.consts) {
                                        hints.push((name.clone(), i, t));
                                    }
                                }
                            }
                        }
                    }
                });
            }
        }
        for (name, idx, ty) in hints {
            if let Some(sig) = ctx.fns.get_mut(&name) {
                if sig.params[idx] == Ty::Unknown {
                    sig.params[idx] = ty;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    // 回写形参类型；仍未推断出的按 i64 处理
    for item in &mut prog.items {
        if let Item::Fn(f) = item {
            if let Some(sig) = ctx.fns.get(&f.name) {
                let types = sig.params.clone();
                let body_snapshot = f.body.clone();
                let caps = ctx.capture_types.get(&f.name).cloned().unwrap_or_default();
                for (i, p) in f.params.iter_mut().enumerate() {
                    if p.ty.is_none() {
                        // 闭包捕获参数：优先用调用点 captures 的类型
                        if i < caps.len() && caps[i] != Ty::Unknown {
                            p.ty = Some(caps[i].clone());
                            continue;
                        }
                        let inferred = types.get(i).cloned().unwrap_or(Ty::I64);
                        // 无标注且被"以函数方式使用"的参数视为闭包
                        p.ty = if matches!(inferred, Ty::I64 | Ty::Unknown) && param_used_as_fn(&body_snapshot, &p.name) {
                            Some(Ty::Closure(Vec::new(), Box::new(Ty::I64)))
                        } else {
                            Some(inferred)
                        };
                    }
                    if p.ty == Some(Ty::Unknown) {
                        p.ty = Some(Ty::I64);
                    }
                }
            }
        }
    }

    // ---------- 4. 推断返回类型（两轮，覆盖相互递归） ----------
    for _round in 0..2 {
        for item in &mut prog.items {
            let Item::Fn(f) = item else { continue };
            if f.ret.is_some() {
                f.ret_ty = f.ret.clone().unwrap();
                if let Some(sig) = ctx.fns.get_mut(&f.name) {
                    sig.ret = f.ret_ty.clone();
                }
                continue;
            }
            ctx.scopes.push(HashMap::new());
            for p in &f.params {
                let ty = p.ty.clone().unwrap_or_else(|| if param_used_as_fn(&f.body, &p.name) { Ty::Closure(Vec::new(), Box::new(Ty::I64)) } else { Ty::I64 });
                ctx.scopes
                    .last_mut()
                    .unwrap()
                    .insert(p.name.clone(), VarInfo { ty, mutable: true, explicit: p.ty.is_some() });
            }
            ctx.pending_ret.push(f.name.clone());
            let t = infer_block_ret(&mut ctx, &mut f.body).unwrap_or(Ty::Void);
            ctx.pending_ret.pop();
            ctx.scopes.pop();

            f.ret_ty = if t == Ty::Unknown { Ty::Void } else { t };
            if let Some(sig) = ctx.fns.get_mut(&f.name) {
                sig.ret = f.ret_ty.clone();
            }
        }
    }

    // 把闭包捕获参数的类型真正写回 Param.ty（供 codegen/jit 使用；
    // 前面的回写发生在 capture_types 填充之前，这里补一次）。
    for item in &mut prog.items {
        if let Item::Fn(f) = item {
            let caps = ctx.capture_types.get(&f.name).cloned().unwrap_or_default();
            for (i, p) in f.params.iter_mut().enumerate() {
                if i < caps.len() && caps[i] != Ty::Unknown {
                    p.ty = Some(caps[i].clone());
                }
            }
        }
    }

    // ---------- 5. 逐函数类型检查 ----------
    for item in &mut prog.items {
        let Item::Fn(f) = item else { continue };
        ctx.cur_ret = f.ret_ty.clone();
        ctx.generic_bounds = f.bounds.clone();
        ctx.param_names = f.params.iter().map(|p| p.name.clone()).collect();
        ctx.scopes.push(HashMap::new());
        let caps = ctx.capture_types.get(&f.name).cloned().unwrap_or_default();
        for (i, p) in f.params.iter().enumerate() {
            let ty = if i < caps.len() && caps[i] != Ty::Unknown {
                caps[i].clone()
            } else {
                p.ty.clone().unwrap_or_else(|| if param_used_as_fn(&f.body, &p.name) { Ty::Closure(Vec::new(), Box::new(Ty::I64)) } else { Ty::I64 })
            };
            ctx.scopes
                .last_mut()
                .unwrap()
                .insert(p.name.clone(), VarInfo { ty, mutable: true, explicit: p.ty.is_some() });
        }
        check_block(&mut ctx, &mut f.body, &mut errors);

        // 显式写了返回类型时，块尾表达式也要与之一致。
        // （只有显式注解才检查：推断出来的类型本就来自块尾表达式，重复校验没有意义）
        if f.ret.is_some() && f.ret_ty != Ty::Void {
            if let Some(Stmt::Expr(tail)) = f.body.last() {
                if tail.ty != Ty::Void && tail.ty != Ty::Unknown {
                    if let Err(why) = check_annotation("函数返回值", &f.ret_ty, &tail.ty) {
                        errors.push(crate::lb!(tail.line, "{}", "{}", why));
                    }
                }
            }
            // 新增：显式非 void 返回类型的函数，函数体不能"落空"（缺少返回值）。
            if !block_yields(&f.body) {
                errors.push(crate::error::msg::missing_return(f.line, &f.ret_ty).render());
            }
        }

        ctx.scopes.pop();
    }

    // 把细化后的 struct 字段类型回写到 AST（供 codegen/mono 使用）
    for item in prog.items.iter_mut() {
        if let Item::Struct(s) = item {
            if let Some(fs) = ctx.structs.get(&s.name) {
                for (fname, fty, _) in s.fields.iter_mut() {
                    if let Some((_, t)) = fs.iter().find(|(n, _)| n == fname) {
                        *fty = Some(t.clone());
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(Analysis { consts: ctx.consts, traits: collected_traits, trait_impls: collected_trait_impls })
    } else {
        errors.dedup();
        Err(errors)
    }
}

// ============================================================
// 推断出的返回类型
// ============================================================

/// 参数是否"以函数方式使用"：出现在 `name(...)`（Call）的 name 位置，
/// 或作为高阶函数实参（直接传 `name` 给另一调用）。
fn param_used_as_fn(b: &Block, name: &str) -> bool {
    let mut found = false;
    each_expr_block(b, &mut |e| match &e.kind {
        // 只认"被调用"：`name(...)`。不把普通实参当作"传函数"，
        // 否则 `len(xs)` 里的 `xs` 会被误判为闭包。
        ExprKind::Call(callee, _) => {
            if callee == name {
                found = true;
            }
        }
        // 参数被"闭包捕获"（如 `return |x| g(f(x))` 里 f/g 进了闭包环境）
        // → 说明它是函数值，推为 Closure。
        ExprKind::ClosureNew { captures, .. } => {
            for c in captures {
                if let ExprKind::Ident(n) = &c.kind {
                    if n == name { found = true; }
                }
            }
        }
        // hoist 已把对闭包参数名的调用改成 CallValue(Ident(f), ...)
        ExprKind::CallValue { callee, .. } => {
            if let ExprKind::Ident(n) = &callee.kind {
                if n == name {
                    found = true;
                }
            }
        }
        _ => {}
    });
    found
}

fn infer_block_ret(ctx: &mut Ctx, b: &mut Block) -> Result<Ty, String> {
    let mut found: Option<Ty> = None;
    // 临时作用域：块内 let/const 可被块尾表达式引用（多语句内联的块表达式依赖此）
    ctx.scopes.push(HashMap::new());
    for s in b.iter_mut() {
        let t = match s {
            Stmt::Let { name, value, ty: ann, mutable, .. } => {
                if let Ok(vt) = ctx.infer(value) {
                    let ty = ann.clone().unwrap_or(vt);
                    let ty = if ty == Ty::Unknown { Ty::I64 } else { ty };
                    ctx.scopes.last_mut().unwrap().insert(name.clone(), VarInfo { ty, mutable: *mutable, explicit: ann.is_some() });
                }
                None
            }
            Stmt::Const { name, value, .. } => {
                if let Ok(vt) = ctx.infer(value) {
                    let ty = if vt == Ty::Unknown { Ty::I64 } else { vt };
                    ctx.scopes.last_mut().unwrap().insert(name.clone(), VarInfo { ty, mutable: false, explicit: true });
                }
                None
            }
            Stmt::Return(Some(e), _) => ctx.infer(e).ok(),
            Stmt::If { then, els, .. } => {
                let a = infer_block_ret(ctx, then).ok();
                let b2 = match els {
                    Some(e) => infer_block_ret(ctx, e).ok(),
                    None => None,
                };
                a.or(b2)
            }
            Stmt::Block(inner) => infer_block_ret(ctx, inner).ok(),
            Stmt::While { body, .. } | Stmt::DoWhile { body, .. } | Stmt::ForRange { body, .. } | Stmt::ForEach { body, .. } => {
                infer_block_ret(ctx, body).ok()
            }
            _ => None,
        };
        if found.is_none() && t.is_some() && t != Some(Ty::Void) {
            found = t;
        }
    }
    // 块尾表达式即返回值
    if found.is_none() {
        if let Some(Stmt::Expr(e)) = b.last_mut() {
            found = ctx.infer(e).ok().filter(|t| *t != Ty::Void);
        }
    }
    ctx.scopes.pop();
    Ok(found.unwrap_or(Ty::Void))
}

// ============================================================
// 类型检查
// ============================================================

/// 块是否"产出值"（可作为返回值）：尾语句是表达式、return，或 if/else 两分支都产出值。
fn block_yields(b: &Block) -> bool {
    match b.last() {
        None => false,
        Some(Stmt::Expr(e)) => e.ty != Ty::Void,
        Some(Stmt::Return(..)) => true,
        Some(Stmt::If { then, els: Some(els), .. }) => block_yields(then) && block_yields(els),
        Some(Stmt::Block(inner)) => block_yields(inner),
        _ => false,
    }
}

impl Ctx {
    /// 可变借出变量信息（用于 push 细化 list 元素类型等）。
    fn lookup_var_mut(&mut self, name: &str) -> Option<&mut VarInfo> {
        for sc in self.scopes.iter_mut().rev() {
            if sc.contains_key(name) { return sc.get_mut(name); }
        }
        None
    }

    fn lookup(&self, name: &str) -> Option<Ty> {
        for s in self.scopes.iter().rev() {
            if let Some(v) = s.get(name) {
                return Some(v.ty.clone());
            }
        }
        self.consts.get(name).map(|c| c.ty.clone())
    }

    /// 名字是否可变（let/const 不可变；:= / let mut / 形参 可变）
    fn is_mutable(&self, name: &str) -> bool {
        for s in self.scopes.iter().rev() {
            if let Some(v) = s.get(name) {
                return v.mutable;
            }
        }
        false
    }

}

// ============================================================
// 辅助
// ============================================================

/// 类型规则统一来自 `type.rs`（单一事实来源，两个后端共用）
use crate::types::{
    binary_result, builtin_ret, check_annotation, is_assignable, numeric_join, unary_result,
};


