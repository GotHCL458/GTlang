//! 编译期错误：稳定错误码、统一的诊断消息构造，以及少量辅助检查。
//!
//! 设计目标：
//!   - **单一来源**：所有面向用户的错误消息文本集中在 `msg` 模块，避免各阶段
//!     各写一份、措辞不一。
//!   - **稳定错误码**：每个错误类别有 `E0xx` 编号，便于测试断言、IDE 提示与文档。
//!   - **可扩展**：新增检查只需在 `ErrorCode` 加一项、在 `msg` 加一个构造函数。
//!
//! 位置信息沿用现有的「第 N 行：」前缀格式（`diag.rs` 依赖它定位源码区间）。

/// 按消息内容推断修复建议（hint）。
pub fn hint_for(msg: &str) -> Option<&'static str> {
    if msg.contains("未定义的变量") || msg.contains("undefined variable") {
        Some(if crate::lang::is_zh() { "检查拼写，或先用 := 声明" } else { "check spelling, or declare with :=" })
    } else if msg.contains("未定义的函数") || msg.contains("undefined function") {
        Some(if crate::lang::is_zh() { "检查函数名，或确认已定义/导入" } else { "check the name, or ensure it is defined/imported" })
    } else if msg.contains("未定义的结构体") || msg.contains("undefined struct") {
        Some(if crate::lang::is_zh() { "确认结构体已定义或已导入" } else { "ensure the struct is defined/imported" })
    } else if msg.contains("不可变") || msg.contains("immutable") {
        Some(if crate::lang::is_zh() { "用 := 或 let mut 声明可变变量" } else { "declare with := or let mut" })
    } else if msg.contains("类型") || msg.contains("type") || msg.contains("赋给") || msg.contains("expected") {
        Some(if crate::lang::is_zh() { "检查类型标注，或用显式转换（int()/f64()/str()）" } else { "check the type annotation, or use an explicit conversion" })
    } else if msg.contains("下标") || msg.contains("index") {
        Some(if crate::lang::is_zh() { "下标应为整数且在范围内" } else { "index must be an integer within bounds" })
    } else if msg.contains("参数") || msg.contains("argument") {
        Some(if crate::lang::is_zh() { "检查实参个数与类型" } else { "check argument count and types" })
    } else {
        None
    }
}

/// 按消息内容推断错误码（供 sema 的 `lb!` 自动附加）。
/// 覆盖常见类别；未匹配返回 None。
pub fn infer_code(msg: &str) -> Option<&'static str> {
    let m = msg;
    let c = if m.contains("未定义的变量") || m.contains("undefined variable") { ErrorCode::UndefinedVar }
    else if m.contains("未定义的函数") || m.contains("undefined function") { ErrorCode::UndefinedFn }
    else if m.contains("未定义的结构体") || m.contains("undefined struct") { ErrorCode::UndefinedType }
    else if m.contains("类型") || m.contains("type") || m.contains("期望") || m.contains("expected") || m.contains("赋给") || m.contains("assign") { ErrorCode::TypeMismatch }
    else if m.contains("不可变") || m.contains("immutable") { ErrorCode::ImmutableAssign }
    else if m.contains("条件应为布尔") || m.contains("condition must be bool") { ErrorCode::NonBoolCondition }
    else if m.contains("下标") || m.contains("index") || m.contains("不支持下标") { ErrorCode::NotIndexable }
    else if m.contains("不是结构体") || m.contains("not a struct") { ErrorCode::NotAStruct }
    else if m.contains("没有字段") || m.contains("has no field") { ErrorCode::NoSuchField }
    else if m.contains("参数") || m.contains("argument") { ErrorCode::ArgTypeMismatch }
    else if m.contains("返回值") || m.contains("return value") { ErrorCode::MissingReturn }
    else if m.contains("循环") || m.contains("loop") { ErrorCode::LoopControlOutside }
    else if m.contains("函数") || m.contains("function") { ErrorCode::UndefinedFn }
    else if m.contains("不穷尽") || m.contains("not exhaustive") { ErrorCode::NonExhaustiveMatch }
    else if m.contains("溢出") || m.contains("overflow") { ErrorCode::BadOperand }
    else { return None };
    Some(c.code())
}

