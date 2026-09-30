//! GTLang 的统一编译/解释库（对外 API 入口）。
//!
//! 架构：
//! ```text
//!   源码文本
//!      │  lexer（词法）
//!      ▼
//!   Token 流
//!      │  parser（语法）
//!      ▼
//!   统一 AST  ── sema 类型检查/推断（规则来自 type.rs）
//!      │
//!      ├─ codegen：AST → LLVM IR → clang → 可执行文件（编译器后端）
//!      └─ jit    ：AST → Cranelift 机器码 → 内存中直接执行（解释器后端）
//! ```
//!
//! 两个后端共用同一份 AST 与同一套类型规则（`type.rs`），因此对同一输入结果一致。
//!
//! # 用法
//! ```text
//! use gtc_rust::{build, Unit};
//!
//! let src = "fn main() { put(\"hello\") }";
//! let unit: Unit = build("demo.gt", src).expect("前端通过（检查已随此步完成）");
//!
//! unit.interpret().unwrap();          // 解释执行（Cranelift）
//! let ir = unit.emit_llvm().unwrap(); // 生成 LLVM IR 文本
//! unit.compile_to("demo.exe".as_ref(), 2).unwrap(); // 编译为可执行文件
//! ```

// ---------- 编译器内部模块 ----------
pub mod ast;
pub mod cblock;
pub mod codegen;
pub mod diag;
pub mod driver;
pub mod encoding;
pub mod error;
pub mod frontend;
pub mod hoist;
pub mod jit;
pub mod lang;
pub mod lint;
pub mod lexer;
pub mod mono;
pub mod module;
pub mod opt;
pub mod own;
pub mod parser;
pub mod parser_recover;
pub mod range;
pub mod sema;
pub mod tcc;
pub mod tmp;
pub mod stdlib;
pub mod unit;
pub mod unify;

#[path = "type.rs"]
pub mod types;

// ---------- 对外 API 重导出 ----------
pub use ast::{Program, Span};
pub use diag::{extract_line, strip_line_prefix, Diag, Stage};
pub use error::{hint_for, infer_code, CompileError, ErrorCode};

/// 关闭整数溢出检查（编译后端）。
pub fn disable_overflow_check() {
    codegen::set_overflow_check(false);
}

/// 全局详细日志开关（`--verbose`）。
pub fn set_verbose(on: bool) {
    lang::set_verbose(on);
}

/// 是否处于 verbose 模式。
pub fn verbose() -> bool {
    lang::verbose()
}
pub use frontend::{abs_of, build, build_file, load_sources, read_source};
pub use sema::Analysis;
pub use tmp::TempDir;
pub use types::Ty;
pub use unit::Unit;
