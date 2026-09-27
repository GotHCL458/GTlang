//! 语法错误恢复：多错误报告。
//!
//! 目标：`gtc --check/--run/--c` 遇到语法错误时**收集全部**，而非首错即停。
//!
//! 策略（panic-mode recovery）：单个顶层声明解析失败后，**跳到下一个顶层声明**
//! 的起始关键字（`fn`/`struct`/`enum`/`trait`/`impl`/`macro`/`const`/`import`/`extern`），
//! 或跳到文件末尾，然后继续解析。

use crate::lexer::Tok;

/// 顶层声明起始关键字（用于同步点）。
pub const ITEM_STARTS: &[&str] = &[
    "fn", "struct", "enum", "trait", "impl", "macro", "const", "import", "extern", "pub",
];

/// 判断某 token 是否是顶层声明起始。
pub fn is_item_start(tok: &Tok) -> bool {
    matches!(tok, Tok::Ident(n) if ITEM_STARTS.contains(&n.as_str()))
}