/// 编译期错误类别（稳定编号）。
///
/// 编号约定：
///   - E0xx：词法/语法
///   - E1xx：名字解析
///   - E2xx：类型
///   - E3xx：控制流/返回
///   - E4xx：容器/下标
///   - E5xx：结构体/字段
///   - E6xx：函数/调用
///   - E7xx：模块/导入
///   - E8xx：所有权/借用
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    // ---- E0xx 词法/语法 ----
    /// 词法错误（非法字面量、未闭合字符串等）
    Lex,
    /// 语法错误（期望某符号/表达式）
    Syntax,

    // ---- E1xx 名字解析 ----
    /// 使用了未定义的变量
    UndefinedVar,
    /// 调用了未定义的函数
    UndefinedFn,
    /// 使用了未定义的类型/结构体
    UndefinedType,

    // ---- E2xx 类型 ----
    /// 类型不匹配
    TypeMismatch,
    /// 赋值类型不兼容
    AssignMismatch,
    /// 运算符不支持该类型
    BadOperand,
    /// 常量不可变却试图赋值
    ImmutableAssign,
    /// 条件表达式不是布尔
    NonBoolCondition,
    /// 无法从类型推断（需要标注）
    CannotInfer,

    // ---- E3xx 控制流/返回 ----
    /// 函数缺少返回值
    MissingReturn,
    /// return 出现在函数外
    ReturnOutsideFn,
    /// break/continue 出现在循环外
    LoopControlOutside,
    /// 函数体内含顶层 return 的尾表达式缺失
    UnreachableCode,

    // ---- E4xx 容器/下标 ----
    /// 下标不是整数
    IndexNotInt,
    /// 类型不支持下标访问
    NotIndexable,
    /// 对不可变容器做元素赋值
    IndexAssignImmutable,
    /// 数组/容器遍历类型错误
    NotIterable,
    /// 内置函数参数个数/类型错误
    BadBuiltinCall,

    // ---- E5xx 结构体/字段 ----
    /// 类型不是结构体
    NotAStruct,
    /// 结构体没有该字段
    NoSuchField,
    /// 结构体字面量缺少字段 / 未知字段
    BadStructLit,

    // ---- E6xx 函数/调用 ----
    /// 参数个数不符
    ArityMismatch,
    /// 实参类型不符
    ArgTypeMismatch,
    /// 调用不可调用对象
    NotCallable,

    // ---- E7xx 模块/导入 ----
    /// 找不到模块
    ModuleNotFound,
    /// 模块循环依赖
    ModuleCycle,
    /// 访问了模块未导出的符号
    ModulePrivate,

    // ---- E8xx 所有权/借用 ----
    /// 使用了已移动的值
    UseAfterMove,
    /// 借用冲突
    BorrowConflict,

    // ---- 其它 ----
    /// match 不穷尽
    NonExhaustiveMatch,
    /// 其它/未归类
    Other,
}

