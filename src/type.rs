//! GTLang 类型系统（单一事实来源）。
//!
//! 这个模块集中定义：
//!   1. `Ty` —— 语言的类型集合
//!   2. 类型间的**赋值兼容性**与**数值提升**规则
//!   3. 运算符的**结果类型**推导规则
//!   4. 后端需要的类型映射（LLVM / Cranelift）
//!
//! 编译后端（LLVM）与解释后端（Cranelift JIT）都只从本模块取类型规则，
//! 因此二者对同一程序的类型判定完全一致——这是"解释器与编译器结果一致"的基础。

use std::fmt;

use crate::ast::{BinOp, UnOp};

// ============================================================
// 类型集合
// ============================================================

/// GTLang 的静态类型。
///
/// 设计取舍：所有整型（i8/i16/i32/i64/u8/...）统一归一为 `I64`，
/// 先把类型系统与检查逻辑做扎实，位宽细节留待后续按需展开。
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    /// 尚未推断出来（类型检查结束后不应残留）
    Unknown,
    /// 无值
    Void,
    Bool,
    I64,
    F64,
    Str,
    /// 定长数组：元素类型 + 长度（值语义，栈分配）
    Array(Box<Ty>, usize),
    /// 动态列表（引用语义，堆分配，运行时 `gt_list_*`）
    List(Box<Ty>),
    /// 集合（引用语义，堆分配，运行时 `gt_set_*`）
    Set(Box<Ty>),
    /// 映射（引用语义，堆分配，运行时 `gt_map_*`）
    Map(Box<Ty>, Box<Ty>),
    /// 结构体（值语义，栈分配，字段 8 字节槽）
    Struct(String),
    /// 枚举（引用语义：堆块 [tag, payload...]）
    Enum(String),
    /// 泛型类型参数（单态化前存在，单态化后应被替换为具体类型）
    Generic(String),
    /// 闭包：参数类型 + 返回类型（值 = 指向 {fn_ptr, env_ptr} 的指针）
    Closure(Vec<Ty>, Box<Ty>),
    /// 结果类型 `Result[T, E]`：Ok(T) 或 Err(E)（引用语义，堆块 [tag, payload]）
    Result(Box<Ty>, Box<Ty>),
    /// 可选类型 `Option[T]`（语法 `?T`）：Some(T) 或 None（引用语义，堆块 [tag, payload]）
    Option(Box<Ty>),
    /// 元组 `(T1, T2, ...)`（值语义：堆块，字段 8 字节槽）
    Tuple(Vec<Ty>),
    /// trait 对象 `dyn Trait`（胖指针：数据 + vtable，堆块引用语义）
    Dyn(String),
    /// 共享借用 `&T`（可多个共存）
    Ref(Box<Ty>),
    /// 独占借用 `&mut T`（与其他借用互斥）
    RefMut(Box<Ty>),
    /// HM 类型变量（仅未标注参数/泛型体推断时出现；单态化/检查结束后应被解出）
    Var(u32),
}

impl Ty {
    pub fn is_int(&self) -> bool {
        matches!(self, Ty::I64 | Ty::Generic(_))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Ty::F64)
    }

    /// 是否数值（可参与算术）
    pub fn is_num(&self) -> bool {
        self.is_int() || self.is_float() || matches!(self, Ty::Generic(_))
    }

    /// 是否标量（可比较）
    pub fn is_scalar(&self) -> bool {
        matches!(self, Ty::Bool | Ty::I64 | Ty::F64 | Ty::Str)
    }

    /// 后端统一使用的 LLVM IR 类型
    pub fn llvm(&self) -> String {
        match self {
            Ty::Void => "void".into(),
            Ty::Bool => "i1".into(),
            Ty::I64 | Ty::Unknown => "i64".into(),
            Ty::F64 => "double".into(),
            Ty::Str | Ty::Array(..) | Ty::List(..) | Ty::Set(..) | Ty::Map(..) | Ty::Struct(_)
            | Ty::Result(..) | Ty::Option(_) | Ty::Tuple(_) | Ty::Enum(_) | Ty::Dyn(_) => "ptr".into(),
            // 单态化/推断前不应用；给出合理占位
            Ty::Generic(_) | Ty::Var(_) => "i64".into(),
            Ty::Closure(..) | Ty::Ref(_) | Ty::RefMut(_) => "ptr".into(),
        }
    }

    /// LLVM IR 里的零值字面量
    pub fn zero(&self) -> &'static str {
        match self {
            Ty::F64 => "0.0",
            Ty::Str | Ty::Array(..) | Ty::List(..) | Ty::Set(..) | Ty::Map(..)
            | Ty::Struct(_) | Ty::Result(..) | Ty::Option(_) | Ty::Dyn(_) => "null",
            _ => "0",
        }
    }

    /// 从源码里的类型注解名解析
    pub fn from_name(n: &str) -> Option<Ty> {
        Some(match n {
            "void" => Ty::Void,
            "bool" => Ty::Bool,
            "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "char" | "i64" | "u64" | "isize"
            | "usize" | "int" => Ty::I64,
            "f32" | "f64" | "float" => Ty::F64,
            "str" | "string" => Ty::Str,
            "list" => Ty::List(Box::new(Ty::Unknown)),
            "set" => Ty::Set(Box::new(Ty::Unknown)),
            "map" | "dict" => Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)),
            "Result" | "result" => Ty::Result(Box::new(Ty::Unknown), Box::new(Ty::Unknown)),
            "Option" | "option" => Ty::Option(Box::new(Ty::Unknown)),
            _ => return None,
        })
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Unknown => write!(f, "未推断"),
            Ty::Var(id) => write!(f, "?{}", id),
            Ty::Void => write!(f, "空(void)"),
            Ty::Bool => write!(f, "布尔(bool)"),
            Ty::I64 => write!(f, "整数(i64)"),
            Ty::F64 => write!(f, "浮点(f64)"),
            Ty::Str => write!(f, "字符串(str)"),
            Ty::Tuple(ts) => { write!(f, "元组(")?; for (i, t) in ts.iter().enumerate() { if i > 0 { write!(f, ", ")?; } write!(f, "{}", t)?; } write!(f, ")") }
            Ty::Enum(n) => write!(f, "枚举({})", n),
            Ty::Array(e, n) => write!(f, "[{}; {}]", e, n),
            Ty::List(e) => write!(f, "列表(list[{}])", e),
            Ty::Set(e) => write!(f, "集合(set[{}])", e),
            Ty::Map(k, v) => write!(f, "映射(map[{}, {}])", k, v),
            Ty::Result(t, e) => write!(f, "结果(Result[{}, {}])", t, e),
            Ty::Option(t) => write!(f, "可选(?{})", t),
            Ty::Struct(n) => write!(f, "结构体({})", n),
            Ty::Generic(n) => write!(f, "泛型参数({})", n),
            Ty::Closure(ps, r) => {
                write!(f, "闭包(|{}| -> {})", ps.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", "), r)
            }
            Ty::Ref(t) => write!(f, "&{}", t),
            Ty::RefMut(t) => write!(f, "&mut {}", t),
            Ty::Dyn(n) => write!(f, "trait 对象(dyn {})", n),
        }
    }
}

