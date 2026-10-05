//! 编译期错误：稳定错误码、统一的诊断消息构造，以及少量辅助检查。
//!
//! 设计目标：
//!   - **单一来源**：所有面向用户的错误消息文本集中在 `msg` 模块，避免各阶段
//!     各写一份、措辞不一。
//!   - **稳定错误码**：每个错误类别有 `E0xx` 编号，便于测试断言、IDE 提示与文档。
//!   - **可扩展**：新增检查只需在 `ErrorCode` 加一项、在 `msg` 加一个构造函数。
//!
//! 位置信息沿用现有的「第 N 行：」前缀格式（`diag.rs` 依赖它定位源码区间）。

/// 针对常见类型不匹配给出"用什么转换"的具体建议。
/// 只在能给出**明确可执行**的修法时返回，避免泛泛而谈。
pub fn conversion_hint(want: &str, got: &str) -> Option<String> {
    let zh = crate::lang::is_zh();
    // 提取基础类型关键词（类型显示里带中文，如 "整数(i64)" / "浮点(f64)" / "字符串(str)"）
    let has = |s: &str, k: &str| s.contains(k);
    let want_str = has(want, "str") || has(want, "字符串");
    let want_int = has(want, "i64") || has(want, "整数");
    let want_f64 = has(want, "f64") || has(want, "浮点");
    let got_str = has(got, "str") || has(got, "字符串");
    let got_int = has(got, "i64") || has(got, "整数");
    let got_f64 = has(got, "f64") || has(got, "浮点");
    let h: String = if want_str && (got_int || got_f64) {
        if zh { "用 str(x) 转为字符串".into() } else { "convert with str(x)".into() }
    } else if want_int && got_str {
        if zh { "用 int(x) 把字符串转为整数".into() } else { "convert with int(x)".into() }
    } else if want_f64 && got_str {
        if zh { "用 f64(x) 把字符串转为浮点".into() } else { "convert with f64(x)".into() }
    } else if want_f64 && got_int {
        if zh { "用 f64(x) 做整数→浮点提升".into() } else { "widen with f64(x)".into() }
    } else if want_int && got_f64 {
        if zh { "用 int(x) 做浮点→整数截断".into() } else { "truncate with int(x)".into() }
    } else if has(want, "bool") && (got_int || got_f64) {
        if zh { "用 bool(x) 转为布尔".into() } else { "convert with bool(x)".into() }
    } else {
        return None;
    };
    Some(h)
}

