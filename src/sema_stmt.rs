//! sema 的语句检查：块 / 各类语句 / 条件。
//!
//! 从 sema.rs 拆出（原文件超 50KB）。

use super::*;

pub(crate) fn check_block(ctx: &mut Ctx, b: &mut Block, errors: &mut Vec<String>) {
    // 不引入新作用域：GTLang 的块作用域不严格（解构块依赖此）
    for s in b.iter_mut() {
        check_stmt(ctx, s, errors);
    }
}

pub(crate) fn check_stmt(ctx: &mut Ctx, s: &mut Stmt, errors: &mut Vec<String>) {
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
                                let kt = (**k).clone();
                                let vt0 = (**v).clone();
                                if let Err(e) = ctx.infer(idx) {
                                    errors.push(e);
                                } else if kt != Ty::Unknown && idx.ty != Ty::Unknown && !is_assignable(&kt, &idx.ty) {
                                    errors.push(crate::lb!(line, "map key must be {}, found {}", "map 键应为 {}，实际是 {}", kt, idx.ty));
                                }
                                let it = ctx.infer(value).unwrap_or(Ty::Unknown);
                                // map 的键/值类型为 Unknown 时按首次赋值细化（供 codegen/jit 判断 f64 等）。
                                if let Some(vi) = ctx.lookup_var_mut(name) {
                                    if let Ty::Map(bk, bv) = &vi.ty {
                                        let mut nk = (**bk).clone();
                                        let mut nv = (**bv).clone();
                                        if nk == Ty::Unknown && idx.ty != Ty::Unknown { nk = idx.ty.clone(); }
                                        if nv == Ty::Unknown && it != Ty::Unknown { nv = it.clone(); }
                                        vi.ty = Ty::Map(Box::new(nk), Box::new(nv));
                                    }
                                }
                                let vt = if vt0 == Ty::Unknown && it != Ty::Unknown { it } else { vt0 };
                                (vt, format!("'{}[]'", name))
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
                                // 纯赋值 x = v：允许改变变量类型（但 RHS 类型未知时保留原类型，
                                // 否则 `acc = f(...)`（f 无标注、返回 Unknown）会把 acc 改成 i64）
                                if rty == Ty::Unknown {
                                    // RHS 类型未知：保留变量原类型
                                } else {
                                    let new_ty = rty.clone();
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
                                }
                            } else if !compatible(&lt, &rty) {
                                errors.push(crate::lb!(line, "cannot assign {} to {}{}", "无法把 {} 赋给 {}{}",
                                    rty, target,
                                    if lt == Ty::Unknown { String::new() } else { format!("({})", lt) }));
                            }
                            // m[k] = v：用键/值类型细化 map 的元素类型（供后续 for-in 遍历推断）
                            if let Some(idx) = index {
                                let kt = idx.ty.clone();
                                if let Some(entry) = ctx.scopes.iter_mut().rev().find_map(|sc| sc.get_mut(name)) {
                                    if let Ty::Map(ek, ev) = &entry.ty {
                                        let nk = if matches!(**ek, Ty::Unknown) && kt != Ty::Unknown { kt.clone() } else { (**ek).clone() };
                                        let nv = if matches!(**ev, Ty::Unknown) && rty != Ty::Unknown { rty.clone() } else { (**ev).clone() };
                                        entry.ty = Ty::Map(Box::new(nk), Box::new(nv));
                                    }
                                }
                            }
                        }
                        Err(e) => errors.push(e),
                    }
                }
            }
        }
        Stmt::FieldAssign { obj, field, op, value, line } => {
            // 借用自动解引用：`a := &mut p; a.x = v` 视作 `p.x = v`。
            let obj_ty = ctx.lookup(obj).map(|t| match t {
                Ty::Ref(inner) | Ty::RefMut(inner) => *inner,
                other => other,
            });
            match obj_ty {
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
                Ok(mut t) => {
                    // 若 iter 是变量引用，用变量表里（可能被 push/m[k]=v 细化过的）最新类型
                    // 覆盖 expr.ty —— 让 codegen/jit 也能拿到精确的元素类型。语义等价。
                    if let ExprKind::Ident(n) = &iter.kind {
                        if let Some(vt) = ctx.lookup(n) {
                            if !matches!(vt, Ty::Unknown) { t = vt.clone(); iter.ty = vt; }
                        }
                    }
                    let elem = match &t {
                        Ty::Array(el, _) => (**el).clone(),
                        Ty::List(el) => (**el).clone(),
                        Ty::Str => Ty::Str,
                        Ty::Set(el) => (**el).clone(),
                        Ty::Map(k, _) => (**k).clone(),
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
                            // impl Trait 返回位置：具体类型自动装箱为 dyn Trait，不报类型错
                            let auto_box = matches!(want, Ty::Dyn(_)) && matches!(t, Ty::Struct(_) | Ty::Enum(_));
                            if !auto_box {
                            if let Err(why) = check_annotation("函数返回值", &want, &t) {
                                errors.push(crate::lb!(line, "{}", "{}", why));
                            }
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

pub(crate) fn check_cond(ctx: &mut Ctx, cond: &mut Expr, line: &usize, errors: &mut Vec<String>) {
    match ctx.infer(cond) {
        Ok(t) => {
            if t != Ty::Bool && t != Ty::Unknown {
                errors.push(crate::lb!(line, "condition must be bool, found {}", "条件应为布尔(bool)，实际是 {}", t));
            }
        }
        Err(e) => errors.push(e),
    }
}