// ============================================================
// 赋值兼容性
// ============================================================

/// `actual` 能否赋给 `expected`（编译期检查的核心判定）。
///
/// 规则：
/// - `Unknown` 双向放行（推断过程中的中间状态）
/// - 同类型直接通过
/// - 整数 → 浮点：允许（隐式放宽，不丢值语义）
/// - 数组：长度与元素类型都要兼容
/// - 其余一律不兼容（浮点 → 整数需显式转换，避免静默截断）
pub fn is_assignable(expected: &Ty, actual: &Ty) -> bool {
    if expected == &Ty::Unknown || actual == &Ty::Unknown {
        return true;
    }
    if expected == actual {
        return true;
    }
    match (expected, actual) {
        (Ty::F64, t) if t.is_int() => true,
        (Ty::Array(a, n), Ty::Array(b, m)) => n == m && is_assignable(a, b),
        // 容器：元素类型兼容即可（含 Unknown 通配）
        (Ty::List(a), Ty::List(b)) => is_assignable(a, b),
        (Ty::Set(a), Ty::Set(b)) => is_assignable(a, b),
        (Ty::Map(k1, v1), Ty::Map(k2, v2)) => is_assignable(k1, k2) && is_assignable(v1, v2),
        (Ty::Result(t1, e1), Ty::Result(t2, e2)) => is_assignable(t1, t2) && is_assignable(e1, e2),
        (Ty::Option(a), Ty::Option(b)) => is_assignable(a, b),
        (Ty::Struct(a), Ty::Struct(b)) => a == b,
        // trait 对象：同名兼容
        (Ty::Dyn(a), Ty::Dyn(b)) => a == b,
        // 结构体/枚举 → dyn Trait：自动装箱（impl Trait for T 在 sema 已校验）
        (Ty::Dyn(_), Ty::Struct(_) | Ty::Enum(_)) => true,
        // 类型标注中的名字无法区分 struct/enum，同名视为兼容（sema 已按实际定义校验字段）
        (Ty::Struct(a), Ty::Enum(b)) | (Ty::Enum(a), Ty::Struct(b)) => a == b,
        (Ty::Enum(a), Ty::Enum(b)) => a == b,
        (Ty::Generic(a), Ty::Generic(b)) => a == b,
        (Ty::Ref(a), Ty::Ref(b)) => is_assignable(a, b),
        (Ty::RefMut(a), Ty::RefMut(b)) => is_assignable(a, b),
        // &mut T 可作 &T 用（共享借用的放宽）
        (Ty::Ref(a), Ty::RefMut(b)) => is_assignable(a, b),
        // 借用自动解引用：&T / &mut T 可赋给 T（方法/字段调用传 self 时用）
        (t, Ty::Ref(b)) | (t, Ty::RefMut(b)) => is_assignable(t, b),
        (Ty::Closure(p1, r1), Ty::Closure(p2, r2)) => {
            // 参数个数未知（空）或返回未知 → 视为通配
            if p1.is_empty() || p2.is_empty() || **r1 == Ty::Unknown || **r2 == Ty::Unknown {
                return true;
            }
            p1.len() == p2.len()
                && p1.iter().zip(p2).all(|(a, b)| is_assignable(a, b))
                && is_assignable(r1, r2)
        }
        // 泛型参数在单态化前对任意具体类型放行
        (Ty::Generic(_), _) | (_, Ty::Generic(_)) => true,
        _ => false,
    }
}

