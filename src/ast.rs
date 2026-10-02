//! GTLang 的 AST（统一中间表示）与诊断类型。
//!
//! 类型系统见 `type.rs`；本模块只描述语法结构。

// AST 节点统一携带源码位置与标记位（mutable / line 等），
// 部分字段当前仅用于诊断与后续阶段。
#![allow(dead_code)]

use std::fmt;

/// 类型系统集中在 `type.rs`（单一事实来源），这里重导出以便各模块沿用 `ast::Ty`。
pub use crate::types::Ty;

// ============================================================
// 源码位置与诊断
// ============================================================

/// 源码字节区间（用于诊断高亮）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        Span { start, end }
    }
    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

/// 词法/语法阶段错误：带精确字节区间
#[derive(Debug, Clone)]
pub struct ParseError {
    pub msg: String,
    pub span: Span,
}

impl ParseError {
    pub fn new(msg: impl Into<String>, span: Span) -> ParseError {
        ParseError { msg: msg.into(), span }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

// ============================================================
// 表达式
// ============================================================

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    /// sema 阶段回填的静态类型
    pub ty: Ty,
    pub line: usize,
}

impl Expr {
    pub fn new(kind: ExprKind, line: usize) -> Self {
        Expr { kind, ty: Ty::Unknown, line }
    }
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    /// 带 `{expr}` 插值的字符串
    Interp(Vec<StrPart>),
    Ident(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    /// 命名参数调用 `f(a: 1, b: 2)`：opt 阶段按形参名重排为普通 Call
    CallNamed(String, Vec<(String, Expr)>),
    Index(Box<Expr>, Box<Expr>),
    /// 切片 `s[lo..hi]`（字符串/数组）
    Slice(Box<Expr>, Box<Expr>, Box<Expr>),
    ArrayLit(Vec<Expr>),
    /// 元组字面量 `(a, b, c)`（值语义：堆块）
    TupleLit(Vec<Expr>),
    /// 列表推导 `[expr for var in iter if cond]`
    ListComp { expr: Box<Expr>, var: String, iter: Box<Expr>, cond: Option<Box<Expr>> },
    /// 字段访问 `obj.field`
    Field(Box<Expr>, String),
    /// 结构体字面量 `Point { x: 1, y: 2 }`
    StructLit(String, Vec<(String, Expr)>),
    /// 枚举构造 Name::Variant(args)
    EnumLit(String, String, Vec<Expr>),
    /// `dyn Trait(v)`：把 v 装箱为 trait 对象
    DynBox { trait_name: String, value: Box<Expr> },

    /// if 作为表达式：`x := if c { 1 } else { 2 }`
    If { cond: Box<Expr>, then: Block, els: Option<Block> },
    /// match 作为表达式：`match v { pat => val, ... }`
    Match { subject: Box<Expr>, arms: Vec<MatchArm> },
    /// 闭包字面量 `|x, y| body`
    Closure { params: Vec<String>, param_tys: Vec<Option<Ty>>, ret_ty: Option<Ty>, body: Box<Expr>, line: usize },
    /// 间接调用 `f(args)`，其中 f 是闭包值（而非函数名）
    CallValue { callee: Box<Expr>, args: Vec<Expr> },
    /// 方法链 `recv.方法(args)`：recv 是任意表达式（如另一个调用结果）。
    /// 由 mono 降级为 `类型__方法(recv, ...args)`。
    MethodOn { recv: Box<Expr>, method: String, args: Vec<Expr> },
    /// 闭包构造（由 lifting 从 Closure 降级而来）：
    /// 分配 `[fn_ptr, env_ptr]` 块，env 里按顺序存 captures。
    ClosureNew { fn_name: String, captures: Vec<Expr> },
    /// 借用表达式 `&x`（mutable=false）/ `&mut x`（mutable=true）。
    /// 编译期检查借用冲突；代码生成阶段按值透传。
    Borrow { mutable: bool, inner: Box<Expr> },
    /// `Ok(v)`：构造成功的 Result
    Ok(Box<Expr>),
    /// `Err(e)`：构造失败的 Result
    Err(Box<Expr>),
    /// `expr?`：Result 传播——Err 时从当前函数提前返回 Err，Ok 时解包其值
    Try(Box<Expr>),
    /// `Some(v)`：构造 Option
    Some(Box<Expr>),
    /// `None`：空 Option
    None,
    /// `try { ... } expt ... fily ...` 作为**表达式**（块值）
    TryBlock { body: Block, catches: Vec<CatchArm>, fin: Option<Block> },
}

/// match 的一条分支
#[derive(Debug, Clone)]
pub struct MatchArm {
    /// 模式：None 表示通配 `_`
    pub pat: Option<Expr>,
    /// 范围模式 `lo..hi`（含 lo，不含 hi）；与 pat 二选一
    pub range: Option<(Expr, Expr)>,
    /// 守卫条件 `if cond`
    pub guard: Option<Expr>,
    /// 分支体
    pub body: Block,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum StrPart {
    Lit(String),
    Expr(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    /// 按位取反 `~x`
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    /// 按位与 `&`
    BitAnd,
    /// 按位或 `|`
    BitOr,
    /// 按位异或 `^`
    BitXor,
    /// 左移 `<<`
    Shl,
    /// 算术右移 `>>`
    Shr,
}

impl BinOp {
    /// 源码运算符 → BinOp（非运算符返回 None）
    pub fn from_str(s: &str) -> Option<BinOp> {
        Some(match s {
            "+" => BinOp::Add,
            "-" => BinOp::Sub,
            "*" => BinOp::Mul,
            "/" => BinOp::Div,
            "//" => BinOp::FloorDiv,
            "%" => BinOp::Rem,
            "==" => BinOp::Eq,
            "!=" => BinOp::Ne,
            "<" => BinOp::Lt,
            "<=" => BinOp::Le,
            ">" => BinOp::Gt,
            ">=" => BinOp::Ge,
            "&&" => BinOp::And,
            "||" => BinOp::Or,
            "&" => BinOp::BitAnd,
            "|" => BinOp::BitOr,
            "^" => BinOp::BitXor,
            "<<" => BinOp::Shl,
            ">>" => BinOp::Shr,
            _ => return None,
        })
    }

    pub fn is_cmp(self) -> bool {
        matches!(self, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
    }

    pub fn is_logic(self) -> bool {
        matches!(self, BinOp::And | BinOp::Or)
    }

    /// 是否位运算（要求整数操作数，结果为整数）
    pub fn is_bit(self) -> bool {
        matches!(
            self,
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr
        )
    }

    /// 是否移位（右操作数是移位数，不参与类型提升）
    pub fn is_shift(self) -> bool {
        matches!(self, BinOp::Shl | BinOp::Shr)
    }

    /// 源码写法（用于诊断消息）
    pub fn sym(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::FloorDiv => "//",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
        }
    }
}

// ============================================================
// 语句
// ============================================================

pub type Block = Vec<Stmt>;

#[derive(Debug, Clone)]
pub enum Stmt {
    Let { name: String, ty: Option<Ty>, value: Expr, mutable: bool, line: usize },
    /// 赋值。`index` 为 `None` 表示整体赋值（`x = v`），
    /// 为 `Some(i)` 表示数组元素赋值（`a[i] = v`）。
    Assign { name: String, index: Option<Expr>, op: Option<BinOp>, value: Expr, line: usize },
    /// 字段赋值 `obj.field = v` / `obj.field += v`
    FieldAssign { obj: String, field: String, op: Option<BinOp>, value: Expr, line: usize },
    /// 裸表达式语句（位于块尾时即为该块的值）
    Expr(Expr),
    If { cond: Expr, then: Block, els: Option<Block>, line: usize },
    While { cond: Expr, body: Block, line: usize },
    /// `do { body } while cond`
    DoWhile { body: Block, cond: Expr, line: usize },
    ForRange { var: String, from: Expr, to: Expr, body: Block, els: Option<Block>, line: usize },
    ForEach { var: String, iter: Expr, body: Block, els: Option<Block>, line: usize },
    Return(Option<Expr>, usize),
    /// `label: loop`（标签循环，供 break/continue 按名跳出多层）
    Labeled { label: String, inner: Box<Stmt>, line: usize },
    /// `break` / `break label`
    Break(Option<String>, usize),
    /// `continue` / `continue label`
    Continue(Option<String>, usize),
    Block(Block),
    /// 函数内声明的嵌套函数（会被"提升"到顶层，加唯一前缀）
    LocalFn(FnDef),
    /// 函数内 `const` 声明
    Const { name: String, ty: Option<Ty>, value: Expr, line: usize },
    /// `go f(args)`：在新线程中调用函数（fire-and-forget）
    Go { func: String, args: Vec<Expr>, line: usize },
    /// `try { ... } expt ... fily { ... }` 异常/错误处理。
    /// - `body`：受保护块
    /// - `catches`：`expt` 分支（可多个，按顺序匹配）
    /// - `fin`：`fily` 分支（无论如何都执行）
    Try { body: Block, catches: Vec<CatchArm>, fin: Option<Block>, line: usize },
    /// `throw e` / `raise e`：抛出错误/异常
    Throw(Expr, usize),
    /// 内联汇编（已废弃，parser 不再产出）
    Asm { lines: Vec<String>, line: usize },
}

impl Stmt {
    /// 遍历本语句**直接**包含的所有子块（不递归）。
    /// 用于"需要看全部语句"的分析（如 scan_mutation），
    /// 新增含块的 Stmt 变体时只需在此补一处，避免各处 match 漏分支。
    pub fn each_block<'a>(&'a self, f: &mut impl FnMut(&'a Block)) {
        match self {
            Stmt::If { then, els, .. } => { f(then); if let Some(e) = els { f(e); } }
            Stmt::While { body, .. } => f(body),
            Stmt::DoWhile { body, .. } => f(body),
            Stmt::ForRange { body, els, .. } => { f(body); if let Some(e) = els { f(e); } }
            Stmt::ForEach { body, els, .. } => { f(body); if let Some(e) = els { f(e); } }
            Stmt::Block(inner) => f(inner),
            Stmt::Labeled { inner, .. } => inner.each_block(f),
            Stmt::Try { body, catches, fin, .. } => {
                f(body);
                for ca in catches { f(&ca.body); }
                if let Some(fin) = fin { f(fin); }
            }
            Stmt::LocalFn(fd) => f(&fd.body),
            Stmt::Let { .. } | Stmt::Assign { .. } | Stmt::FieldAssign { .. } | Stmt::Expr(_)
            | Stmt::Return(..) | Stmt::Break(..) | Stmt::Continue(..) | Stmt::Const { .. }
            | Stmt::Go { .. } | Stmt::Throw(..) | Stmt::Asm { .. } => {}
        }
    }
}

/// `expt` 的一个捕获分支。
#[derive(Debug, Clone)]
pub struct CatchArm {
    /// 值绑定：`expt e { ... }` 中的 `e`（None = 通配，不绑定）
    pub binding: Option<String>,
    /// 标签匹配：`expt 除零 { ... }`（None = 匹配任意）
    pub label: Option<String>,
    /// 守卫：`expt e if e > 0 { ... }`
    pub guard: Option<Expr>,
    pub body: Block,
    pub line: usize,
}

// ============================================================
// 顶层项
// ============================================================

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<Ty>,
    /// 默认值 `fn f(a: int = 5)`
    pub default: Option<Expr>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    /// 泛型类型参数列表 `fn f[T, U](...)`；空表示非泛型
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    pub ret: Option<Ty>,
    /// sema 回填的返回类型
    pub ret_ty: Ty,
    pub body: Block,
    pub line: usize,
    /// 是否 `pub`（可被其它模块导入）
    pub is_pub: bool,
    /// where 泛型约束：(类型参数名, trait 名)
    pub bounds: Vec<(String, String)>,
}

/// `enum 名 { Variant(payload...) ... }`
#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub type_params: Vec<String>,
    /// 变体：(名字, 载荷类型列表)
    pub variants: Vec<(String, Vec<Ty>)>,
    pub derives: Vec<String>,
    pub line: usize,
    pub is_pub: bool,
}

impl EnumDef {
    pub fn variant_index(&self, name: &str) -> Option<usize> {
        self.variants.iter().position(|(n, _)| n == name)
    }
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    /// 泛型类型参数 `struct 盒[T]`；空 = 非泛型
    pub type_params: Vec<String>,
    /// 字段：(名字, 可选类型, 行号)
    pub fields: Vec<(String, Option<Ty>, usize)>,
    /// `@derive(...)` 派生的 trait 名（如 Eq / Debug）
    pub derives: Vec<String>,
    pub line: usize,
    pub is_pub: bool,
}

impl StructDef {
    /// 字段按声明顺序的字节偏移（每字段 8 字节槽）
    pub fn field_offset(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|(n, _, _)| n == name).map(|i| i * 8)
    }
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|(n, _, _)| n == name)
    }
}

