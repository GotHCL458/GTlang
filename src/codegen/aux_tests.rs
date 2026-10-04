//! codegen 辅助函数的单元测试。
#![cfg(test)]

use super::*;
use crate::types::Ty;

#[test]
fn mangle_is_ascii_and_hex_for_non_ascii() {
    assert_eq!(mangle("add"), "add");
    assert_eq!(mangle("a_b1"), "a_b1");
    // 非 ASCII（中文）逐字节转 _xHH
    let m = mangle("累加");
    assert!(m.starts_with("_x"), "got {}", m);
    assert!(m.is_ascii());
    // "加" 的 UTF-8 是 E5 8A A0
    assert_eq!(mangle("加"), "_xE5_x8A_xA0");
}

#[test]
fn mangle_empty_and_symbols() {
    assert_eq!(mangle(""), "");
    // 非字母数字符号 → _xHH
    assert_eq!(mangle("-"), "_x2D");
    assert_eq!(mangle("a-b"), "a_x2Db");
}

#[test]
fn fmt_double_uses_hex_bits() {
    // 1.0 的 IEEE754 位 = 0x3FF0000000000000
    assert_eq!(fmt_double(1.0), "0x3FF0000000000000");
    assert_eq!(fmt_double(0.0), "0x0000000000000000");
    // 负零
    assert_eq!(fmt_double(-0.0), "0x8000000000000000");
}

#[test]
fn fmt_double_special_values() {
    assert_eq!(fmt_double(f64::INFINITY), "0x7FF0000000000000");
    assert_eq!(fmt_double(f64::NEG_INFINITY), "0xFFF0000000000000");
    assert_eq!(fmt_double(f64::NAN), "0x7FF8000000000000");
}

#[test]
fn elem_is_ptr_for_heap_types() {
    assert_eq!(elem_is_ptr(&Ty::I64), 0);
    assert_eq!(elem_is_ptr(&Ty::F64), 0);
    assert_eq!(elem_is_ptr(&Ty::Bool), 0);
    assert_eq!(elem_is_ptr(&Ty::Str), 1);
    assert_eq!(elem_is_ptr(&Ty::List(Box::new(Ty::I64))), 1);
    assert_eq!(elem_is_ptr(&Ty::Set(Box::new(Ty::I64))), 1);
    assert_eq!(elem_is_ptr(&Ty::Map(Box::new(Ty::I64), Box::new(Ty::I64))), 1);
    assert_eq!(elem_is_ptr(&Ty::Struct("S".into())), 1);
    assert_eq!(elem_is_ptr(&Ty::Enum("E".into())), 1);
    assert_eq!(elem_is_ptr(&Ty::Tuple(vec![Ty::I64])), 1);
}

#[test]
fn is_pure_for_literals_and_ops() {
    let lit = |k| Expr::new(k, 0);
    assert!(is_pure(&lit(ExprKind::Int(1))));
    assert!(is_pure(&lit(ExprKind::Ident("x".into()))));
    assert!(is_pure(&lit(ExprKind::Binary(BinOp::Add, Box::new(lit(ExprKind::Int(1))), Box::new(lit(ExprKind::Int(2)))))));
    // 调用有副作用 → 非纯
    assert!(!is_pure(&lit(ExprKind::Call("f".into(), vec![]))));
}
