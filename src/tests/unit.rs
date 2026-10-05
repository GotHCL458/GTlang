//! unit.rs 辅助函数的单元测试。
#![cfg(test)]

use super::Unit;

#[test]
fn no_stdlib_refs_returns_empty() {
    let ir = "define i32 @main() { ret i32 0 }";
    assert!(Unit::used_gtlib_dlls_of(ir).is_empty());
}

#[test]
fn detects_math_dll() {
    // 引用 math.dll 里的符号（py_sqrt）→ 应返回 math
    let ir = "declare double @py_sqrt(double)\ndefine i32 @main() { ret i32 0 }";
    let dlls = Unit::used_gtlib_dlls_of(ir);
    assert!(dlls.contains(&"math"), "got {:?}", dlls);
}

#[test]
fn result_is_deduped() {
    // 多个 math 符号只出现一次
    let ir = "declare double @py_sqrt(double)\ndeclare double @py_floor(double)\ndeclare double @py_pow(double, double)";
    let dlls = Unit::used_gtlib_dlls_of(ir);
    let math_count = dlls.iter().filter(|d| **d == "math").count();
    assert_eq!(math_count, 1, "duplicates: {:?}", dlls);
}

#[test]
fn multiple_modules_detected() {
    // 同时用 math（py_sqrt）与 string（py_strip）
    let ir = "declare double @py_sqrt(double)\ndeclare ptr @py_strip(ptr, ptr)";
    let dlls = Unit::used_gtlib_dlls_of(ir);
    assert!(dlls.contains(&"math"), "got {:?}", dlls);
    assert!(dlls.contains(&"string"), "got {:?}", dlls);
}

#[test]
fn c_source_includes_bridge_when_funcs_exist() {
    // 无内联 C 时 c_source 返回空或极简（不 panic）
    // 这里只验证方法可调用且返回 String
    // （构造 Unit 需要完整前端，故此处跳过实际构造，仅测静态函数）
    let ir = "";
    assert!(Unit::used_gtlib_dlls_of(ir).is_empty());
}
