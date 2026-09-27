//! 语义分析：常量求值、无标注形参推断、表达式类型推断与检查。
//!
//! 推断结果直接回写到 AST（`Expr::ty`、`Param::ty`、`FnDef::ret_ty`），
//! 使代码生成阶段只有一个类型来源。

use std::collections::HashMap;

use crate::ast::*;

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
                for (i, p) in f.params.iter_mut().enumerate() {
                    if p.ty.is_none() {
                        p.ty = Some(types.get(i).cloned().unwrap_or(Ty::I64));
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
                ctx.scopes
                    .last_mut()
                    .unwrap()
                    .insert(p.name.clone(), VarInfo { ty: p.ty.clone().unwrap_or(Ty::I64), mutable: true, explicit: p.ty.is_some() });
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

    // ---------- 5. 逐函数类型检查 ----------
    for item in &mut prog.items {
        let Item::Fn(f) = item else { continue };
        ctx.cur_ret = f.ret_ty.clone();
        ctx.scopes.push(HashMap::new());
        for p in &f.params {
            ctx.scopes
                .last_mut()
                .unwrap()
                .insert(p.name.clone(), VarInfo { ty: p.ty.clone().unwrap_or(Ty::I64), mutable: true, explicit: p.ty.is_some() });
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

fn check_block(ctx: &mut Ctx, b: &mut Block, errors: &mut Vec<String>) {
    // 不引入新作用域：GTLang 的块作用域不严格（解构块依赖此）
    for s in b.iter_mut() {
        check_stmt(ctx, s, errors);
    }
}

fn check_stmt(ctx: &mut Ctx, s: &mut Stmt, errors: &mut Vec<String>) {
    match s {
        Stmt::Labeled { inner, .. } => {
            let mut inner_blk: Block = vec![(**inner).clone()];
            check_block(ctx, &mut inner_blk, errors);
        }
        Stmt::Let { name, ty, value, line, mutable } => {
            match ctx.infer(value) {
                Ok(vty) => {
                    let final_ty = match ty {
                        Some(decl) => {
                            if let Err(why) =
                                check_annotation(&format!("变量 '{}'", name), decl, &vty)
                            {
                                errors.push(crate::lb!(line, "{}", "{}", why));
                            }
                            // 双向推断：值类型未知（如 [] / None）时，用声明类型回填
                            if vty == Ty::Unknown {
                                value.ty = decl.clone();
                            }
                            decl.clone()
                        }
                        None => {
                            if vty == Ty::Unknown {
                                Ty::I64
                            } else {
                                vty
                            }
                        }
                    };
                    let explicit = ty.is_some();
                    ctx.scopes.last_mut().unwrap().insert(name.clone(), VarInfo { ty: final_ty, mutable: *mutable, explicit });
                }
                Err(e) => errors.push(e),
            }
        }
        Stmt::Const { name, ty, value, line } => {
            match eval_const(value, &ctx.consts) {
                Ok((vty, v)) => {
                    let final_ty = match ty {
                        Some(decl) => {
                            if let Err(why) = check_annotation(&format!("常量 '{}'", name), decl, &vty) {
                                errors.push(crate::lb!(line, "{}", "{}", why));
                            }
                            decl.clone()
                        }
                        None => if vty == Ty::Unknown { Ty::I64 } else { vty },
                    };
                    ctx.consts.insert(name.clone(), ConstVal { ty: final_ty.clone(), val: v });
                    // 同时进作用域（不可变），使后续引用解析到名字
                    ctx.scopes.last_mut().unwrap().insert(name.clone(), VarInfo { ty: final_ty, mutable: false, explicit: true });
                }
                Err(e) => errors.push(crate::lb!(line, "cannot evaluate constant '{}': {}", "常量 '{}' 无法求值：{}", name, e)),
            }
        }
        Stmt::Assign { name, index, op, value, line } => {
            if ctx.lookup(name).is_some() && !ctx.is_mutable(name) {
                errors.push(crate::lb!(line, "cannot assign to immutable variable or constant '{}'", "不能给不可变变量或常量 '{}' 赋值", name));
            }
            let vt = ctx.lookup(name);
            match vt {
                None => {
                    // 裸赋值 `x = v`：自动声明（类型由 RHS 推导，可变）
                    match ctx.infer(value) {
                        Ok(rty) => {
                            let new_ty = if rty == Ty::Unknown { Ty::I64 } else { rty };
                            ctx.scopes.last_mut().unwrap().insert(
                                name.clone(),
                                VarInfo { ty: new_ty, mutable: true, explicit: false },
                            );
                        }
                        Err(e) => errors.push(e),
                    }
                }
                Some(vt) => {
                    // 数组元素赋值：左值类型是元素类型，并顺带校验下标
                    let (lt, target) = match index {
                        None => (vt.clone(), format!("'{}'", name)),
                        Some(idx) => match &vt {
                            Ty::Array(el, _) => {
                                if let Err(e) = ctx.infer(idx) {
                                    errors.push(e);
                                } else if !idx.ty.is_int() && idx.ty != Ty::Unknown {
                                    errors.push(crate::lb!(line, "index must be an integer, found {}", "下标应为整数，实际是 {}", idx.ty));
                                }
                                ((**el).clone(), format!("'{}[]'", name))
                            }
                            Ty::List(el) => {
                                if let Err(e) = ctx.infer(idx) {
                                    errors.push(e);
                                } else if !idx.ty.is_int() && idx.ty != Ty::Unknown {
                                    errors.push(crate::lb!(line, "list index must be an integer, found {}", "list 下标应为整数，实际是 {}", idx.ty));
                                }
                                ((**el).clone(), format!("'{}[]'", name))
                            }
                            Ty::Map(k, v) => {
                                if let Err(e) = ctx.infer(idx) {
                                    errors.push(e);
                                } else if idx.ty != Ty::Unknown && !is_assignable(&k, &idx.ty) {
                                    errors.push(crate::lb!(line, "map key must be {}, found {}", "map 键应为 {}，实际是 {}", k, idx.ty));
                                }
                                ((**v).clone(), format!("'{}[]'", name))
                            }
                            other => {
                                errors.push(crate::lb!(line, "{} does not support indexed assignment", "{} 不支持下标赋值", other));
                                (Ty::Unknown, format!("'{}'", name))
                            }
                        },
                    };
                    match ctx.infer(value) {
                        Ok(rty) => {
                            if op.is_some() {
                                let want = binary_result(op.unwrap(), &lt, &rty).unwrap_or(lt.clone());
                                if !compatible(&want, &rty) {
                                    errors.push(crate::lb!(line, "cannot assign {} to {}{}", "无法把 {} 赋给 {}{}",
                                        rty, target,
                                        if want == Ty::Unknown { String::new() } else { format!("({})", want) }));
                                }
                            } else if index.is_none() {
                                // 纯赋值 x = v：允许改变变量类型
                                let new_ty = if rty == Ty::Unknown { Ty::I64 } else { rty.clone() };
                                for sc in ctx.scopes.iter_mut().rev() {
                                    if let Some(entry) = sc.get_mut(name) {
                                        if entry.explicit {
                                            errors.push(crate::lb!(line, "cannot change type of '{}': declared with explicit type", "无法改变 '{}' 的类型：它带显式类型声明", name));
                                        } else {
                                            entry.ty = new_ty.clone();
                                        }
                                        break;
                                    }
                                }
                            } else if !compatible(&lt, &rty) {
                                errors.push(crate::lb!(line, "cannot assign {} to {}{}", "无法把 {} 赋给 {}{}",
                                    rty, target,
                                    if lt == Ty::Unknown { String::new() } else { format!("({})", lt) }));
                            }
                        }
                        Err(e) => errors.push(e),
                    }
                }
            }
        }
        Stmt::FieldAssign { obj, field, op, value, line } => {
            match ctx.lookup(obj) {
                None => errors.push(crate::lb!(line, "undefined variable '{}'", "未定义的变量 '{}'", obj)),
                Some(Ty::Struct(sname)) => {
                    let fty = ctx
                        .structs
                        .get(&sname)
                        .and_then(|fs| fs.iter().find(|(n, _)| n == field).map(|(_, t)| t.clone()));
                    match fty {
                        None => errors.push(crate::lb!(line, "struct '{}' has no field '{}'", "结构体 '{}' 没有字段 '{}'", sname, field)),
                        Some(fty) => {
                            let want = match (op, ctx.infer(value)) {
                                (Some(o), Ok(rty)) => {
                                    binary_result(*o, &fty, &rty).unwrap_or(fty.clone())
                                }
                                (None, Ok(rty)) => {
                                    if !compatible(&fty, &rty) {
                                        errors.push(crate::lb!(line, "cannot assign {} to field '{}.{}' ({})", "无法把 {} 赋给字段 '{}.{}'（{}）", rty, obj, field, fty));
                                    }
                                    fty.clone()
                                }
                                (_, Err(e)) => {
                                    errors.push(e);
                                    fty.clone()
                                }
                            };
                            let _ = want;
                        }
                    }
                }
                Some(other) => errors.push(crate::lb!(line, "{} is not a struct; cannot access field '{}'", "{} 不是结构体，不能访问字段 '{}'", other, field)),
            }
        }
        Stmt::Expr(e) => {
            if let Err(err) = ctx.infer(e) {
                errors.push(err);
            }
        }
        Stmt::If { cond, then, els, line } => {
            check_cond(ctx, cond, line, errors);
            check_block(ctx, then, errors);
            if let Some(e) = els {
                check_block(ctx, e, errors);
            }
        }
        Stmt::DoWhile { body, cond, line } => {
            ctx.loop_depth += 1;
            check_block(ctx, body, errors);
            ctx.loop_depth -= 1;
            check_cond(ctx, cond, line, errors);
        }
        Stmt::While { cond, body, line } => {
            check_cond(ctx, cond, line, errors);
            ctx.loop_depth += 1;
            check_block(ctx, body, errors);
            ctx.loop_depth -= 1;
        }
        Stmt::ForRange { var, from, to, body, els: _, line: _ } => {
            for e in [from, to] {
                if let Err(err) = ctx.infer(e) {
                    errors.push(err);
                }
            }
            ctx.scopes.push(HashMap::new());
            ctx.scopes.last_mut().unwrap().insert(var.clone(), VarInfo { ty: Ty::I64, mutable: true, explicit: false });
            ctx.loop_depth += 1;
            check_block(ctx, body, errors);
            ctx.loop_depth -= 1;
            ctx.scopes.pop();
        }
        Stmt::ForEach { var, iter, body, els: _, line } => {
            match ctx.infer(iter) {
                Ok(t) => {
                    let elem = match &t {
                        Ty::Array(el, _) => (**el).clone(),
                        Ty::List(el) => (**el).clone(),
                        _ => {
                            errors.push(crate::error::msg::not_iterable(*line, &t).render());
                            Ty::I64
                        }
                    };
                    ctx.scopes.push(HashMap::new());
                    ctx.scopes.last_mut().unwrap().insert(var.clone(), VarInfo { ty: elem, mutable: true, explicit: false });
                    ctx.loop_depth += 1;
                    check_block(ctx, body, errors);
                    ctx.loop_depth -= 1;
                    ctx.scopes.pop();
                }
                Err(e) => errors.push(e),
            }
        }
        Stmt::Return(e, line) => {
            if let Some(e) = e {
                match ctx.infer(e) {
                    Ok(t) => {
                        let want = ctx.cur_ret.clone();
                        if want != Ty::Void {
                            if let Err(why) = check_annotation("函数返回值", &want, &t) {
                                errors.push(crate::lb!(line, "{}", "{}", why));
                            }
                        }
                    }
                    Err(err) => errors.push(err),
                }
            }
        }
        Stmt::Block(b) => { for s in b.iter_mut() { check_stmt(ctx, s, errors); } }
        Stmt::Asm { .. } => {}
        // go f(args)：检查函数存在 + 推断实参
        Stmt::Go { func, args, line } => {
            if !ctx.fns.contains_key(func) && !crate::types::is_builtin_name(func) {
                errors.push(crate::error::msg::undefined_fn(*line, func).render());
            }
            for a in args.iter_mut() { let _ = ctx.infer(a); }
        }
        Stmt::Throw(e, _) => {
            if let Err(err) = ctx.infer(e) {
                errors.push(err);
            }
        }
        Stmt::Try { body, catches, fin, line } => {
            ctx.try_depth += 1;
            check_block(ctx, body, errors);
            ctx.try_depth -= 1;
            for ca in catches.iter_mut() {
                // 绑定变量进作用域
                ctx.scopes.push(HashMap::new());
                if let Some(binding) = &ca.binding {
                    let bt = match ctx.infer(&mut Expr::new(ExprKind::Int(0), ca.line)) {
                        Ok(_) => Ty::Unknown,
                        Err(_) => Ty::Unknown,
                    };
                    let _ = bt;
                    // 绑定类型由 body 的错误类型决定；简化记为 Unknown（后端按 I64 处理）
                    ctx.scopes
                        .last_mut()
                        .unwrap()
                        .insert(binding.clone(), VarInfo { ty: Ty::Unknown, mutable: true, explicit: false });
                }
                if let Some(g) = ca.guard.as_mut() {
                    if let Err(err) = ctx.infer(g) {
                        errors.push(err);
                    }
                }
                check_block(ctx, &mut ca.body, errors);
                ctx.scopes.pop();
            }
            if let Some(f) = fin {
                check_block(ctx, f, errors);
            }
            let _ = line;
        }
        // 嵌套函数已在 hoist 阶段提升为顶层，这里不应出现
        Stmt::LocalFn(_) => {}
        Stmt::Break(_lbl, line) => {
            if ctx.loop_depth == 0 {
                errors.push(crate::error::msg::loop_control_outside(*line, "break").render());
            }
        }
        Stmt::Continue(_lbl, line) => {
            if ctx.loop_depth == 0 {
                errors.push(crate::error::msg::loop_control_outside(*line, "continue").render());
            }
        }
    }
}

fn check_cond(ctx: &mut Ctx, cond: &mut Expr, line: &usize, errors: &mut Vec<String>) {
    match ctx.infer(cond) {
        Ok(t) => {
            if t != Ty::Bool && t != Ty::Unknown {
                errors.push(crate::lb!(line, "condition must be bool, found {}", "条件应为布尔(bool)，实际是 {}", t));
            }
        }
        Err(e) => errors.push(e),
    }
}

impl Ctx {
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

    /// 推断并回填表达式类型
    fn infer(&mut self, e: &mut Expr) -> Result<Ty, String> {
        let ty = match &mut e.kind {
            ExprKind::Int(_) => Ty::I64,
            ExprKind::Float(_) => Ty::F64,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::TupleLit(items) => { let mut ts = Vec::new(); for it in items.iter_mut() { ts.push(self.infer(it)?); } Ty::Tuple(ts) },

            ExprKind::DynBox { trait_name, value } => { self.infer(value)?; Ty::Dyn(trait_name.clone()) },
            ExprKind::EnumLit(name, variant, args) => {
                let ets: Vec<Ty> = match self.enums.get(name) {
                    Some(vs) => vs.iter().find(|(n, _)| n == variant).map(|(_, ts)| ts.clone()).unwrap_or_default(),
                    None => return Err(crate::lb!(e.line, "undefined enum '{}'", "未定义的枚举 '{}'", name)),
                };
                for a in args.iter_mut() { self.infer(a)?; }
                let _ = ets;
                Ty::Enum(name.clone())
            }
            ExprKind::ListComp { expr, var, iter, cond } => {
                let it = self.infer(iter)?;
                let elem = match &it {
                    Ty::List(e) | Ty::Array(e, _) | Ty::Set(e) => (**e).clone(),
                    _ => Ty::Unknown,
                };
                self.scopes.push(HashMap::new());
                self.scopes.last_mut().unwrap().insert(var.clone(), VarInfo { ty: elem, mutable: false, explicit: false });
                if let Some(c) = cond { self.infer(c)?; }
                let et = self.infer(expr)?;
                self.scopes.pop();
                Ty::List(Box::new(et))
            }
            ExprKind::CallNamed(_, named) => { for (_, v) in named.iter_mut() { let _ = self.infer(v); } Ty::Unknown }
            ExprKind::Str(_) => Ty::Str,
            ExprKind::Interp(parts) => {
                for p in parts.iter_mut() {
                    if let StrPart::Expr(inner) = p {
                        self.infer(inner)?;
                    }
                }
                Ty::Str
            }
            ExprKind::Ident(n) => match self.lookup(n) {
                Some(t) => t,
                None => {
                    let base = crate::lb!(e.line, "undefined variable '{}'", "未定义的变量 '{}'", n);
                    let hint = match closest_name(n, self) {
                        Some(s) => if crate::lang::is_zh() { format!("\x01是否想用 '{}'？", s) } else { format!("\x01did you mean '{}'?", s) },
                        None => String::new(),
                    };
                    return Err(format!("{}{}", base, hint));
                }
            },
            ExprKind::Unary(op, a) => {
                let at = self.infer(a)?;
                let op = *op;
                unary_result(op, &at)
                    .map_err(|why| crate::lb!(e.line, "{}", "{}", why))?
            }
            ExprKind::Binary(op, a, b) => {
                let at = self.infer(a)?;
                let bt = self.infer(b)?;
                let op = *op;
                binary_result(op, &at, &bt)
                    .map_err(|why| crate::lb!(e.line, "{}", "{}", why))?
            }
            ExprKind::Try(inner) => {
                let it = self.infer(inner)?;
                match it {
                    Ty::Result(t, _e) => {
                        // ? 允许在返回 Result 的函数中，或 try 块内（由 try 捕获）
                        if self.try_depth == 0 && !matches!(self.cur_ret, Ty::Result(..)) && self.cur_ret != Ty::Unknown {
                            return Err(crate::lb!(e.line,
                                "'?' can only be used in a function returning Result",
                                "'?' 只能用在返回 Result 的函数中"));
                        }
                        (*t).clone()
                    }
                    Ty::Option(t) => {
                        // ? 用于 Option：None 时提前返回 None，Some 时解包
                        if self.try_depth == 0 && !matches!(self.cur_ret, Ty::Option(_)) && self.cur_ret != Ty::Unknown {
                            return Err(crate::lb!(e.line,
                                "'?' can only be used in a function returning Option",
                                "'?' 只能用在返回 Option(?T) 的函数中"));
                        }
                        (*t).clone()
                    }
                    Ty::Unknown => Ty::Unknown,
                    other => return Err(crate::lb!(e.line,
                        "'?' requires a Result or Option, found {}", "'?' 需要 Result 或 Option，实际是 {}", other)),
                }
            }
            ExprKind::Some(inner) => {
                let it = self.infer(inner)?;
                let t = match &self.cur_ret {
                    Ty::Option(ot) => (**ot).clone(),
                    _ => it,
                };
                Ty::Option(Box::new(t))
            }
            ExprKind::None => {
                let t = match &self.cur_ret {
                    Ty::Option(ot) => (**ot).clone(),
                    _ => Ty::Unknown,
                };
                Ty::Option(Box::new(t))
            }
            ExprKind::TryBlock { body, catches, fin } => {
                self.try_depth += 1;
                check_block(self, body, &mut Vec::new());
                self.try_depth -= 1;
                for ca in catches.iter_mut() {
                    if let Some(g) = ca.guard.as_mut() { let _ = self.infer(g); }
                    check_block(self, &mut ca.body, &mut Vec::new());
                }
                if let Some(f) = fin { check_block(self, f, &mut Vec::new()); }
                // 值类型取 body 末表达式（简化）
                match body.last_mut() {
                    Some(Stmt::Expr(e)) => self.infer(e)?,
                    _ => Ty::Void,
                }
            }
            ExprKind::TryOr { inner, default } => {
                let it = self.infer(inner)?;
                let dt = self.infer(default)?;
                match it {
                    Ty::Option(t) => {
                        if dt == Ty::Unknown { (*t).clone() } else { numeric_join(&t, &dt) }
                    }
                    Ty::Result(t, _) => {
                        if dt == Ty::Unknown { (*t).clone() } else { numeric_join(&t, &dt) }
                    }
                    Ty::Unknown => dt,
                    other => return Err(crate::lb!(e.line,
                        "'or' requires an Option or Result, found {}", "'or' 需要 Option 或 Result，实际是 {}", other)),
                }
            }
            ExprKind::Ok(inner) | ExprKind::Err(inner) => {
                let it = self.infer(inner)?;
                let (t, er) = match &self.cur_ret {
                    Ty::Result(rt, re) => ((**rt).clone(), (**re).clone()),
                    _ => (it.clone(), it.clone()),
                };
                Ty::Result(Box::new(t), Box::new(er))
            }
            ExprKind::Call(name, args) => {
                let name = name.clone();
                // `s.方法(...)`：s 是 dyn Trait 对象 → 查 trait 方法
                if let Some(dot) = name.find('.') {
                    let recv = self.lookup(&name[..dot]);
                    let tr = match recv { Some(Ty::Dyn(tr)) => Some(tr), _ => None };
                    if let Some(tr) = tr {
                        let mname = name[dot+1..].to_string();
                        let ret = self.trait_methods.get(&tr).and_then(|ms| ms.iter().find(|(n, _, _)| *n == mname).map(|(_, _, r)| r.clone()));
                        if let Some(ret) = ret {
                            for a in args.iter_mut().skip(1) { let _ = self.infer(a); }
                            return Ok(ret);
                        }
                    }
                }
                let mut arg_tys = Vec::with_capacity(args.len());
                for a in args.iter_mut() {
                    arg_tys.push(self.infer(a)?);
                }
                // Result 构造：Ok(v) / Err(e)
                if name == "Ok" || name == "Err" {
                    if arg_tys.len() != 1 {
                        return Err(crate::lb!(e.line, "{}() takes exactly 1 argument", "{}() 恰好需要 1 个参数", name));
                    }
                    let inner = arg_tys[0].clone();
                    let (t, er) = match &self.cur_ret {
                        Ty::Result(rt, re) => ((**rt).clone(), (**re).clone()),
                        _ => (inner.clone(), inner.clone()),
                    };
                    return Ok(Ty::Result(Box::new(t), Box::new(er)));
                }
                // Option 构造：Some(v) / None
                if name == "Some" {
                    if arg_tys.len() != 1 {
                        return Err(crate::lb!(e.line, "Some() takes exactly 1 argument", "Some() 恰好需要 1 个参数"));
                    }
                    let inner = arg_tys[0].clone();
                    let t = match &self.cur_ret {
                        Ty::Option(ot) => (**ot).clone(),
                        _ => inner,
                    };
                    return Ok(Ty::Option(Box::new(t)));
                }
                if name == "None" {
                    if !arg_tys.is_empty() {
                        return Err(crate::lb!(e.line, "None takes no arguments", "None 不接受参数"));
                    }
                    let t = match &self.cur_ret {
                        Ty::Option(ot) => (**ot).clone(),
                        _ => Ty::Unknown,
                    };
                    return Ok(Ty::Option(Box::new(t)));
                }
                // 用户函数优先于标准库（用户定义同名函数时遮蔽 stdlib）
                if let Some(sig) = self.fns.get(&name).cloned() {
                    if sig.params.len() != arg_tys.len() {
                        return Err(crate::error::msg::arity_mismatch(
                            e.line, &name, sig.params.len(), arg_tys.len(),
                        ).render());
                    }
                    for (i, (want, got)) in sig.params.iter().zip(arg_tys.iter()).enumerate() {
                        if !compatible(want, got) {
                            return Err(crate::error::msg::arg_type_mismatch(
                                e.line, &name, i, want, got,
                            ).render());
                        }
                    }
                    if sig.ret == Ty::Unknown { Ty::I64 } else { sig.ret }
                } else if let Some(r) = builtin_ret(&name, &arg_tys) {
                    r.map_err(|why| crate::lb!(e.line, "{}", "{}", why))?
                } else if let Some(sf) = crate::types::stdlib_fn(&name) {
                    // 标准库（libGT.dll）：检查元数，参数按需数值提升
                    if sf.params.len() != arg_tys.len() {
                        return Err(crate::error::msg::arity_mismatch(e.line, &name, sf.params.len(), arg_tys.len()).render());
                    }
                    for (want, got) in sf.params.iter().zip(arg_tys.iter()) {
                        if !is_assignable(want, got) {
                            return Err(crate::lb!(e.line, "{}() argument must be {}, found {}", "{}() 参数类型应为 {}，实际是 {}", name, want, got));
                        }
                    }
                    sf.ret
                } else if let Some(vt) = self.lookup(&name) {
                    // 局部变量是闭包：按闭包调用推断
                    match vt {
                        Ty::Closure(_, ret) => (*ret).clone(),
                        _ => return Err(crate::error::msg::undefined_fn(e.line, &name).render()),
                    }
                } else {
                    let sig = match self.fns.get(&name).cloned() {
                        Some(s) => s,
                        None => {
                            let base = crate::error::msg::undefined_fn(e.line, &name).render();
                            let hint = closest_fn(&name, self).map(|s| if crate::lang::is_zh() { format!("\x01是否想用 '{}'？", s) } else { format!("\x01did you mean '{}'?", s) }).unwrap_or_default();
                            return Err(format!("{}{}", base, hint));
                        }
                    };
                    if sig.params.len() != arg_tys.len() {
                        return Err(crate::error::msg::arity_mismatch(
                            e.line, &name, sig.params.len(), arg_tys.len(),
                        ).render());
                    }
                    for (i, (want, got)) in sig.params.iter().zip(arg_tys.iter()).enumerate() {
                        if !compatible(want, got) {
                            return Err(crate::error::msg::arg_type_mismatch(
                                e.line, &name, i, want, got,
                            ).render());
                        }
                    }
                    if sig.ret == Ty::Unknown {
                        // 递归或尚未推断完成：暂按 i64 处理
                        Ty::I64
                    } else {
                        sig.ret
                    }
                }
            }
            ExprKind::Index(base, idx) => {
                let bt = self.infer(base)?;
                let it = self.infer(idx)?;
                match bt {
                    Ty::Array(el, _) => (*el).clone(),
                    Ty::Str => Ty::I64,
                    Ty::List(el) if false => (*el).clone(),
                    Ty::List(el) => (*el).clone(),
                    Ty::Map(k, v) => {
                        // map 下标：键须匹配键类型
                        if it != Ty::Unknown && !is_assignable(&k, &it) {
                            return Err(crate::lb!(e.line, "map key must be {}, found {}", "map 键应为 {}，实际是 {}", k, it));
                        }
                        (*v).clone()
                    }
                    Ty::Unknown => Ty::I64,
                    other => {
                        return Err(crate::lb!(e.line, "{} does not support indexing", "{} 不支持下标访问", other))
                    }
                }
            }
            ExprKind::Slice(base, lo, hi) => {
                let bt = self.infer(base)?;
                self.infer(lo)?;
                self.infer(hi)?;
                match bt {
                    Ty::Str => Ty::Str,
                    Ty::List(el) => Ty::List(el),
                    Ty::Array(el, _) => Ty::List(el),
                    Ty::Unknown => Ty::Unknown,
                    other => return Err(crate::lb!(e.line, "{} does not support slicing", "{} 不支持切片", other)),
                }
            }
            ExprKind::ArrayLit(items) => {
                let mut elem: Option<Ty> = None;
                for it in items.iter_mut() {
                    let t = self.infer(it)?;
                    elem = Some(match (&elem, &t) {
                        (None, _) => t,
                        (Some(a), b) => numeric_join(a, b),
                    });
                }
                let elem = elem.unwrap_or(Ty::I64);
                let elem = if elem == Ty::Unknown { Ty::I64 } else { elem };
                Ty::Array(Box::new(elem), items.len())
            }
            ExprKind::If { cond, then, els } => {
                let ct = self.infer(cond)?;
                if ct != Ty::Bool && ct != Ty::Unknown {
                    return Err(crate::lb!(e.line, "condition must be bool, found {}", "条件应为布尔(bool)，实际是 {}", ct));
                }
                let tt = infer_block_ret(self, then)?;
                let et = match els {
                    Some(b) => infer_block_ret(self, b)?,
                    None => Ty::Void,
                };
                if tt == Ty::Void {
                    et
                } else if et == Ty::Void {
                    tt
                } else {
                    numeric_join(&tt, &et)
                }
            }
            ExprKind::Field(base, field) => {
                let bt = self.infer(base)?;
                match bt {
                    Ty::Struct(sname) => {
                        let fty = self
                            .structs
                            .get(&sname)
                            .and_then(|fs| fs.iter().find(|(n, _)| n == field).map(|(_, t)| t.clone()));
                        match fty {
                            Some(t) => t,
                            None => {
                                {
                                    let base = crate::error::msg::no_such_field(e.line, &sname, field).render();
                                    let hint = self.structs.get(&sname).and_then(|fs| closest_of(field, fs.iter().map(|(n, _)| n.clone()))).map(|s| if crate::lang::is_zh() { format!("\x01是否想用 '{}'？", s) } else { format!("\x01did you mean '{}'?", s) }).unwrap_or_default();
                                    return Err(format!("{}{}", base, hint));
                                }
                            }
                        }
                    }
                    Ty::Tuple(ts) => {
                        // 元组下标：`.0`
                        let i: usize = field.parse().unwrap_or(usize::MAX);
                        match ts.get(i) { Some(t) => t.clone(), None => return Err(crate::lb!(e.line, "tuple index {} out of range", "元组下标 {} 越界", field)) }
                    }
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        return Err(crate::lb!(e.line, "{} is not a struct; cannot access field '{}'", "{} 不是结构体，不能访问字段 '{}'", other, field))
                    }
                }
            }
            ExprKind::Match { subject, arms } => {
                let st = self.infer(subject)?;
                let mut result: Option<Ty> = None;
                for arm in arms.iter_mut() {
                    // Result/Option 变体绑定：Ok(v) / Err(e) / Some(v)
                    if let Some((ctor, bind)) = match_result_bind(arm.pat.as_ref()) {
                        let inner = match &st {
                            Ty::Result(t, e) => if ctor == "Ok" { (**t).clone() } else { (**e).clone() },
                            Ty::Option(t) => (**t).clone(),
                            _ => Ty::Unknown,
                        };
                        self.scopes.push(HashMap::new());
                        self.scopes.last_mut().unwrap().insert(bind.clone(), VarInfo { ty: inner, mutable: false, explicit: false });
                    } else if let Some(ExprKind::EnumLit(en, var, binds)) = arm.pat.as_ref().map(|p| &p.kind) {
                        let payload: Vec<Ty> = self.enums.get(en).and_then(|vs| vs.iter().find(|(n, _)| n == var).map(|(_, ts)| ts.clone())).unwrap_or_default();
                        self.scopes.push(HashMap::new());
                        for (i, b) in binds.iter().enumerate() {
                            if let ExprKind::Ident(bn) = &b.kind {
                                let ty = payload.get(i).cloned().unwrap_or(Ty::Unknown);
                                self.scopes.last_mut().unwrap().insert(bn.clone(), VarInfo { ty, mutable: false, explicit: false });
                            }
                        }
                    } else if let Some(p) = arm.pat.as_mut() {
                        let pt = self.infer(p)?;
                        if pt != Ty::Unknown && st != Ty::Unknown && !compatible(&st, &pt) && !compatible(&pt, &st) {
                            return Err(crate::lb!(arm.line, "match pattern type {} does not match subject {}", "match 模式类型 {} 与主体 {} 不匹配", pt, st));
                        }
                    }
                    if let Some((lo, hi)) = arm.range.as_mut() {
                        let lt = self.infer(lo)?;
                        let ht = self.infer(hi)?;
                        if lt != Ty::Unknown && !lt.is_int() { return Err(crate::lb!(arm.line, "range start must be integer", "范围起点应为整数")); }
                        if ht != Ty::Unknown && !ht.is_int() { return Err(crate::lb!(arm.line, "range end must be integer", "范围终点应为整数")); }
                    }
                    if let Some(g) = arm.guard.as_mut() {
                        let gt = self.infer(g)?;
                        if gt != Ty::Bool && gt != Ty::Unknown {
                            return Err(crate::lb!(arm.line, "match guard must be bool", "match 守卫应为布尔"));
                        }
                    }
                    let bt = infer_block_ret(self, &mut arm.body)?;
                    // 枚举解构模式推入的 scope 需弹出
                    if matches!(arm.pat.as_ref().map(|p| &p.kind), Some(ExprKind::EnumLit(..))) {
                        self.scopes.pop();
                    }
                    result = Some(match (result.take(), &bt) {
                        (None, _) => bt,
                        (Some(prev), Ty::Void) => prev,
                        (Some(prev), _) => {
                            if prev == Ty::Void {
                                bt
                            } else {
                                type_join(&prev, &bt)
                            }
                        }
                    });
                }
                // 穷尽性检查（仅当有明确类型且无 _ 通配时）
                if let Some(msg) = check_match_exhaustive(&st, arms, self) {
                    return Err(crate::lb!(e.line, "{}", "{}", msg));
                }
                result.unwrap_or(Ty::Void)
            }
            ExprKind::Borrow { mutable, inner } => {
                let it = self.infer(inner)?;
                if *mutable { Ty::RefMut(Box::new(it)) } else { Ty::Ref(Box::new(it)) }
            }
            ExprKind::ClosureNew { fn_name, captures } => {
                for c in captures.iter_mut() {
                    self.infer(c)?;
                }
                // 闭包类型取自提升后的函数签名（captures + params）
                let (params, ret) = match self.fns.get(fn_name) {
                    Some(sig) => (sig.params.clone(), sig.ret.clone()),
                    None => (vec![Ty::I64; captures.len()], Ty::I64),
                };
                Ty::Closure(params, Box::new(if ret == Ty::Void { Ty::I64 } else { ret }))
            }
            ExprKind::CallValue { callee, args } => {
                let ct = self.infer(callee)?;
                for a in args.iter_mut() {
                    self.infer(a)?;
                }
                match ct {
                    Ty::Closure(_, ret) => (*ret).clone(),
                    Ty::Unknown => Ty::I64,
                    other => {
                        return Err(crate::lb!(e.line, "{} is not callable", "{} 不是可调用的闭包", other))
                    }
                }
            }
            ExprKind::Closure { params, param_tys, ret_ty, body, line } => {
                // 闭包：参数类型可用标注覆盖，未标注则从 body 推断
                self.scopes.push(HashMap::new());
                for (i, p) in params.iter().enumerate() {
                    let ty = param_tys.get(i).cloned().flatten().unwrap_or(Ty::Unknown);
                    self.scopes.last_mut().unwrap().insert(p.clone(), VarInfo { ty, mutable: true, explicit: false });
                }
                let bt = self.infer(body)?;
                self.scopes.pop();
                let ptypes: Vec<Ty> = params.iter().enumerate().map(|(i, _)| param_tys.get(i).cloned().flatten().unwrap_or(Ty::Unknown)).collect();
                let _ = line;
                let rt = ret_ty.clone().unwrap_or(if bt == Ty::Void { Ty::I64 } else { bt });
                Ty::Closure(ptypes, Box::new(rt))
            }
            ExprKind::StructLit(name, fields) => {
                let declared = match self.structs.get(name) {
                    Some(fs) => fs.clone(),
                    None => {
                        let base = crate::error::msg::undefined_type(e.line, name).render();
                        let mut cands: Vec<String> = self.structs.keys().cloned().collect();
                        cands.extend(self.enums.keys().cloned());
                        cands.extend(self.trait_methods.keys().cloned());
                        let hint = closest_of(name, cands.into_iter()).map(|s| if crate::lang::is_zh() { format!("\x01是否想用 '{}'？", s) } else { format!("\x01did you mean '{}'?", s) }).unwrap_or_default();
                        return Err(format!("{}{}", base, hint));
                    }
                };
                // 每个提供的字段：类型必须兼容
                for (fname, v) in fields.iter_mut() {
                    let vt = self.infer(v)?;
                    match declared.iter().find(|(n, _)| n == fname) {
                        None => {
                            {
                                let base = crate::error::msg::no_such_field(e.line, name, fname).render();
                                let hint = declared.iter().find_map(|_| None).or_else(|| closest_of(fname, declared.iter().map(|(n, _)| n.clone()))).map(|s| if crate::lang::is_zh() { format!("\x01是否想用 '{}'？", s) } else { format!("\x01did you mean '{}'?", s) }).unwrap_or_default();
                                return Err(format!("{}{}", base, hint));
                            }
                        }
                        Some((_, want)) => {
                            if !compatible(want, &vt) {
                                return Err(crate::lb!(e.line, "field '{}.{}' must be {}, found {}", "字段 '{}.{}' 应为 {}，实际是 {}", name, fname, want, vt));
                            }
                        }
                    }
                }
                // 新增：检查是否遗漏了必填字段
                let missing: Vec<&str> = declared
                    .iter()
                    .filter(|(n, _)| !fields.iter().any(|(f, _)| f == n))
                    .map(|(n, _)| n.as_str())
                    .collect();
                if !missing.is_empty() {
                    return Err(crate::error::CompileError::new(
                        crate::error::ErrorCode::BadStructLit,
                        e.line,
                        format!("结构体 '{}' 缺少字段：{}", name, missing.join("、")),
                    ).with_hint("补齐所有字段，或用默认值").render());
                }
                Ty::Struct(name.clone())
            }
        };
        e.ty = ty.clone();
        Ok(ty)
    }
}

// ============================================================
// 辅助
// ============================================================

/// 类型规则统一来自 `type.rs`（单一事实来源，两个后端共用）
use crate::types::{
    binary_result, builtin_ret, check_annotation, is_assignable, numeric_join, unary_result,
};

/// `actual` 能否赋给 `expected`
/// 识别 `Ok(v)` / `Err(e)` / `Some(v)` 模式，返回 (构造名, 绑定变量名)。
fn match_result_bind(pat: Option<&Expr>) -> Option<(String, String)> {
    if let Some(p) = pat {
        // 返回 (构造名, 绑定变量名)
        let (name, inner): (String, &Expr) = match &p.kind {
            ExprKind::Ok(a) => ("Ok".to_string(), a),
            ExprKind::Err(a) => ("Err".to_string(), a),
            ExprKind::Some(a) => ("Some".to_string(), a),
            // `Ok(v)` 在 parser 中是 Call("Ok", [Ident])
            ExprKind::Call(n, args) if matches!(n.as_str(), "Ok" | "Err" | "Some") && args.len() == 1 => (n.clone(), &args[0]),
            _ => return None,
        };
        if let ExprKind::Ident(b) = &inner.kind {
            return Some((name, b.clone()));
        }
    }
    None
}

fn compatible(expected: &Ty, actual: &Ty) -> bool {
    is_assignable(expected, actual)
}

/// 不依赖完整类型检查的语法级类型猜测（用于形参推断）
fn guess(e: &Expr, consts: &HashMap<String, ConstVal>) -> Option<Ty> {
    match &e.kind {
        ExprKind::Int(_) => Some(Ty::I64),
        ExprKind::Float(_) => Some(Ty::F64),
        ExprKind::Bool(_) => Some(Ty::Bool),
        ExprKind::Str(_) | ExprKind::Interp(_) => Some(Ty::Str),
        ExprKind::ArrayLit(items) => {
            let first = items.first()?;
            Some(Ty::Array(Box::new(guess(first, consts)?), items.len()))
        }
        ExprKind::ClosureNew { .. } => Some(Ty::Closure(Vec::new(), Box::new(Ty::Unknown))),
        ExprKind::Closure { param_tys, ret_ty, .. } => Some(Ty::Closure(param_tys.iter().map(|t| t.clone().unwrap_or(Ty::Unknown)).collect(), Box::new(ret_ty.clone().unwrap_or(Ty::Unknown)))),
        ExprKind::Ident(n) => consts.get(n).map(|c| c.ty.clone()),
        ExprKind::Unary(UnOp::Neg, a) => guess(a, consts),
        ExprKind::Unary(UnOp::BitNot, _) => Some(Ty::I64),
        ExprKind::Binary(op, a, b) if op.is_bit() => Some(Ty::I64),
        ExprKind::Binary(op, a, b) if !op.is_cmp() && !op.is_logic() => {
            let at = guess(a, consts);
            let bt = guess(b, consts);
            match (at, bt) {
                (Some(Ty::F64), _) | (_, Some(Ty::F64)) => Some(Ty::F64),
                (Some(t), _) => Some(t),
                (_, Some(t)) => Some(t),
                _ => None,
            }
        }
        _ => None,
    }
}

/// 常量求值
fn eval_const(e: &Expr, consts: &HashMap<String, ConstVal>) -> Result<(Ty, Value), String> {
    match &e.kind {
        ExprKind::Int(v) => Ok((Ty::I64, Value::Int(*v))),
        ExprKind::Float(v) => Ok((Ty::F64, Value::Float(*v))),
        ExprKind::Bool(v) => Ok((Ty::Bool, Value::Bool(*v))),
        ExprKind::Str(s) => Ok((Ty::Str, Value::Str(s.clone()))),
        ExprKind::Ident(n) => consts
            .get(n)
            .map(|c| (c.ty.clone(), c.val.clone()))
            .ok_or_else(|| format!("未定义的常量 '{}'", n)),
        ExprKind::Unary(UnOp::Neg, a) => match eval_const(a, consts)? {
            (Ty::I64, Value::Int(v)) => Ok((Ty::I64, Value::Int(-v))),
            (Ty::F64, Value::Float(v)) => Ok((Ty::F64, Value::Float(-v))),
            _ => Err("一元 '-' 不能用于该常量".into()),
        },
        ExprKind::Unary(UnOp::BitNot, a) => match eval_const(a, consts)? {
            (Ty::I64, Value::Int(v)) => Ok((Ty::I64, Value::Int(!v))),
            _ => Err("一元 '~' 只能用于整数常量".into()),
        },
        ExprKind::Binary(op, a, b) => {
            let (at, av) = eval_const(a, consts)?;
            let (bt, bv) = eval_const(b, consts)?;
            let as_f64 = at.is_float() || bt.is_float();
            match (av, bv) {
                (Value::Int(x), Value::Int(y)) if !as_f64 => {
                    let r = match op {
                        BinOp::Add => x.wrapping_add(y),
                        BinOp::Sub => x.wrapping_sub(y),
                        BinOp::Mul => x.wrapping_mul(y),
                        BinOp::Div | BinOp::FloorDiv => {
                            if y == 0 {
                                return Err("除数为 0".into());
                            }
                            x / y
                        }
                        BinOp::Rem => {
                            if y == 0 {
                                return Err("除数为 0".into());
                            }
                            x % y
                        }
                        BinOp::BitAnd => x & y,
                        BinOp::BitOr => x | y,
                        BinOp::BitXor => x ^ y,
                        BinOp::Shl => x.wrapping_shl(y as u32),
                        BinOp::Shr => x.wrapping_shr(y as u32),
                        BinOp::Eq => return Ok((Ty::Bool, Value::Bool(x == y))),
                        BinOp::Ne => return Ok((Ty::Bool, Value::Bool(x != y))),
                        BinOp::Lt => return Ok((Ty::Bool, Value::Bool(x < y))),
                        BinOp::Le => return Ok((Ty::Bool, Value::Bool(x <= y))),
                        BinOp::Gt => return Ok((Ty::Bool, Value::Bool(x > y))),
                        BinOp::Ge => return Ok((Ty::Bool, Value::Bool(x >= y))),
                        BinOp::And | BinOp::Or => return Err("逻辑运算不参与常量折叠".into()),
                    };
                    Ok((Ty::I64, Value::Int(r)))
                }
                (a2, b2) => {
                    let x = as_num(&a2)?;
                    let y = as_num(&b2)?;
                    let r = match op {
                        BinOp::Add => x + y,
                        BinOp::Sub => x - y,
                        BinOp::Mul => x * y,
                        BinOp::Div => x / y,
                        BinOp::FloorDiv => (x / y).floor(),
                        BinOp::Rem => x % y,
                        BinOp::Eq => return Ok((Ty::Bool, Value::Bool(x == y))),
                        BinOp::Ne => return Ok((Ty::Bool, Value::Bool(x != y))),
                        BinOp::Lt => return Ok((Ty::Bool, Value::Bool(x < y))),
                        BinOp::Le => return Ok((Ty::Bool, Value::Bool(x <= y))),
                        BinOp::Gt => return Ok((Ty::Bool, Value::Bool(x > y))),
                        BinOp::Ge => return Ok((Ty::Bool, Value::Bool(x >= y))),
                        BinOp::And | BinOp::Or => return Err("逻辑运算不参与常量折叠".into()),
                        // 位运算要求整数，浮点常量走到这里说明类型检查已拦下
                        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                            return Err("位运算不参与浮点常量折叠".into())
                        }
                    };
                    Ok((Ty::F64, Value::Float(r)))
                }
            }
        }
        _ => Err("该表达式不是编译期常量".into()),
    }
}

fn as_num(v: &Value) -> Result<f64, String> {
    match v {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        _ => Err("不是数值".into()),
    }
}

// ============================================================
// AST 遍历
// ============================================================

fn each_expr_block(b: &Block, f: &mut impl FnMut(&Expr)) {
    for s in b {
        each_expr_stmt(s, f);
    }
}

fn each_expr_stmt(s: &Stmt, f: &mut impl FnMut(&Expr)) {
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

fn each_expr(e: &Expr, f: &mut impl FnMut(&Expr)) {
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
        _ => {}
    }
}

/// match 穷尽性检查：返回 None 表示穷尽，Some(消息) 表示不穷尽。
/// 规则：有 _ 通配 → 穷尽；bool 需 true+false；enum 需全部变体；
///       Option 需 Some+None；Result 需 Ok+Err；整数/字符串 → 必须有 _（否则不穷尽）。
fn check_match_exhaustive(st: &Ty, arms: &[MatchArm], ctx: &Ctx) -> Option<String> {
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
        Ty::Enum(name) => {
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
            if has_value { Some(if zh { "match 不穷尽：非枚举主体需用 _ 兜底".into() } else { "match not exhaustive: non-enum subject needs _".into() }) } else { None }
        }
        _ => None,
    }
}

/// 通用类型 join：相同→同；Unknown→另一个；数值→提升；否则 Unknown。
fn type_join(a: &Ty, b: &Ty) -> Ty {
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
fn closest_name(name: &str, ctx: &Ctx) -> Option<String> {
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
fn levenshtein(a: &str, b: &str) -> usize {
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
fn closest_fn(name: &str, ctx: &Ctx) -> Option<String> {
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
fn closest_of<'a, I: Iterator<Item = String>>(name: &str, cands: I) -> Option<String> {
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
