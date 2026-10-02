//! 语义分析：常量求值、无标注形参推断、表达式类型推断与检查。
//!
//! 推断结果直接回写到 AST（`Expr::ty`、`Param::ty`、`FnDef::ret_ty`），
//! 使代码生成阶段只有一个类型来源。

use std::collections::HashMap;

use crate::ast::*;

#[path = "sema_const.rs"]
mod sema_const;
#[path = "sema_match.rs"]
mod sema_match;
#[path = "sema_stmt.rs"]
mod sema_stmt;
#[path = "sema_util.rs"]
mod sema_util;
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
        ctx.generic_bounds = f.bounds.clone();
        ctx.param_names = f.params.iter().map(|p| p.name.clone()).collect();
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

    /// 推断并回填表达式类型
    fn infer(&mut self, e: &mut Expr) -> Result<Ty, String> {
        let ty = match &mut e.kind {
            ExprKind::Int(_) => Ty::I64,
            ExprKind::Float(_) => Ty::F64,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::TupleLit(items) => { let mut ts = Vec::new(); for it in items.iter_mut() { ts.push(self.infer(it)?); } Ty::Tuple(ts) },

            ExprKind::DynBox { trait_name, value } => { self.infer(value)?; Ty::Dyn(trait_name.clone()) },
            ExprKind::MethodOn { recv, method, args } => {
                let rt = self.infer(recv)?;
                for a in args.iter_mut() { self.infer(a)?; }
                // 查类型的方法返回类型（结构体：fns 里的 类型__方法）
                let ret = match &rt {
                    Ty::Struct(s) => self.fns.get(&format!("{}__{}", s, method)).map(|sig| sig.ret.clone()),
                    Ty::Dyn(tr) => self.trait_methods.get(tr).and_then(|ms| ms.iter().find(|(n, _, _)| n == method).map(|(_, _, r)| r.clone())),
                    Ty::Generic(tp) => self.generic_bounds.iter().find(|(t, _)| t == tp)
                        .and_then(|(_, tr)| self.trait_methods.get(tr))
                        .and_then(|ms| ms.iter().find(|(n, _, _)| n == method).map(|(_, _, r)| r.clone())),
                    _ => None,
                };
                ret.unwrap_or(Ty::Unknown)
            }
            ExprKind::EnumLit(name, variant, args) => {
                // 关联函数 `类型::函数(args)`（不是枚举变体）：查 `类型__函数`
                if !self.enums.contains_key(name) && self.structs.contains_key(name) {
                    let fname = format!("{}__{}", name, variant);
                    if let Some(sig) = self.fns.get(&fname).cloned() {
                        for a in args.iter_mut() { self.infer(a)?; }
                        e.ty = sig.ret.clone();
                        return Ok(sig.ret);
                    }
                }
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
                    let mut hint = match closest_name(n, self) {
                        Some(s) => if crate::lang::is_zh() { format!("\x01是否想用 '{}'？", s) } else { format!("\x01did you mean '{}'?", s) },
                        None => String::new(),
                    };
                    // 名字其实是一个函数/内置函数？提示"忘了加 ()"
                    if hint.is_empty() && (self.fns.contains_key(n) || crate::types::is_builtin_name(n)) {
                        hint = if crate::lang::is_zh() { format!("\x01'{}' 是函数，调用它需要加 ()：{}()", n, n) } else { format!("\x01'{}' is a function; call it with () : {}()", n, n) };
                    }
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
                // web.serve_fn(port, handler)：第二参数是函数名，不做表达式类型检查
                if name == "serve_fn" && args.len() == 2 {
                    let fname = match &args[1].kind {
                        ExprKind::Ident(n) => n.clone(),
                        _ => return Err(crate::lb!(e.line, "serve_fn second arg must be a function name", "serve_fn 第二参数须是函数名")),
                    };
                    if !self.fns.contains_key(&fname) {
                        let keys: Vec<String> = self.fns.keys().cloned().collect();
                        return Err(crate::lb!(e.line, "serve_fn: function '{}' not found; have: {:?}", "serve_fn: 未找到函数 '{}'; 现有: {:?}", fname, keys));
                    }
                    return Ok(Ty::I64);
                }
                // `s.方法(...)`：s 是 dyn Trait 对象 → 查 trait 方法
                if let Some(dot) = name.find('.') {
                    let recv = self.lookup(&name[..dot]);
                    let tr = match recv { Some(Ty::Dyn(tr)) => Some(tr), _ => None };
                    if let Some(tr) = tr {
                        let mname = name[dot+1..].to_string();
                        let ret = self.trait_methods.get(&tr).and_then(|ms| ms.iter().find(|(n, _, _)| *n == mname).map(|(_, _, r)| r.clone()));
                        if let Some(ret) = ret {
                            for a in args.iter_mut().skip(1) { let _ = self.infer(a); }
                            e.ty = ret.clone();
                            return Ok(ret);
                        }
                    }
                }
                // `x.方法(...)`：x 是泛型参数 T 且 T 有 trait 约束 → 查该 trait 的方法
                if let Some(dot) = name.find('.') {
                    let rname = name[..dot].to_string();
                    if let Some(Ty::Generic(tp)) = self.lookup(&rname) {
                        // 找 T 的约束 trait
                        if let Some(tr) = self.generic_bounds.iter().find(|(t, _)| t == &tp).map(|(_, tr)| tr.clone()) {
                            let mname = name[dot+1..].to_string();
                            let ret = self.trait_methods.get(&tr).and_then(|ms| ms.iter().find(|(n, _, _)| *n == mname).map(|(_, _, r)| r.clone()));
                            if let Some(ret) = ret {
                                for a in args.iter_mut() { let _ = self.infer(a); }
                                e.ty = ret.clone();
                                return Ok(ret);
                            }
                        }
                    }
                }
                // `obj.方法(...)`：obj 是结构体变量 → 查方法返回类型
                if let Some(dot) = name.find('.') {
                    let recv = name[..dot].to_string();
                    let mname = name[dot + 1..].to_string();
                    if let Some(Ty::Struct(sty)) = self.lookup(&recv) {
                        if let Some(sig) = self.fns.get(&format!("{}__{}", sty, mname)).cloned() {
                            for a in args.iter_mut() { let _ = self.infer(a); }
                            e.ty = sig.ret.clone();
                            return Ok(sig.ret);
                        }
                    }
                }
                // `obj.方法(...)`：若 obj 是已知类型的变量，而方法名拼错，给出候选建议。
                if let Some(dot) = name.find('.') {
                    let recv = &name[..dot];
                    let mname = &name[dot + 1..];
                    if let Some(Ty::Struct(sty)) = self.lookup(recv) {
                        let prefix = format!("{}__", sty);
                        let cands: Vec<String> = self.fns.keys().filter_map(|k| k.strip_prefix(&prefix).map(|s| s.to_string())).collect();
                        if !cands.iter().any(|c| c == mname) {
                            if let Some(sug) = closest_of(mname, cands.into_iter()) {
                                let base = crate::lb!(e.line, "undefined function '{}'", "未定义的函数 '{}'", name);
                                let hint = if crate::lang::is_zh() { format!("\x01是否想用 '{}.{}'？", recv, sug) } else { format!("\x01did you mean '{}.{}'?", recv, sug) };
                                return Err(format!("{}{}", base, hint));
                            }
                        }
                    }
                }
                // 形参当函数调用（高阶函数）：`f(x)` 中 f 是无标注形参 → 放行
                if !name.contains('.') && self.param_names.contains(&name) {
                    for a in args.iter_mut() { let _ = self.infer(a); }
                    e.ty = Ty::Unknown;
                    return Ok(Ty::Unknown);
                }
                let mut arg_tys = Vec::with_capacity(args.len());
                for a in args.iter_mut() {
                    arg_tys.push(self.infer(a)?);
                }
                // push(l, x)：若 l 是"元素未知的 list 变量"，用 x 的类型细化其元素类型。
                // 这让 codegen/jit 能判断"元素是否为堆指针"，从而正确设置 GC 的 elem_ptr。
                if (name == "push" || name == "append") && args.len() == 2 {
                    if let Some(vty) = arg_tys.get(1).cloned() {
                        match &args[0].kind {
                            ExprKind::Ident(lname) => {
                                if let Some(vi) = self.lookup_var_mut(lname) {
                                    if let Ty::List(e) = &vi.ty {
                                        if matches!(**e, Ty::Unknown) { vi.ty = Ty::List(Box::new(vty)); }
                                    }
                                }
                            }
                            // 字段：`obj.kids`（obj 是 struct）→ 细化该 struct 字段的元素类型
                            ExprKind::Field(base, fname) => {
                                if let ExprKind::Ident(bn) = &base.kind {
                                    if let Some(Ty::Struct(sname)) = self.lookup(bn) {
                                        let vt = vty.clone();
                                        if let Some(fs) = self.structs.get_mut(&sname) {
                                            for (f, ft) in fs.iter_mut() {
                                                if f == fname {
                                                    if let Ty::List(e) = ft {
                                                        if matches!(**e, Ty::Unknown) { *ft = Ty::List(Box::new(vt.clone())); }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    // 细化后回填 receiver 表达式的类型（供 codegen 用）
                    if let ExprKind::Field(base, fname) = &args[0].kind {
                        if let ExprKind::Ident(bn) = &base.kind {
                            if let Some(Ty::Struct(sname)) = self.lookup(bn) {
                                if let Some(ft) = self.structs.get(&sname).and_then(|fs| fs.iter().find(|(n, _)| n == fname).map(|(_, t)| t.clone())) {
                                    args[0].ty = ft;
                                }
                            }
                        }
                    }
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
                    let ty = Ty::Result(Box::new(t), Box::new(er));
                    e.ty = ty.clone();
                    return Ok(ty);
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
                    let ty = Ty::Option(Box::new(t));
                    e.ty = ty.clone();
                    return Ok(ty);
                }
                if name == "None" {
                    if !arg_tys.is_empty() {
                        return Err(crate::lb!(e.line, "None takes no arguments", "None 不接受参数"));
                    }
                    let t = match &self.cur_ret {
                        Ty::Option(ot) => (**ot).clone(),
                        _ => Ty::Unknown,
                    };
                    let ty = Ty::Option(Box::new(t));
                    e.ty = ty.clone();
                    return Ok(ty);
                }
                // 用户函数优先于标准库（用户定义同名函数时遮蔽 gtlib）
                if let Some(sig) = self.fns.get(&name).cloned() {
                    if sig.params.len() != arg_tys.len() {
                        return Err(crate::error::msg::arity_mismatch_sig(
                            e.line, &name, sig.params.len(), arg_tys.len(), &sig.params,
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
                } else if let Some(sf) = crate::types::gtlib_fn(&name) {
                    // 标准库需先 import 对应模块
                    if let Some(dll) = crate::gtlib::dll_of(&name) {
                        if !self.imported_gtlib.iter().any(|m| m == dll) {
                            return Err(crate::lb!(e.line, "module '{}' is not imported", "模块 '{}' 未导入（缺少 import）", dll));
                        }
                    }
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
                            // 先看"用户函数"里最近的名字
                            let mut hint = closest_fn(&name, self).map(|s| if crate::lang::is_zh() { format!("\x01是否想用 '{}'？", s) } else { format!("\x01did you mean '{}'?", s) }).unwrap_or_default();
                            // 再看"标准库"里最近的名字（用户可能忘了它来自 stdlib）
                            if hint.is_empty() {
                                let cands: Vec<String> = crate::jit::symbol::BUILTIN_NAMES.iter().chain(crate::jit::symbol::GTLIB_NAMES.iter()).map(|s| s.to_string()).collect();
                                if let Some(s) = closest_of(&name, cands.into_iter()) {
                                    hint = if crate::lang::is_zh() { format!("\x01是否想用标准库函数 '{}'？（该函数由标准库提供，直接调用即可）", s) } else { format!("\x01did you mean the stdlib function '{}'? (it is provided by the standard library; call it directly)", s) };
                                }
                            }
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
                        {
                            let base = crate::lb!(e.line, "{} does not support indexing", "{} 不支持下标访问", other);
                            if matches!(other, Ty::I64) {
                                return Err(format!("{}{}", base, if crate::lang::is_zh() { "\x01若它是容器/字符串，请为变量或形参显式标注类型（如 list[int] / str）" } else { "\x01if it is a container/string, annotate the variable or parameter type (e.g. list[int] / str)" }));
                            }
                            return Err(base);
                        }
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
                // 闭包类型取自提升后的函数签名（captures + params）。
                // 注意：params 必须包含捕获值——后端靠 len(params) - len(args) 推算捕获个数。
                let (params, ret) = match self.fns.get(fn_name) {
                    Some(sig) => (sig.params.clone(), sig.ret.clone()),
                    None => (vec![Ty::I64; captures.len()], Ty::I64),
                };
                Ty::Closure(params, Box::new(if ret == Ty::Void { Ty::I64 } else { ret }))
            }
            ExprKind::CallValue { callee, args } => {
                // 形参当闭包调用：`f(x)` 中 f 是无标注形参 → 返回类型未知（放行）
                if let ExprKind::Ident(n) = &callee.kind {
                    if self.param_names.contains(n) {
                        for a in args.iter_mut() { let _ = self.infer(a); }
                        e.ty = Ty::Unknown;
                        return Ok(Ty::Unknown);
                    }
                }
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