impl ErrorCode {
    /// 稳定的错误码字符串（如 `"E101"`）。
    pub fn code(self) -> &'static str {
        use ErrorCode::*;
        match self {
            Lex => "E001",
            Syntax => "E002",
            UndefinedVar => "E101",
            UndefinedFn => "E102",
            UndefinedType => "E103",
            TypeMismatch => "E201",
            AssignMismatch => "E202",
            BadOperand => "E203",
            ImmutableAssign => "E204",
            NonBoolCondition => "E205",
            CannotInfer => "E206",
            MissingReturn => "E301",
            ReturnOutsideFn => "E302",
            LoopControlOutside => "E303",
            UnreachableCode => "E304",
            IndexNotInt => "E401",
            NotIndexable => "E402",
            IndexAssignImmutable => "E403",
            NotIterable => "E404",
            BadBuiltinCall => "E405",
            NotAStruct => "E501",
            NoSuchField => "E502",
            BadStructLit => "E503",
            NonExhaustiveMatch => "E504",
            ArityMismatch => "E601",
            ArgTypeMismatch => "E602",
            NotCallable => "E603",
            ModuleNotFound => "E701",
            ModuleCycle => "E702",
            ModulePrivate => "E703",
            UseAfterMove => "E801",
            BorrowConflict => "E802",
            Other => "E000",
        }
    }

    /// 人类可读的类别标签（用于诊断标题）。
    pub fn label(self) -> &'static str {
        use ErrorCode::*;
        match self {
            Lex | Syntax => "语法错误",
            UndefinedVar | UndefinedFn | UndefinedType => "名字错误",
            TypeMismatch | AssignMismatch | BadOperand | ImmutableAssign
            | NonBoolCondition | CannotInfer => "类型错误",
            MissingReturn | ReturnOutsideFn | LoopControlOutside | UnreachableCode => "控制流错误",
            IndexNotInt | NotIndexable | IndexAssignImmutable | NotIterable
            | BadBuiltinCall => "容器错误",
            NotAStruct | NoSuchField | BadStructLit | NonExhaustiveMatch => "结构体错误",
            ArityMismatch | ArgTypeMismatch | NotCallable => "调用错误",
            ModuleNotFound | ModuleCycle | ModulePrivate => "模块错误",
            UseAfterMove | BorrowConflict => "所有权错误",
            Other => "错误",
        }
    }
}

/// 一条结构化编译期错误：错误码 + 行号 + 消息（不含行前缀）+ 可选提示。
#[derive(Debug, Clone)]
pub struct CompileError {
    pub code: ErrorCode,
    pub line: usize,
    pub message: String,
    /// 可选修复建议（渲染为「提示：…」）
    pub hint: Option<String>,
}

impl CompileError {
    pub fn new(code: ErrorCode, line: usize, message: impl Into<String>) -> CompileError {
        CompileError { code, line, message: message.into(), hint: None }
    }

    /// 附加修复建议。
    pub fn with_hint(mut self, hint: impl Into<String>) -> CompileError {
        self.hint = Some(hint.into());
        self
    }

    /// 渲染为带行号前缀的字符串。
    ///
    /// 前缀格式对中英文都可被 `diag.rs` 的行号提取解析：
    ///   - 英文：`line N: msg (E0xx)`
    ///   - 中文：`第 N 行：msg（E0xx）`
    pub fn render(&self) -> String {
        let zh = crate::lang::is_zh();
        match (&self.hint, zh) {
            (Some(h), true) => format!("第 {} 行：{}（{}；提示：{}）", self.line, self.message, self.code.code(), h),
            (None, true) => format!("第 {} 行：{}（{}）", self.line, self.message, self.code.code()),
            (Some(h), false) => format!("line {}: {} ({}; hint: {})", self.line, self.message, self.code.code(), h),
            (None, false) => format!("line {}: {} ({})", self.line, self.message, self.code.code()),
        }
    }
}

/// 统一的错误消息构造。集中所有面向用户的措辞，便于一致性与本地化。
/// 默认英文；`lang::is_zh()` 为真时输出中文。
pub mod msg {
    use super::*;
    use crate::lang::tr;