/// 按消息内容推断修复建议（hint）。
pub fn hint_for(msg: &str) -> Option<String> {
    let zh = crate::lang::is_zh();
    // 算术/位运算的"类型不匹配"：从消息里的 `found X and Y` 提取实际类型，给具体转换建议
    if msg.contains("arithmetic requires numbers") || msg.contains("算术运算需要数值") {
        let hint = if msg.contains("字符串") || msg.contains("str") {
            if zh { "字符串不能直接参与算术；先用 int(x) 或 f64(x) 转换" } else { "strings don't do arithmetic; convert with int(x) or f64(x)" }
        } else {
            if zh { "算术运算的两侧都应为数值（int/f64）" } else { "both operands must be numeric (int/f64)" }
        };
        return Some(hint.to_string());
    }
    if msg.contains("bitwise operation requires an integer") || msg.contains("位运算需要整数") {
        return Some(if zh { "位运算只支持整数；浮点请用 int(x) 转换" } else { "bitwise ops need integers; convert floats with int(x)" }.to_string());
    }
    if msg.contains("condition must be bool") || msg.contains("条件应为布尔") {
        return Some(if zh { "条件应为 bool；比较请用 ==/!=/</>，或用 bool(x) 显式转换" } else { "condition must be bool; compare with ==/!=/</>, or convert with bool(x)" }.to_string());
    }
    if msg.contains("未定义的变量") || msg.contains("undefined variable") {
        Some(if zh { "检查拼写，或先用 := 声明" } else { "check spelling, or declare with :=" })
    } else if msg.contains("未定义的函数") || msg.contains("undefined function") {
        Some(if zh { "检查函数名，或确认已定义/导入" } else { "check the name, or ensure it is defined/imported" })
    } else if msg.contains("未定义的结构体") || msg.contains("undefined struct") {
        Some(if zh { "确认结构体已定义或已导入" } else { "ensure the struct is defined/imported" })
    } else if msg.contains("不可变") || msg.contains("immutable") {
        Some(if zh { "用 := 或 let mut 声明可变变量" } else { "declare with := or let mut" })
    } else if msg.contains("类型") || msg.contains("type") || msg.contains("赋给") || msg.contains("expected") {
        Some(if zh { "检查类型标注，或用显式转换（int()/f64()/str()）" } else { "check the type annotation, or use an explicit conversion" })
    } else if msg.contains("下标") || msg.contains("index") {
        Some(if zh { "下标应为整数且在范围内" } else { "index must be an integer within bounds" })
    } else if msg.contains("参数") || msg.contains("argument") {
        Some(if zh { "检查实参个数与类型" } else { "check argument count and types" })
    } else {
        None
    }.map(|s| s.to_string())
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
        let w = want.to_string();
        let g = got.to_string();
        let m = if crate::lang::is_zh() {
            format!("{}：期望 {}，实际是 {}", ctx, w, g)
        } else {
            format!("{}: expected {}, found {}", ctx, w, g)
        };
        let e = CompileError::new(ErrorCode::TypeMismatch, line, m);
        if let Some(h) = conversion_hint(&w, &g) {
            e.with_hint(h)
        } else {
            e
        }
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

    /// 同 `arity_mismatch`，但附带"函数原型"（形参类型列表），帮助用户对照。
    pub fn arity_mismatch_sig(line: usize, name: &str, want: usize, got: usize, params: &[crate::ast::Ty]) -> CompileError {
        let zh = crate::lang::is_zh();
        let sig = params.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", ");
        let m = if zh {
            format!("函数 '{}' 需要 {} 个参数，实际传入 {}", name, want, got)
        } else {
            format!("function '{}' expects {} argument(s), got {}", name, want, got)
        };
        let hint = if want == params.len() && !params.is_empty() {
            Some(if zh { format!("原型：fn {}({})", name, sig) } else { format!("signature: fn {}({})", name, sig) })
        } else {
            None
        };
        let e = CompileError::new(ErrorCode::ArityMismatch, line, m);
        match hint { Some(h) => e.with_hint(h), None => e }
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

/// 错误码说明（`gtc --explain EXXX`）。返回 (标题, 含义, 常见原因, 修法)。
pub fn explain(code: &str) -> Option<(&'static str, String, String, String)> {
    use ErrorCode::*;
    let c = match code.to_ascii_uppercase().as_str() {
        "E001" => Lex, "E002" => Syntax,
        "E101" => UndefinedVar, "E102" => UndefinedFn, "E103" => UndefinedType,
        "E201" => TypeMismatch, "E202" => AssignMismatch, "E203" => BadOperand,
        "E204" => ImmutableAssign, "E205" => NonBoolCondition, "E206" => CannotInfer,
        "E301" => MissingReturn, "E302" => ReturnOutsideFn, "E303" => LoopControlOutside, "E304" => UnreachableCode,
        "E401" => IndexNotInt, "E402" => NotIndexable, "E403" => IndexAssignImmutable,
        "E404" => NotIterable, "E405" => BadBuiltinCall,
        "E501" => NotAStruct, "E502" => NoSuchField, "E503" => BadStructLit, "E504" => NonExhaustiveMatch,
        "E601" => ArityMismatch, "E602" => ArgTypeMismatch, "E603" => NotCallable,
        "E701" => ModuleNotFound, "E702" => ModuleCycle, "E703" => ModulePrivate,
        "E801" => UseAfterMove, "E802" => BorrowConflict,
        _ => return None,
    };
    let zh = crate::lang::is_zh();
    let (title, meaning, cause, fix) = match c {
        Lex => ("词法错误", "源码中出现无法识别的字符或记号的写法错误。", "非法字符、未闭合的字符串/注释。", "检查该行的字符与引号是否配对。"),
        Syntax => ("语法错误", "代码结构不符合语法规则。", "缺少括号/分隔符、关键字拼错、表达式不完整。", "对照报错位置补齐语法元素。"),
        UndefinedVar => ("未定义的变量", "使用了未声明（或不在作用域内）的变量。", "拼写错误，或忘了用 := / let 声明。", "检查拼写；先声明再用（:= 自动声明）。"),
        UndefinedFn => ("未定义的函数", "调用了不存在的函数。", "名字拼错、未定义、或未 import。", "检查名字；定义它或用 import 引入。"),
        UndefinedType => ("未定义的类型", "使用了不存在的结构体/类型名。", "名字拼错，或未定义/未导入。", "确认类型已定义或已导入。"),
        TypeMismatch => ("类型不匹配", "表达式类型与期望类型不一致。", "赋值、传参、返回时类型不符；混用 str 与数值。", "用 int()/f64()/str() 显式转换，或修正类型标注。"),
        AssignMismatch => ("赋值类型错误", "赋值的类型与变量声明的类型不兼容。", "给固定类型变量赋了别的类型。", "修正值或变量类型。"),
        BadOperand => ("运算符类型错误", "运算符用在了不支持的类型上。", "对字符串做算术、对非整数做位运算等。", "转换为数值，或改用字符串拼接。"),
        ImmutableAssign => ("不可变赋值", "试图修改不可变变量或常量。", "用 let 声明的变量、const 常量不可改。", "用 := 或 let mut 声明可变变量。"),
        NonBoolCondition => ("条件非布尔", "if/while 的条件不是 bool。", "条件写了数值或字符串。", "用比较运算符，或 bool(x) 转换。"),
        CannotInfer => ("无法推断类型", "编译器无法确定表达式类型。", "缺少标注或上下文信息不足。", "补上类型标注。"),
        MissingReturn => ("缺少返回值", "函数声明了返回类型却没有返回值。", "分支未覆盖、忘了 return。", "确保所有路径都返回值。"),
        ReturnOutsideFn => ("return 位置错误", "在函数外使用 return。", "顶层或错误位置写了 return。", "把 return 放进函数体。"),
        LoopControlOutside => ("循环控制位置错误", "break/continue 不在循环内。", "在循环外使用。", "移到循环体内。"),
        UnreachableCode => ("不可达代码", "该语句永远不会执行。", "return/break 之后还有代码。", "删除或调整。"),
        IndexNotInt => ("下标非整数", "下标不是整数类型。", "用了字符串/浮点做下标。", "下标用整数（int）。"),
        NotIndexable => ("不支持下标", "对该类型使用了 []。", "对非容器/非字符串取下标。", "确认对象可下标；或改用字段访问。"),
        IndexAssignImmutable => ("下标赋值不可变", "对不可变容器的元素赋值。", "容器本身不可变。", "用可变容器（:= / let mut）。"),
        NotIterable => ("不可遍历", "for 遍历了不支持的类型。", "遍历了非容器。", "遍历 list/set/map/str/数组。"),
        BadBuiltinCall => ("内置函数调用错误", "内置函数的参数个数或类型不对。", "传错参数。", "对照内置函数签名调整。"),
        NotAStruct => ("不是结构体", "对非结构体访问字段。", "对普通值用了 .字段。", "确认对象是结构体。"),
        NoSuchField => ("没有该字段", "结构体不存在该字段。", "字段名拼错。", "检查字段名（编译器会给拼写建议）。"),
        BadStructLit => ("结构体字面量错误", "构造结构体时字段缺失或多余。", "字段不匹配。", "补齐所有字段。"),
        NonExhaustiveMatch => ("match 不穷尽", "match 未覆盖所有情况。", "枚举缺变体、bool 缺 true/false。", "补全分支或用 _ 兜底。"),
        ArityMismatch => ("参数个数不匹配", "调用时参数个数与定义不符。", "多传或少传。", "对照函数原型调整（编译器会给出签名）。"),
        ArgTypeMismatch => ("参数类型不匹配", "某个参数的类型不对。", "传错类型。", "转换为期望类型。"),
        NotCallable => ("不可调用", "对非函数/闭包的值做了调用。", "把变量当函数调用。", "确认被调用的值是可调用的。"),
        ModuleNotFound => ("找不到模块", "import 的文件不存在。", "路径错。", "检查 import 路径。"),
        ModuleCycle => ("模块循环", "import 形成环。", "互相导入。", "打破循环依赖。"),
        ModulePrivate => ("模块私有", "访问了未导出的项。", "项未公开。", "导出该名字。"),
        UseAfterMove => ("移动后使用", "值被移动（所有权转移）后又使用。", "把值传给了会消耗它的地方。", "重新绑定，或避免移动。"),
        BorrowConflict => ("借用冲突", "同时存在不兼容的借用。", "& 与 &mut 冲突、多个 &mut。", "缩短借用作用域，或改用值语义。"),
        Other => ("其他错误", "未分类的错误。", "—", "看具体消息。"),
    };
    let _ = zh;
    let mut fix = fix.to_string();
    let rel = related_codes(c.code());
    if !rel.is_empty() {
        fix.push_str(&format!("\n  相关错误码：{}", rel.join("、")));
    }
    Some((title, meaning.to_string(), cause.to_string(), fix))
}

/// 返回与给定错误码常一起出现的相关码（用于 `--explain`）。
fn related_codes(code: &str) -> Vec<&'static str> {
    match code {
        "E201" => vec!["E602", "E202"],
        "E202" => vec!["E201", "E204"],
        "E204" => vec!["E202", "E801"],
        "E101" => vec!["E102", "E103"],
        "E102" => vec!["E101", "E601"],
        "E601" => vec!["E602", "E102"],
        "E602" => vec!["E601", "E201"],
        "E401" => vec!["E402"],
        "E402" => vec!["E401", "E404"],
        "E404" => vec!["E402"],
        "E501" => vec!["E502"],
        "E502" => vec!["E501", "E503"],
        "E503" => vec!["E502"],
        "E504" => vec!["E502"],
        "E801" => vec!["E802", "E204"],
        "E802" => vec!["E801"],
        "E701" => vec!["E702", "E703"],
        "E702" => vec!["E701"],
        "E703" => vec!["E701"],
        _ => vec![],
    }
}

#[path = "tests/error.rs"]
#[cfg(test)]
mod error_tests;