/// 数值提升：两数参与同一运算后的公共类型。
/// 只要一方是浮点就提升为浮点，否则保持整数；`Unknown` 让位于另一方。
pub fn numeric_join(a: &Ty, b: &Ty) -> Ty {
    if a == &Ty::Unknown {
        return b.clone();
    }
    if b == &Ty::Unknown {
        return a.clone();
    }
    if a.is_float() || b.is_float() {
        Ty::F64
    } else if a.is_int() && b.is_int() {
        a.clone()
    } else {
        a.clone()
    }
}

/// 二元运算符对应的重载方法名（运算符重载）。
pub fn op_method(op: BinOp) -> Option<&'static str> {
    Some(match op {
        BinOp::Add => "add",
        BinOp::Sub => "sub",
        BinOp::Mul => "mul",
        BinOp::Div => "div",
        BinOp::Rem => "rem",
        BinOp::Eq => "eq",
        BinOp::Ne => "ne",
        BinOp::Lt => "lt",
        BinOp::Le => "le",
        BinOp::Gt => "gt",
        BinOp::Ge => "ge",
        _ => return None,
    })
}

/// 一元运算符对应的重载方法名。
pub fn unary_op_method(op: UnOp) -> Option<&'static str> {
    match op {
        UnOp::Neg => Some("neg"),
        _ => None,
    }
}

// ============================================================
// 运算符类型规则
// ============================================================

/// 一元运算符的结果类型；不合法时返回 Err(原因)
pub fn unary_result(op: UnOp, t: &Ty) -> Result<Ty, String> {
    match op {
        UnOp::Neg => {
            if t.is_num() {
                Ok(t.clone())
            } else if *t == Ty::Unknown {
                Ok(Ty::I64)
            } else {
                Err(crate::te!("unary '-' requires a number, found {}", "一元 '-' 需要数值，实际是 {}", t))
            }
        }
        UnOp::Not => {
            if *t == Ty::Bool || *t == Ty::Unknown {
                Ok(Ty::Bool)
            } else {
                Err(crate::te!("'!' requires bool, found {}", "'!' 需要布尔，实际是 {}", t))
            }
        }
        UnOp::BitNot => {
            if t.is_int() || *t == Ty::Unknown {
                Ok(Ty::I64)
            } else {
                Err(crate::te!("'~' requires an integer, found {}", "'~' 需要整数，实际是 {}", t))
            }
        }
    }
}

/// 二元运算符的结果类型；不合法时返回 Err(原因)
pub fn binary_result(op: BinOp, a: &Ty, b: &Ty) -> Result<Ty, String> {
    if op.is_logic() {
        for t in [a, b] {
            if *t != Ty::Bool && *t != Ty::Unknown {
                return Err(crate::te!("logical operation requires bool, found {}", "逻辑运算需要布尔，实际是 {}", t));
            }
        }
        return Ok(Ty::Bool);
    }

    // 运算符重载：任一侧是结构体时先放行（返回 Unknown），由 mono 降级为方法调用
    if matches!(a, Ty::Struct(_)) || matches!(b, Ty::Struct(_)) {
        return Ok(Ty::Unknown);
    }

    // 字符串拼接：`+` 连接两个字符串（也允许 str + 非str？此处只做 str+str）
    if op == BinOp::Add && *a == Ty::Str && *b == Ty::Str {
        return Ok(Ty::Str);
    }

    if op.is_cmp() {
        // 字符串只支持 == / !=：放在前端拦下，两个后端才能给出同一条诊断
        if *a == Ty::Str && *b == Ty::Str {
            if !matches!(op, BinOp::Eq | BinOp::Ne) {
                return Err(format!(
                    "字符串只支持 == / != 比较，不支持 '{}'",
                    op.sym()
                ));
            }
            return Ok(Ty::Bool);
        }
        let ok = (a.is_num() && b.is_num())
            || (*a == Ty::Bool && *b == Ty::Bool)
            || *a == Ty::Unknown
            || *b == Ty::Unknown;
        if !ok {
            return Err(crate::te!("cannot compare {} with {}", "无法比较 {} 与 {}", a, b));
        }
        return Ok(Ty::Bool);
    }

    // 位运算：两个操作数都必须是整数（字符串/浮点/布尔都不允许）
    if op.is_bit() {
        for t in [a, b] {
            if !t.is_int() && *t != Ty::Unknown {
                return Err(crate::te!("bitwise operation requires an integer, found {}", "位运算需要整数，实际是 {}", t));
            }
        }
        return Ok(Ty::I64);
    }

    // 算术
    if !(a.is_num() || *a == Ty::Unknown) || !(b.is_num() || *b == Ty::Unknown) {
        return Err(crate::te!("arithmetic requires numbers, found {} and {}", "算术运算需要数值，实际是 {} 与 {}", a, b));
    }
    Ok(numeric_join(a, b))
}