    pub fn type_mismatch(line: usize, ctx: &str, want: impl std::fmt::Display, got: impl std::fmt::Display) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("{}：期望 {}，实际是 {}", ctx, want, got)
        } else {
            format!("{}: expected {}, found {}", ctx, want, got)
        };
        CompileError::new(ErrorCode::TypeMismatch, line, m)
    }

    pub fn undefined_var(line: usize, name: &str) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("未定义的变量 '{}'", name)
        } else {
            format!("undefined variable '{}'", name)
        };
        CompileError::new(ErrorCode::UndefinedVar, line, m)
            .with_hint(tr("declare it first with :=", "检查拼写，或先用 := 声明"))
    }

    pub fn undefined_fn(line: usize, name: &str) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("未定义的函数 '{}'", name)
        } else {
            format!("undefined function '{}'", name)
        };
        CompileError::new(ErrorCode::UndefinedFn, line, m)
    }

    pub fn undefined_type(line: usize, name: &str) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("未定义的结构体 '{}'", name)
        } else {
            format!("undefined struct '{}'", name)
        };
        CompileError::new(ErrorCode::UndefinedType, line, m)
    }

    pub fn immutable_assign(line: usize, name: &str) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("不能给不可变变量或常量 '{}' 赋值", name)
        } else {
            format!("cannot assign to immutable variable or constant '{}'", name)
        };
        CompileError::new(ErrorCode::ImmutableAssign, line, m)
            .with_hint(tr("declare with := or let mut", "用 := 或 let mut 声明可变变量"))
    }

    pub fn non_bool_condition(line: usize, got: impl std::fmt::Display) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("条件应为布尔(bool)，实际是 {}", got)
        } else {
            format!("condition must be bool, found {}", got)
        };
        CompileError::new(ErrorCode::NonBoolCondition, line, m)
    }

    pub fn index_not_int(line: usize, got: impl std::fmt::Display) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("下标应为整数，实际是 {}", got)
        } else {
            format!("index must be an integer, found {}", got)
        };
        CompileError::new(ErrorCode::IndexNotInt, line, m)
    }

    pub fn not_indexable(line: usize, ty: impl std::fmt::Display) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("{} 不支持下标访问", ty)
        } else {
            format!("{} does not support indexing", ty)
        };
        CompileError::new(ErrorCode::NotIndexable, line, m)
    }

    pub fn not_iterable(line: usize, ty: impl std::fmt::Display) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("for 只能遍历数组/列表，实际是 {}", ty)
        } else {
            format!("for can only iterate over arrays/lists, found {}", ty)
        };
        CompileError::new(ErrorCode::NotIterable, line, m)
    }

    pub fn not_a_struct(line: usize, ty: impl std::fmt::Display) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("{} 不是结构体，不能访问字段", ty)
        } else {
            format!("{} is not a struct; cannot access field", ty)
        };
        CompileError::new(ErrorCode::NotAStruct, line, m)
    }

    pub fn no_such_field(line: usize, sname: &str, field: &str) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("结构体 '{}' 没有字段 '{}'", sname, field)
        } else {
            format!("struct '{}' has no field '{}'", sname, field)
        };
        CompileError::new(ErrorCode::NoSuchField, line, m)
    }

    pub fn arity_mismatch(line: usize, name: &str, want: usize, got: usize) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("函数 '{}' 需要 {} 个参数，实际传入 {}", name, want, got)
        } else {
            format!("function '{}' expects {} argument(s), got {}", name, want, got)
        };
        CompileError::new(ErrorCode::ArityMismatch, line, m)
    }

    pub fn arg_type_mismatch(line: usize, name: &str, idx: usize, want: impl std::fmt::Display, got: impl std::fmt::Display) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("调用 '{}' 的第 {} 个参数应为 {}，实际是 {}", name, idx + 1, want, got)
        } else {
            format!("argument {} of '{}' must be {}, found {}", idx + 1, name, want, got)
        };
        CompileError::new(ErrorCode::ArgTypeMismatch, line, m)
    }

    pub fn not_callable(line: usize, ty: impl std::fmt::Display) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("{} 不是可调用的闭包", ty)
        } else {
            format!("{} is not callable", ty)
        };
        CompileError::new(ErrorCode::NotCallable, line, m)
    }

    pub fn missing_return(line: usize, want: impl std::fmt::Display) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("函数应返回 {}，但缺少返回值", want)
        } else {
            format!("function should return {}, but is missing a return value", want)
        };
        CompileError::new(ErrorCode::MissingReturn, line, m)
    }

    pub fn loop_control_outside(line: usize, kw: &str) -> CompileError {
        let m = if crate::lang::is_zh() {
            format!("'{}' 只能出现在循环内", kw)
        } else {
            format!("'{}' can only appear inside a loop", kw)
        };
        CompileError::new(ErrorCode::LoopControlOutside, line, m)
    }

    pub fn syntax(line: usize, msg: impl Into<String>) -> CompileError {
        CompileError::new(ErrorCode::Syntax, line, msg.into())
    }

    pub fn other(line: usize, code: ErrorCode, msg: impl Into<String>) -> CompileError {
        CompileError::new(code, line, msg.into())
    }
}
