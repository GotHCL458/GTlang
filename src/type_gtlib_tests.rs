//! 标准库函数表的单元测试（符号映射 + 签名正确性）。
#![cfg(test)]

use super::{gtlib_fn, is_builtin_name};
use crate::types::Ty;

#[test]
fn math_signatures() {
    let sqrt = gtlib_fn("sqrt").expect("sqrt");
    assert_eq!(sqrt.symbol, "py_sqrt");
    assert_eq!(sqrt.ret, Ty::F64);
    assert_eq!(sqrt.params, &[Ty::F64]);

    let pow = gtlib_fn("pow").expect("pow");
    assert_eq!(pow.symbol, "py_pow");
    assert_eq!(pow.params, &[Ty::F64, Ty::F64]);

    let round = gtlib_fn("round").expect("round");
    assert_eq!(round.ret, Ty::I64);
}

#[test]
fn string_signatures() {
    let strip = gtlib_fn("strip").expect("strip");
    assert_eq!(strip.symbol, "py_strip");
    assert_eq!(strip.ret, Ty::Str);
    let split = gtlib_fn("split_str").expect("split_str");
    assert_eq!(split.ret, Ty::Str);
    // upper 是内建（is_builtin_name），不在 gtlib_fn 表
    assert!(gtlib_fn("upper").is_none());
    assert!(is_builtin_name("upper"));
}

#[test]
fn json_and_os_signatures() {
    assert!(gtlib_fn("json_dumps").is_some());
    assert!(gtlib_fn("json_loads").is_some());
    assert!(gtlib_fn("getcwd").is_some());
    assert!(gtlib_fn("toml_loads").is_some());
    assert!(gtlib_fn("tcp_connect").is_some());
}

#[test]
fn unknown_func_is_none() {
    assert!(gtlib_fn("绝对不存在").is_none());
    assert!(gtlib_fn("").is_none());
    assert!(gtlib_fn("not_a_stdlib_fn_xyz").is_none());
}

#[test]
fn builtin_names_recognized() {
    // put/len/str 等是内置名
    assert!(is_builtin_name("put"));
    assert!(is_builtin_name("len"));
    assert!(is_builtin_name("str"));
    // 标准库函数也算"已知名"（供"忘了加 ()"提示）
    assert!(is_builtin_name("sqrt"));
    // 完全不存在的名字不算
    assert!(!is_builtin_name("绝对不存在_xyz"));
}

#[test]
fn symbols_are_non_empty_for_known_funcs() {
    for name in ["sqrt", "strip", "json_dumps", "getcwd", "toml_loads"] {
        let f = gtlib_fn(name).expect(name);
        assert!(!f.symbol.is_empty(), "empty symbol for {}", name);
    }
}
