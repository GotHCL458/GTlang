//! 诊断类型与源码位置工具。
//!
//! 编译/解释的任何阶段都产出 `Diag`：统一携带阶段、消息与源码字节区间，
//! 便于 CLI（ariadne）或 IDE 渲染。

use crate::ast::Span;

/// 出错阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// 词法阶段
    Lex,
    /// 语法阶段
    Parse,
    /// 语义/类型阶段
    Type,
}

impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Stage::Lex | Stage::Parse => crate::lang::tr("syntax error", "语法错误"),
            Stage::Type => crate::lang::tr("type error", "类型错误"),
        }
    }
}

/// 一条诊断：统一携带阶段、消息与源码字节区间，便于外部工具渲染
#[derive(Debug, Clone)]
pub struct Diag {
    pub stage: Stage,
    pub file: String,
    pub message: String,
    pub span: Span,
    /// 相关位置备注：(标签, 区间)
    pub notes: Vec<(String, Span)>,
}

impl std::fmt::Display for Diag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}：{}（{}）", self.stage.label(), self.message, self.file)
    }
}

/// 从「第 N 行」消息中定位到该行首个非空白字符的字节区间
pub fn span_of_line(text: &str, msg: &str) -> Span {
    let line = match extract_line(msg) {
        Some(l) => l,
        None => return Span::default(),
    };
    let mut start = 0usize;
    for _ in 1..line {
        match text[start..].find('\n') {
            Some(i) => start += i + 1,
            None => break,
        }
    }
    let end = text[start..]
        .find('\n')
        .map(|i| start + i)
        .unwrap_or(text.len());
    let content_start = text[start..end]
        .find(|c: char| !c.is_whitespace())
        .map(|i| start + i)
        .unwrap_or(start);
    Span::new(
        content_start.min(text.len()),
        end.max(content_start + 1).min(text.len().max(1)),
    )
}

/// 从消息里抽取行号：支持中文「第 N 行：」与英文「line N: 」两种前缀。
pub fn extract_line(msg: &str) -> Option<usize> {
    if let Some(rest) = msg.strip_prefix("第 ") {
        let end = rest.find(' ')?;
        return rest[..end].parse::<usize>().ok();
    }
    if let Some(rest) = msg.strip_prefix("line ") {
        let end = rest.find(':')?;
        return rest[..end].trim().parse::<usize>().ok();
    }
    None
}

/// 去掉消息开头的行号前缀（位置已由诊断片段表达）。
/// 兼容中文「第 N 行：」与英文「line N: 」。
pub fn strip_line_prefix(msg: &str) -> &str {
    if msg.starts_with('第') {
        if let Some(i) = msg.find('：') {
            return &msg[i + '：'.len_utf8()..];
        }
    }
    if let Some(rest) = msg.strip_prefix("line ") {
        if let Some(i) = rest.find(':') {
            return rest[i + 1..].trim_start();
        }
    }
    msg
}

#[path = "diag_tests.rs"]
#[cfg(test)]
mod diag_tests;