/// 检查显式类型注解与实际类型是否相容，返回可读的错误
pub fn check_annotation(site: &str, annotated: &Ty, actual: &Ty) -> Result<(), String> {
    if is_assignable(annotated, actual) {
        Ok(())
    } else {
        {
            let (a_name, ac_name) = match (annotated, actual) {
                (crate::ast::Ty::Struct(a), crate::ast::Ty::Struct(b)) => (Some(a.clone()), Some(b.clone())),
                (crate::ast::Ty::Enum(a), crate::ast::Ty::Enum(b)) => (Some(a.clone()), Some(b.clone())),
                _ => (None, None),
            };
            let mut msg = crate::te!("{} declared as {}, but is actually {}", "{} 声明为 {}，但实际是 {}", site, annotated, actual);
            if let (Some(a), Some(b)) = (a_name, ac_name) {
                if crate::sema::closest_name_pub(&a, &[b.clone()][..]) {
                    let zh = crate::lang::is_zh();
                    msg = format!("{}{}", msg, if zh { format!("\x01是否想用 '{}'？", b) } else { format!("\x01did you mean '{}'?", b) });
                }
            }
            Err(msg)
        }
    }
}

// ============================================================
// 内置函数签名（sema 与两个后端共用的唯一判定）
// ============================================================

/// 内置函数是否可打印（数组需要遍历，void 无值）
pub fn is_printable(t: &Ty) -> bool {
    // 现在所有类型都可打印（容器/结构体/枚举/Option/Result 走运行时格式化）
    !matches!(t, Ty::Void)
}

/// 查询内置函数的返回类型。
///
/// 返回 `None` 表示不是内置函数（调用方去查用户函数表）；
/// 返回 `Some(Err(..))` 表示是内置函数但实参不合法，错误消息可直接上报。
///
/// 语义分析与两个后端都调用本函数，因此"哪个调用合法"只有一处定义。
pub fn builtin_ret(name: &str, args: &[Ty]) -> Option<Result<Ty, String>> {
    // 标准库函数不在此表（由 sema 经 gtlib_fn 处理），故对它们返回 None
    if gtlib_fn(name).is_some() {
        return None;
    }
    if !is_builtin_name(name) {
        return None;
    }
    Some(builtin_check(name, args))
}

