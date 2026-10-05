//! 诊断位置工具的单元测试。
#![cfg(test)]

use super::{extract_line, span_of_line, strip_line_prefix, Stage};

#[test]
fn extract_line_chinese_and_english() {
    assert_eq!(extract_line("第 5 行：类型错误"), Some(5));
    assert_eq!(extract_line("第 12 行：x"), Some(12));
    assert_eq!(extract_line("line 7: type error"), Some(7));
    assert_eq!(extract_line("line 100 : ok"), Some(100));
    assert_eq!(extract_line("no prefix here"), None);
    assert_eq!(extract_line(""), None);
}

#[test]
fn strip_line_prefix_removes_prefix() {
    assert_eq!(strip_line_prefix("第 5 行：类型错误"), "类型错误");
    assert_eq!(strip_line_prefix("line 7: type error"), "type error");
    // 无前缀原样返回
    assert_eq!(strip_line_prefix("普通消息"), "普通消息");
}

#[test]
fn span_of_line_finds_content() {
    let text = "fn main() {\n  put(1)\n}\n";
    let sp = span_of_line(text, "第 2 行：x");
    // 应指向第 2 行首个非空白字符 "put"
    assert!(sp.start > 0 && sp.start < text.len());
    assert!(sp.end > sp.start);
    // 无行号 → 默认空 span
    assert_eq!(span_of_line(text, "no line"), crate::ast::Span::default());
}

#[test]
fn span_of_line_aligns_char_boundaries() {
    // 多字节字符（中文）行：span 必须落在字符边界上
    let text = "fn main() {\n  计数器 := 1\n}\n";
    let sp = span_of_line(text, "第 2 行：x");
    assert!(text.is_char_boundary(sp.start), "start not on boundary");
    assert!(text.is_char_boundary(sp.end), "end not on boundary");
    // 行内全中文内容
    let text2 = "甲\n乙\n";
    let sp2 = span_of_line(text2, "第 2 行：x");
    assert!(text2.is_char_boundary(sp2.start));
    assert!(text2.is_char_boundary(sp2.end));
}

#[test]
fn stage_labels_non_empty() {
    assert!(!Stage::Lex.label().is_empty());
    assert!(!Stage::Parse.label().is_empty());
    assert!(!Stage::Type.label().is_empty());
    // Lex 与 Parse 同为"语法错误"
    assert_eq!(Stage::Lex.label(), Stage::Parse.label());
    assert_ne!(Stage::Lex.label(), Stage::Type.label());
}
