//! 错误码 / 提示 / 诊断构造的单元测试。
#![cfg(test)]

use super::{infer_code, conversion_hint, hint_for, explain, ErrorCode};

#[test]
fn infer_code_for_names() {
    assert_eq!(infer_code("未定义的变量 'x'"), Some("E101"));
    assert_eq!(infer_code("undefined variable 'x'"), Some("E101"));
    assert_eq!(infer_code("未定义的函数 'f'"), Some("E102"));
    assert_eq!(infer_code("未定义的结构体 'P'"), Some("E103"));
}

#[test]
fn infer_code_for_types() {
    assert_eq!(infer_code("类型不匹配"), Some("E201"));
    assert_eq!(infer_code("不可变变量"), Some("E204"));
    assert_eq!(infer_code("条件应为布尔"), Some("E205"));
}

#[test]
fn infer_code_for_containers_and_control() {
    assert_eq!(infer_code("下标越界"), Some("E402"));
    assert_eq!(infer_code("不是结构体"), Some("E501"));
    assert_eq!(infer_code("没有字段 'x'"), Some("E502"));
    assert_eq!(infer_code("返回值缺失"), Some("E301"));
    assert_eq!(infer_code("参数个数不对"), Some("E602"));
}

#[test]
fn infer_code_unknown_returns_none() {
    assert_eq!(infer_code("完全无关的消息"), None);
    assert_eq!(infer_code(""), None);
}

#[test]
fn conversion_hints_present_for_scalar_mismatches() {
    // int → str
    assert!(conversion_hint("字符串(str)", "整数(i64)").is_some());
    // str → int
    assert!(conversion_hint("整数(i64)", "字符串(str)").is_some());
    // int → f64
    assert!(conversion_hint("浮点(f64)", "整数(i64)").is_some());
    // 不兼容类型 → None
    assert!(conversion_hint("列表", "结构体").is_none());
}

#[test]
fn hint_for_known_errors() {
    assert!(hint_for("未定义的变量 'x'").is_some());
    assert!(hint_for("undefined function 'f'").is_some());
    assert!(hint_for("条件应为布尔").is_some());
    // 未知 → None
    assert!(hint_for("某些很怪的消息 xyz").is_none());
}

#[test]
fn error_codes_are_stable_and_unique() {
    let all = [
        ErrorCode::Lex, ErrorCode::Syntax, ErrorCode::UndefinedVar,
        ErrorCode::UndefinedFn, ErrorCode::UndefinedType, ErrorCode::TypeMismatch,
        ErrorCode::AssignMismatch, ErrorCode::BadOperand, ErrorCode::ImmutableAssign,
        ErrorCode::NonBoolCondition, ErrorCode::MissingReturn, ErrorCode::LoopControlOutside,
        ErrorCode::IndexNotInt, ErrorCode::NotIndexable, ErrorCode::NotAStruct,
        ErrorCode::NoSuchField, ErrorCode::NonExhaustiveMatch, ErrorCode::ArityMismatch,
        ErrorCode::ArgTypeMismatch, ErrorCode::NotCallable, ErrorCode::Other,
    ];
    let mut seen = std::collections::HashSet::new();
    for c in all {
        let code = c.code();
        assert!(code.starts_with("E"), "bad code {}", code);
        assert!(seen.insert(code), "duplicate code {}", code);
        // label 非空
        assert!(!c.label().is_empty());
    }
}

#[test]
fn specific_code_values() {
    assert_eq!(ErrorCode::Lex.code(), "E001");
    assert_eq!(ErrorCode::Syntax.code(), "E002");
    assert_eq!(ErrorCode::UndefinedVar.code(), "E101");
    assert_eq!(ErrorCode::TypeMismatch.code(), "E201");
    assert_eq!(ErrorCode::LoopControlOutside.code(), "E303");
    assert_eq!(ErrorCode::UseAfterMove.code(), "E801");
}

#[test]
fn explain_covers_known_codes() {
    // 已知错误码应能给出 (title, meaning, causes, fix) 四元组
    for code in ["E001", "E101", "E201", "E303", "E801"] {
        assert!(explain(code).is_some(), "no explain for {}", code);
    }
    assert!(explain("E999").is_none());
}