fn builtin_check(name: &str, args: &[Ty]) -> Result<Ty, String> {
    let arity = |n: usize| -> Result<(), String> {
        if args.len() == n {
            Ok(())
        } else {
            Err(crate::te!("{}() expects {} argument(s), got {}", "{}() 需要 {} 个参数，实际给了 {}", name, n, args.len()))
        }
    };
    let one = || -> Result<&Ty, String> {
        arity(1)?;
        Ok(&args[0])
    };

    match name {
        // ---------- 输出 ----------
        "put" | "print" => {
            let a = one()?;
            if !is_printable(a) {
                return Err(crate::te!("{}() cannot output {}", "{}() 不能输出 {}", name, a));
            }
            Ok(Ty::Void)
        }

        // ---------- 输入 ----------
        "read_line" | "readline" | "input" => {
            if !args.is_empty() { return Err(crate::te!("{}() takes no arguments", "{}() 不接受参数", name)); }
            Ok(Ty::Str)
        }
        "read_int" | "readint" => {
            if !args.is_empty() { return Err(crate::te!("{}() takes no arguments", "{}() 不接受参数", name)); }
            Ok(Ty::I64)
        }

        // ---------- StringBuilder ----------
        "sb_new" => { if !args.is_empty() { return Err(crate::te!("sb_new() takes no arguments", "sb_new() 不接受参数")); } Ok(Ty::I64) }
        "sb_push" | "sb_push_str" | "sb_push_int" | "sb_push_f64" | "sb_push_bool" => { arity(2)?; Ok(Ty::Void) }
        "sb_finish" => { arity(1)?; Ok(Ty::Str) }

        // ---------- 长度 ----------
        "len" => match one()? {
            Ty::Str | Ty::Array(..) | Ty::List(..) | Ty::Set(..) | Ty::Map(..) | Ty::Unknown => {
                Ok(Ty::I64)
            }
            t @ Ty::I64 => Err(format!("{}{}", crate::te!("len() does not support {}", "len() 不支持 {}", t), if crate::lang::is_zh() { "\x01若它是容器/字符串，请为变量或形参显式标注类型（如 list[int] / str）" } else { "\x01if it is a container/string, annotate the variable or parameter type (e.g. list[int] / str)" })),
            other => Err(crate::te!("len() does not support {}", "len() 不支持 {}", other)),
        },

        // ---------- 类型转换 ----------
        // str(x)：任意标量 → 字符串
        "str" | "string" => match one()? {
            t if t.is_scalar() || *t == Ty::Unknown => Ok(Ty::Str),
            other => Err(crate::te!("str() cannot convert {}", "str() 不能转换 {}", other)),
        },
        // int(x)：字符串解析 / 浮点截断 / 布尔取 0|1
        "int" | "i64" => match one()? {
            t if t.is_scalar() || *t == Ty::Unknown => Ok(Ty::I64),
            other => Err(crate::te!("int() cannot convert {}", "int() 不能转换 {}", other)),
        },
        // f64(x)：整数提升 / 字符串解析
        "f64" | "float" => match one()? {
            t if t.is_scalar() || *t == Ty::Unknown => Ok(Ty::F64),
            other => Err(crate::te!("f64() cannot convert {}", "f64() 不能转换 {}", other)),
        },
        // bool(x)：非零/非空为真
        "bool" => match one()? {
            t if t.is_scalar() || *t == Ty::Unknown => Ok(Ty::Bool),
            other => Err(crate::te!("bool() cannot convert {}", "bool() 不能转换 {}", other)),
        },

        // ---------- 容器构造：list() / set() / map() ----------
        // 无参 → 元素类型 Unknown（随后由首次插入或注解确定）
        "list" | "List" => {
            if args.is_empty() {
                return Ok(Ty::List(Box::new(Ty::Unknown)));
            }
            arity(1)?;
            Ok(Ty::List(Box::new(args[0].clone())))
        }
        "set" | "Set" => {
            if args.is_empty() {
                return Ok(Ty::Set(Box::new(Ty::Unknown)));
            }
            arity(1)?;
            Ok(Ty::Set(Box::new(args[0].clone())))
        }
        "map" | "Map" | "dict" => {
            if args.is_empty() {
                return Ok(Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)));
            }
            arity(2)?;
            Ok(Ty::Map(Box::new(args[0].clone()), Box::new(args[1].clone())))
        }
        // chan() -> 通道句柄（I64）；chan_send(ch, v) -> void；chan_recv(ch) -> i64
        "chan" => {
            if !args.is_empty() { return Err("chan() takes no arguments".into()); }
            Ok(Ty::I64)
        }
        // fn_addr(f)：取顶层函数 f 的地址（裸机多任务入口用）
        "fn_addr" => {
            arity(1)?;
            Ok(Ty::I64)
        }
        "chan_send" => {
            arity(2)?;
            Ok(Ty::Void)
        }
        "chan_recv" => {
            arity(1)?;
            Ok(Ty::I64)
        }
        // sleep(ms) -> void
        "sleep" => {
            arity(1)?;
            Ok(Ty::Void)
        }
        // assert(cond) / assert(cond, msg) -> void
        "assert" => {
            if args.is_empty() || args.len() > 2 {
                return Err("assert() expects 1 or 2 arguments".into());
            }
            Ok(Ty::Void)
        }
        // range(a) / range(a, b) -> list[int]
        "range" => {
            if args.is_empty() || args.len() > 2 {
                return Err("range() expects 1 or 2 arguments".into());
            }
            Ok(Ty::List(Box::new(Ty::I64)))
        }

        // ---------- list 操作 ----------
        // push(lst, v) / append：原地追加，返回 void
        "push" | "append" => {
            arity(2)?;
            match &args[0] {
                Ty::List(elem) => {
                    // 元素类型已知（非 Unknown）时，第二参数须兼容
                    if !matches!(**elem, Ty::Unknown) {
                        let e = (**elem).clone();
                        let v = &args[1];
                        if !crate::sema::sema_const::compatible(&e, v) {
                            return Err(crate::te!(
                                "{}() element type mismatch: list of {}, got {}",
                                "{}() 元素类型不匹配：list 元素为 {}，实际为 {}", name, e, v
                            ));
                        }
                    }
                    Ok(Ty::Void)
                }
                Ty::Unknown => Ok(Ty::Void),
                other => Err(crate::te!("{}() expects a list, found {}", "{}() 需要 list，实际是 {}", name, other)),
            }
        }
        // pop(lst) → 元素类型（删末尾）
        "pop" => {
            arity(1)?;
            match &args[0] {
                Ty::List(e) => Ok((**e).clone()),
                Ty::Unknown => Ok(Ty::I64),
                other => Err(crate::te!("pop() expects a list, found {}", "pop() 需要 list，实际是 {}", other)),
            }
        }
        // at(lst, i) → 元素类型
        "at" => {
            arity(2)?;
            match &args[0] {
                Ty::List(e) => Ok((**e).clone()),
                Ty::Map(_, v) => Ok((**v).clone()),
                Ty::Unknown => Ok(Ty::I64),
                other => Err(crate::te!("at() does not support {}", "at() 不支持 {}", other)),
            }
        }
        // insert(map, k, v) / put 到集合
        "insert" => {
            if args.len() != 2 && args.len() != 3 {
                return Err(crate::te!("insert() expects 2 or 3 arguments, got {}", "insert() 需要 2 或 3 个参数，实际给了 {}", args.len()));
            }
            match &args[0] {
                Ty::Set(e) => {
                    if !matches!(**e, Ty::Unknown) && !crate::sema::sema_const::compatible(e, &args[1]) {
                        return Err(crate::te!("insert() element type mismatch: set of {}, got {}", "insert() 元素类型不匹配：set 元素为 {}，实际为 {}", e, args[1]));
                    }
                    Ok(Ty::Void)
                }
                Ty::Map(k, v) => {
                    if args.len() != 3 { return Err(crate::te!("insert() on a map needs (map, key, value)", "map 上的 insert() 需要 (map, key, value)")); }
                    if !matches!(**k, Ty::Unknown) && !crate::sema::sema_const::compatible(k, &args[1]) {
                        return Err(crate::te!("insert() key type mismatch: map key {}, got {}", "insert() 键类型不匹配：map 键为 {}，实际为 {}", k, args[1]));
                    }
                    if !matches!(**v, Ty::Unknown) && !crate::sema::sema_const::compatible(v, &args[2]) {
                        return Err(crate::te!("insert() value type mismatch: map value {}, got {}", "insert() 值类型不匹配：map 值为 {}，实际为 {}", v, args[2]));
                    }
                    Ok(Ty::Void)
                }
                Ty::Unknown => Ok(Ty::Void),
                other => Err(crate::te!("insert() expects a map/set, found {}", "insert() 需要 map/set，实际是 {}", other)),
            }
        }
        // has(container, key) → bool
        "has" | "contains" => {
            arity(2)?;
            match &args[0] {
                Ty::Map(..) | Ty::Set(..) | Ty::List(..) | Ty::Unknown => Ok(Ty::Bool),
                other => Err(crate::te!("{}() does not support {}", "{}() 不支持 {}", name, other)),
            }
        }
        // keys(map) / values(map) → 元素是 list（引用语义）
        "keys" => {
            arity(1)?;
            match &args[0] {
                Ty::Map(k, _) => Ok(Ty::List(k.clone())),
                Ty::Unknown => Ok(Ty::List(Box::new(Ty::Unknown))),
                other => Err(crate::te!("keys() expects a map, found {}", "keys() 需要 map，实际是 {}", other)),
            }
        }
        "values" => {
            arity(1)?;
            match &args[0] {
                Ty::Map(_, v) => Ok(Ty::List(v.clone())),
                Ty::Unknown => Ok(Ty::List(Box::new(Ty::Unknown))),
                other => Err(crate::te!("values() expects a map, found {}", "values() 需要 map，实际是 {}", other)),
            }
        }
        // remove(container, key) → void
        "remove" => {
            arity(2)?;
            match &args[0] {
                Ty::Map(..) | Ty::Set(..) | Ty::List(..) | Ty::Unknown => Ok(Ty::Void),
                other => Err(crate::te!("remove() does not support {}", "remove() 不支持 {}", other)),
            }
        }

        // ---------- 数值内置 ----------
        // abs / min / max / sum：保持数值类型
        "abs" => {
            arity(1)?;
            match &args[0] {
                t if t.is_num() || *t == Ty::Unknown => Ok(args[0].clone()),
                other => Err(crate::te!("abs() requires a number, found {}", "abs() 需要数值，实际是 {}", other)),
            }
        }
        "min" | "max" => {
            if args.is_empty() {
                return Err(crate::te!("{}() requires at least 1 argument", "{}() 至少需要 1 个参数", name));
            }
            Ok(numeric_join(&args[0], args.get(1).unwrap_or(&args[0])))
        }
        "sum" => {
            arity(1)?;
            match &args[0] {
                Ty::List(e) => Ok((**e).clone()),
                Ty::Unknown => Ok(Ty::I64),
                other => Err(crate::te!("sum() does not support {}", "sum() 不支持 {}", other)),
            }
        }

        // ---------- 裸内存 ----------
        // mem_alloc(n) → 指针（i64）
        "sb_new" => { arity(0)?; Ok(Ty::I64) }
        "sb_push" | "sb_push_str" | "sb_push_int" | "sb_push_i64" | "sb_push_f64" | "sb_push_bool" => { arity(2)?; Ok(Ty::Void) }
        "str_builder" => { arity(0)?; Ok(Ty::I64) }
        "sb_append" => { arity(2)?; Ok(Ty::Void) }
        "sb_append_int" => { arity(2)?; Ok(Ty::Void) }
        "sb_push_char" => { arity(2)?; Ok(Ty::Void) }
        "sb_pop" => { arity(1)?; Ok(Ty::Void) }
        "sb_finish" => { arity(1)?; Ok(Ty::Str) }
        "mem_alloc" => {
            arity(1)?;
            Ok(Ty::I64)
        }
        "mem_free" => {
            arity(1)?;
            Ok(Ty::Void)
        }
        // mem_store_i64(p, off, v) / mem_store_u8
        "mem_store_i64" | "mem_store_u8" | "mem_set" => {
            arity(3)?;
            Ok(Ty::Void)
        }
        // mem_load_i64(p, off) / mem_load_u8
        "mem_load_i64" | "mem_load_u8" => {
            arity(2)?;
            Ok(Ty::I64)
        }
        // mem_copy(dst, src, n)
        "mem_copy" => {
            arity(3)?;
            Ok(Ty::Void)
        }

        // ---------- 字符串内置 ----------
        // substr(s, start, len) → str
        "substr" => {
            arity(3)?;
            Ok(Ty::Str)
        }
        // split(s, sep) → list[str]
        "split" => {
            arity(2)?;
            Ok(Ty::List(Box::new(Ty::Str)))
        }
        // join(list, sep) → str
        "join" => {
            arity(2)?;
            Ok(Ty::Str)
        }
        // find(s, sub) → i64（下标，找不到 -1）
        "find" => {
            arity(2)?;
            Ok(Ty::I64)
        }
        // upper / lower / trim → str
        "upper" | "lower" | "trim" => {
            arity(1)?;
            Ok(Ty::Str)
        }
        // str_repeat(s, n) → str
        "repeat" => {
            arity(2)?;
            Ok(Ty::Str)
        }
        // replace(s, from, to) → str
        "replace" => {
            arity(3)?;
            Ok(Ty::Str)
        }
        // pad_left(s, width[, fill]) / pad_right → str（宽度不足时补字符）
        "pad_left" | "pad_right" | "lpad" | "rpad" => {
            if args.len() < 2 || args.len() > 3 {
                return Err("pad_left/pad_right expects 2..3 arguments".into());
            }
            Ok(Ty::Str)
        }
        // fmt_int(x, width) → str（右对齐，空格填充）
        "fmt_int" => {
            arity(2)?;
            Ok(Ty::Str)
        }

        other => Err(crate::te!("internal error: unknown builtin '{}'", "内部错误：未知内置函数 '{}'", other)),
    }
}