#[derive(Debug, Clone)]
pub enum Item {
    Const { name: String, ty: Option<Ty>, value: Expr, line: usize, is_pub: bool },
    Fn(FnDef),
    Struct(StructDef),
    /// `macro 名(参数) { 模板表达式 }`：声明式宏（AST 级展开）
    Macro { name: String, params: Vec<String>, body: Expr, line: usize },
    /// `enum 名 { V1(T1, ...) V2 ... }`：变体带可选载荷
    Enum(EnumDef),
    /// `impl 类型 { ... }` / `impl[T] 类型[T] { ... }` 方法块
    Impl { type_params: Vec<String>, ty: String, methods: Vec<FnDef>, line: usize },
    /// `extern "C" { fn ... }` 外部 C 函数声明
    ExternC(Vec<ExternFn>),
    /// `trait 名字 { fn ... }` trait 声明（方法签名）
    Trait(TraitDef),
    /// `impl Trait for 类型 { ... }` / `impl[T] Trait for T { ... }` trait 实现
    /// （方法体展开为 `类型__方法`；带 `type_params` 时为 blanket / 泛型实现）
    TraitImpl { type_params: Vec<String>, trait_name: String, ty: String, methods: Vec<FnDef>, line: usize },
}

/// trait 声明：名字 + 方法签名列表
#[derive(Debug, Clone)]
pub struct TraitDef {
    pub name: String,
    /// 方法签名：(方法名, 参数类型列表, 返回类型)
    pub methods: Vec<(String, Vec<Ty>, Ty)>,
    /// 默认方法（带 body）：(方法名, 参数名+类型列表, 返回类型, body)
    pub defaults: Vec<(String, Vec<(String, Ty)>, Ty, Block)>,
    pub line: usize,
    pub is_pub: bool,
}

