//! 全局语言开关与消息本地化。
//!
//! 约定：**默认英文**；命令行末尾带 `zh` 参数时切换为中文。
//! 所有面向用户的消息都应经由此处选择文案，避免散落的中文字面量。

use std::sync::atomic::{AtomicBool, Ordering};

/// 是否中文（默认英文 = false）
static ZH: AtomicBool = AtomicBool::new(false);

/// 切换到中文
pub fn set_zh() {
    ZH.store(true, Ordering::Relaxed);
}

/// 当前是否中文
pub fn is_zh() -> bool {
    ZH.load(Ordering::Relaxed)
}

/// 详细日志开关（`--verbose`）
static VERBOSE: AtomicBool = AtomicBool::new(false);

pub fn set_verbose(on: bool) {
    VERBOSE.store(on, Ordering::Relaxed);
}

pub fn verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

/// 二选一：英文 / 中文（返回选中的那个）
pub fn tr<'a>(en: &'a str, zh: &'a str) -> &'a str {
    if is_zh() { zh } else { en }
}

/// 行号前缀：`第 N 行：`（中文）或 `line N: `（英文）。
/// `diag.rs` 的两套解析都识别这两种前缀。
pub fn line_prefix(line: usize) -> String {
    if is_zh() {
        format!("第 {} 行：", line)
    } else {
        format!("line {}: ", line)
    }
}

/// 构造一条带行号前缀的**双语**消息。
///
/// `en`/`zh` 是不含行号前缀的格式串（其余 `{}` 占位参数两语言一致）。
/// 行号自动作为第一个参数注入。
///
/// 用法：`lb!(line, "undefined variable '{}'", "未定义的变量 '{}'", name)`
/// 构造一条**双语**消息（无行号前缀）。
///
/// `en`/`zh` 是格式串，其余占位参数两语言一致。
#[macro_export]
macro_rules! te {
    ($en:literal, $zh:literal $(, $arg:expr)*) => {{
        if $crate::lang::is_zh() {
            format!($zh $(, $arg)*)
        } else {
            format!($en $(, $arg)*)
        }
    }};
}

#[macro_export]
macro_rules! lb {
    ($line:expr, $en:literal, $zh:literal $(, $arg:expr)*) => {{
        let m = if $crate::lang::is_zh() {
            format!(concat!("第 {} 行：", $zh), $line $(, $arg)*)
        } else {
            format!(concat!("line {}: ", $en), $line $(, $arg)*)
        };
        // 按消息内容推断错误码（E0xx/E1xx/...），附加到消息末尾
        let code = $crate::error::infer_code(&m);
        match code {
            Some(c) => format!("{m} ({c})"),
            None => m,
        }
    }};
}