#[path = "type_gtlib.rs"]
mod type_gtlib;
pub use type_gtlib::*;
// ============================================================
// 单元测试：类型规则必须稳定，两个后端共用
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_widens_to_float_but_not_back() {
        assert!(is_assignable(&Ty::F64, &Ty::I64));
        assert!(!is_assignable(&Ty::I64, &Ty::F64));
    }

    #[test]
    fn unknown_is_permissive() {
        assert!(is_assignable(&Ty::Unknown, &Ty::Str));
        assert!(is_assignable(&Ty::Str, &Ty::Unknown));
    }

    #[test]
    fn arithmetic_promotes_to_float() {
        assert_eq!(binary_result(BinOp::Add, &Ty::I64, &Ty::F64).unwrap(), Ty::F64);
        assert_eq!(binary_result(BinOp::Add, &Ty::I64, &Ty::I64).unwrap(), Ty::I64);
    }

    #[test]
    fn arithmetic_rejects_str() {
        assert!(binary_result(BinOp::Add, &Ty::I64, &Ty::Str).is_err());
    }

    #[test]
    fn comparison_returns_bool() {
        assert_eq!(binary_result(BinOp::Lt, &Ty::I64, &Ty::F64).unwrap(), Ty::Bool);
        assert_eq!(binary_result(BinOp::Eq, &Ty::Str, &Ty::Str).unwrap(), Ty::Bool);
    }

    #[test]
    fn array_assignability_checks_shape() {
        let a = Ty::Array(Box::new(Ty::I64), 3);
        let b = Ty::Array(Box::new(Ty::I64), 3);
        let c = Ty::Array(Box::new(Ty::I64), 4);
        assert!(is_assignable(&a, &b));
        assert!(!is_assignable(&a, &c));
    }

    #[test]
    fn not_requires_bool() {
        assert!(unary_result(UnOp::Not, &Ty::I64).is_err());
        assert_eq!(unary_result(UnOp::Not, &Ty::Bool).unwrap(), Ty::Bool);
    }

    #[test]
    fn builtin_conversions_typecheck() {
        assert_eq!(builtin_ret("str", &[Ty::I64]).unwrap().unwrap(), Ty::Str);
        assert_eq!(builtin_ret("int", &[Ty::Str]).unwrap().unwrap(), Ty::I64);
        assert_eq!(builtin_ret("f64", &[Ty::I64]).unwrap().unwrap(), Ty::F64);
        assert_eq!(builtin_ret("bool", &[Ty::F64]).unwrap().unwrap(), Ty::Bool);
        // 数组不可转换；但可打印（完整 put 支持容器）
        let arr = Ty::Array(Box::new(Ty::I64), 2);
        assert!(builtin_ret("int", &[arr.clone()]).unwrap().is_err());
        assert!(builtin_ret("put", &[arr]).unwrap().is_ok());
        // 用户函数名不在内置表里
        assert!(builtin_ret("my_fn", &[Ty::I64]).is_none());
    }

    #[test]
    fn builtin_arity_is_checked() {
        assert!(builtin_ret("len", &[]).unwrap().is_err());
        assert!(builtin_ret("len", &[Ty::Str, Ty::Str]).unwrap().is_err());
        assert_eq!(builtin_ret("len", &[Ty::Str]).unwrap().unwrap(), Ty::I64);
    }

    #[test]
    fn len_accepts_all_containers_and_unknown() {
        // i64 参数：给出"请标注容器类型"的友好错误（而非放行）
        assert!(builtin_ret("len", &[Ty::I64]).unwrap().is_err());
        assert_eq!(builtin_ret("len", &[Ty::Unknown]).unwrap().unwrap(), Ty::I64);
        assert_eq!(builtin_ret("len", &[Ty::List(Box::new(Ty::I64))]).unwrap().unwrap(), Ty::I64);
        assert_eq!(builtin_ret("len", &[Ty::Map(Box::new(Ty::Str), Box::new(Ty::I64))]).unwrap().unwrap(), Ty::I64);
        assert_eq!(builtin_ret("len", &[Ty::Array(Box::new(Ty::I64), 3)]).unwrap().unwrap(), Ty::I64);
        assert!(builtin_ret("len", &[Ty::Bool]).unwrap().is_err());
        // 非内建名不返回类型
        assert!(builtin_ret("不存在的内建", &[]).is_none());
    }

    #[test]
    fn str_converts_scalars_only() {
        assert_eq!(builtin_ret("str", &[Ty::I64]).unwrap().unwrap(), Ty::Str);
        assert_eq!(builtin_ret("str", &[Ty::F64]).unwrap().unwrap(), Ty::Str);
        assert_eq!(builtin_ret("str", &[Ty::Bool]).unwrap().unwrap(), Ty::Str);
        assert_eq!(builtin_ret("str", &[Ty::Unknown]).unwrap().unwrap(), Ty::Str);
        assert!(builtin_ret("str", &[Ty::List(Box::new(Ty::I64))]).unwrap().is_err());
    }

    #[test]
    fn int_and_float_conversions() {
        assert_eq!(builtin_ret("int", &[Ty::F64]).unwrap().unwrap(), Ty::I64);
        assert_eq!(builtin_ret("int", &[Ty::Str]).unwrap().unwrap(), Ty::I64);
        assert_eq!(builtin_ret("f64", &[Ty::I64]).unwrap().unwrap(), Ty::F64);
        assert_eq!(builtin_ret("bool", &[Ty::I64]).unwrap().unwrap(), Ty::Bool);
    }

    #[test]
    fn numeric_join_rules() {
        assert_eq!(numeric_join(&Ty::I64, &Ty::I64), Ty::I64);
        assert_eq!(numeric_join(&Ty::I64, &Ty::F64), Ty::F64);
        assert_eq!(numeric_join(&Ty::F64, &Ty::I64), Ty::F64);
        assert_eq!(numeric_join(&Ty::F64, &Ty::F64), Ty::F64);
    }

    #[test]
    fn op_method_names() {
        assert_eq!(op_method(BinOp::Add), Some("add"));
        assert_eq!(op_method(BinOp::Lt), Some("lt"));
        assert_eq!(op_method(BinOp::And), None); // 逻辑运算无重载方法
    }

    #[test]
    fn unary_method_names() {
        assert_eq!(unary_op_method(UnOp::Neg), Some("neg"));
        assert_eq!(unary_op_method(UnOp::Not), None);
        assert_eq!(unary_op_method(UnOp::BitNot), None);
    }

    #[test]
    fn assignability_containers() {
        let li = Ty::List(Box::new(Ty::I64));
        let lf = Ty::List(Box::new(Ty::F64));
        // 容器：元素类型可赋值（协变宽松）
        assert!(is_assignable(&lf, &li));
        // 结构体名字不同 → 不可赋值
        assert!(!is_assignable(&Ty::Struct("A".into()), &Ty::Struct("B".into())));
        assert!(is_assignable(&Ty::Struct("A".into()), &Ty::Struct("A".into())));
        // Unknown 元素宽松
        assert!(is_assignable(&Ty::List(Box::new(Ty::Unknown)), &li));
    }

    #[test]
    fn unknown_annotation_accepts_anything() {
        assert!(check_annotation("x", &Ty::Unknown, &Ty::I64).is_ok());
        assert!(check_annotation("x", &Ty::Str, &Ty::Unknown).is_ok());
        assert!(check_annotation("x", &Ty::Str, &Ty::I64).is_err());
        assert!(check_annotation("x", &Ty::Str, &Ty::Str).is_ok());
    }

    #[test]
    fn result_and_option_assignability() {
        let r_i = Ty::Result(Box::new(Ty::I64), Box::new(Ty::Str));
        let r_f = Ty::Result(Box::new(Ty::F64), Box::new(Ty::Str));
        assert!(is_assignable(&r_f, &r_i)); // I64 → F64 载荷可放宽
        let o_i = Ty::Option(Box::new(Ty::I64));
        let o_f = Ty::Option(Box::new(Ty::F64));
        assert!(is_assignable(&o_f, &o_i));
    }

    #[test]
    fn printable_scalars() {
        assert!(is_printable(&Ty::I64));
        assert!(is_printable(&Ty::F64));
        assert!(is_printable(&Ty::Bool));
        assert!(is_printable(&Ty::Str));
        assert!(is_printable(&Ty::Unknown));
        // 完整 put：容器/结构体/枚举也可打印
        assert!(is_printable(&Ty::Array(Box::new(Ty::I64), 2)));
        assert!(is_printable(&Ty::List(Box::new(Ty::I64))));
        assert!(is_printable(&Ty::Map(Box::new(Ty::I64), Box::new(Ty::I64))));
        assert!(!is_printable(&Ty::Void));
    }

    #[test]
    fn llvm_repr_of_core_types() {
        assert_eq!(Ty::I64.llvm(), "i64");
        assert_eq!(Ty::Unknown.llvm(), "i64");
        assert_eq!(Ty::F64.llvm(), "double");
        assert_eq!(Ty::Str.llvm(), "ptr");
        assert_eq!(Ty::Result(Box::new(Ty::I64), Box::new(Ty::Str)).llvm(), "ptr");
        assert_eq!(Ty::Option(Box::new(Ty::I64)).llvm(), "ptr");
        assert_eq!(Ty::Void.llvm(), "void");
        assert_eq!(Ty::Bool.llvm(), "i1");
    }
}