/// 一条 `extern "C"` 函数声明
#[derive(Debug, Clone)]
pub struct ExternFn {
    pub name: String,
    pub params: Vec<Ty>,
    pub ret: Ty,
}

/// 一条 `import` 声明（方案 B：真模块）。
#[derive(Debug, Clone)]
pub struct Import {
    /// `import c "foo.h"` 的 C 头标记（true 表示导入 C 头）
    pub is_c_header: bool,
    /// 模块路径段。`import a.b.c` → `["a", "b", "c"]`；
    /// `import "x.gt"` → `["x.gt"]` 且 `is_file = true`。
    pub path: Vec<String>,
    /// `as` 别名（缺省用最后一段）
    pub alias: Option<String>,
    /// 是否为字符串路径导入（`import "…"`）
    pub is_file: bool,
    pub line: usize,
}

impl Import {
    /// 绑定到当前作用域的本地名字
    pub fn local_name(&self) -> String {
        self.alias
            .clone()
            .unwrap_or_else(|| self.path.last().cloned().unwrap_or_default())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub items: Vec<Item>,
    /// 顶层 `import` 声明（方案 B：真模块，由加载器解析）。
    pub imports: Vec<Import>,
    /// 已导入的内置标准库模块名（math/string/json/...）；调用标准库函数前须在此列。
    pub imported_gtlib: Vec<String>,
    /// 内联 C 块（`C { ... }`）的源码，多个块按出现顺序拼接。
    /// 编译后端把它交给 clang，解释器后端交给 tcc。
    pub cblock: String,
    /// 从内联 C 块自动解析出的函数签名，GTLang 可直接调用（无需手写声明）。
    pub cfuncs: Vec<crate::cblock::CFn>,
}
