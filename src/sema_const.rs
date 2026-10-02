//! sema 的常量求值与语法级类型猜测。
//!
//! 从 sema.rs 拆出（原文件超 50KB）。

use super::*;

pub(crate) fn match_result_bind(pat: Option<&Expr>) -> Option<(String, String)> {
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

pub(crate) fn compatible(expected: &Ty, actual: &Ty) -> bool {
    is_assignable(expected, actual)
}

/// 不依赖完整类型检查的语法级类型猜测（用于形参推断）
pub(crate) fn guess(e: &Expr, consts: &HashMap<String, ConstVal>) -> Option<Ty> {
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
pub(crate) fn eval_const(e: &Expr, consts: &HashMap<String, ConstVal>) -> Result<(Ty, Value), String> {
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

