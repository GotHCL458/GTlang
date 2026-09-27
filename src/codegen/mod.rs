//! LLVM IR 文本后端。
//!
//! 直接产出 `.ll` 文本：不依赖 llvm-sys / inkwell，因此与 LLVM 版本、构建环境解耦，
//! 由 driver 调用 clang 把 `.ll` 编译链接成可执行文件。
//!
//! 约定：
//! - 所有整型在 IR 层统一为 i64，输出时用 `%lld`
//! - 所有 alloca 提升到 entry 块，避免循环体内栈增长
//! - 字符串以全局常量 `@.strN`（ptr，NUL 结尾）表示

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// 是否启用整数溢出检查（默认开）。`--no-overflow-check` 关闭后加减乘用回绕指令。
static OVERFLOW_CHECK: AtomicBool = AtomicBool::new(true);

pub fn set_overflow_check(on: bool) {
    OVERFLOW_CHECK.store(on, Ordering::Relaxed);
}
fn overflow_check_enabled() -> bool {
    OVERFLOW_CHECK.load(Ordering::Relaxed)
}
use crate::sema::{Analysis, ConstVal, Value as CVal};
use crate::types::builtin_ret;

/// 插值字符串作为值使用时的栈缓冲上限
const INTERP_BUF: usize = 4096;

pub fn generate(prog: &Program, an: &Analysis, file: &str) -> Result<String, String> {
    let mut cg = Codegen {
        module: String::new(),
        globals: String::new(),
        string_cache: HashMap::new(),
        declares: HashSet::new(),
        err_stack: Vec::new(),
        entry_allocas: Vec::new(),
        body: String::new(),
        reg: 0,
        label: 0,
        slot: 0,
        scopes: vec![HashMap::new()],
        fns: HashMap::new(),
        consts: &an.consts,
        cur_ret: Ty::Void,
        terminated: false,
        break_label: None,
        continue_label: None,
        file: file.to_string(),
        var_cache: HashMap::new(),
        perm_ptrs: HashSet::new(),
        perm_cache: HashMap::new(),
        immutable_lets: HashSet::new(),
        structs: HashMap::new(),
        enum_variants: HashMap::new(),
        traits: an.traits.clone(),
        trait_impls: an.trait_impls.clone(),
        bounded: HashMap::new(),
        range_analysis: None,
        labeled: Vec::new(),
        label_targets: HashMap::new(),
    };
    // 先做范围分析（用 &Program，与后续遍历同一 AST，地址一致）
    cg.range_analysis = Some(crate::range::analyze(prog));
    cg.run(prog)
}

#[derive(Clone)]
struct Local {
    ptr: String,
    ty: Ty,
}

#[derive(Clone)]
struct FnInfo {
    cname: String,
    params: Vec<Ty>,
    ret: Ty,
}

#[derive(Clone)]
struct Val {
    ty: Ty,
    /// 可直接作为 LLVM 操作数使用的字符串（字面量 / 全局标签 / `%tN`）
    s: String,
}

impl Val {
    pub(crate) fn new(ty: &Ty, s: impl Into<String>) -> Val {
        Val { ty: ty.clone(), s: s.into() }
    }
}

struct Codegen<'a> {
    pub(crate) module: String,
    pub(crate) globals: String,
    pub(crate) string_cache: HashMap<Vec<u8>, String>,
    pub(crate) declares: HashSet<String>,
    pub(crate) entry_allocas: Vec<String>,
    pub(crate) body: String,
    pub(crate) reg: usize,
    pub(crate) label: usize,
    pub(crate) slot: usize,
    pub(crate) scopes: Vec<HashMap<String, Local>>,
    pub(crate) fns: HashMap<String, FnInfo>,
    pub(crate) consts: &'a HashMap<String, ConstVal>,
    pub(crate) cur_ret: Ty,
    pub(crate) terminated: bool,
    pub(crate) break_label: Option<String>,
    pub(crate) continue_label: Option<String>,
    pub(crate) file: String,
    /// 当前基本块内已知的变量值（alloca 指针 → SSA 值）。
    /// 用于消除冗余 load/store，见 `load` / `new_label`。
    pub(crate) var_cache: HashMap<String, Val>,
    /// 可以**跨基本块**保持值的变量（alloca 指针）。
    ///
    /// 只包含"声明后从不被重新赋值、且不在循环体内声明"的变量：
    /// 它们的 SSA 值在整个函数里唯一，分支/汇合都不会改变，
    /// 因此不必在每个新块重新 load。
    pub(crate) perm_ptrs: HashSet<String>,
    /// 上述变量的值
    pub(crate) perm_cache: HashMap<String, Val>,
    /// 当前函数里"从不被重新赋值、且不在循环体内声明"的 `let` 变量名
    pub(crate) immutable_lets: HashSet<String>,
    /// 结构体布局：名字 → [(字段名, 类型)]
    pub(crate) structs: HashMap<String, Vec<(String, Ty)>>,
    /// 枚举变体表：名字 → [(变体名, 载荷类型)]
    pub(crate) enum_variants: HashMap<String, Vec<(String, Vec<Ty>)>>,
    /// trait 名 → 方法名列表（顺序即 vtable 索引）
    #[allow(dead_code)]
    pub(crate) traits: HashMap<String, Vec<String>>,
    /// (类型, trait) → 展平方法名列表
    pub(crate) trait_impls: HashMap<(String, String), Vec<String>>,
    /// 已知上界的循环变量：名字 → 排他上界（`for i in 0..N`）。用于省略数组下标的边界检查。
    pub(crate) bounded: HashMap<String, i64>,
    /// 范围分析结果（判定哪些 Add/Sub/Mul 可免溢出检查）
    pub(crate) range_analysis: Option<crate::range::Analysis>,
    /// 标签循环：待绑定的标签栈（外→内）。内层循环创建时，若栈非空则把该标签登记到自己的 break/continue 目标。
    pub(crate) labeled: Vec<String>,
    /// 已登记的标签 → (break 目标, continue 目标)
    pub(crate) label_targets: HashMap<String, (String, String)>,
    /// 当前 try 的错误目标栈：(catch 标签, 错误值槽)。
    /// `throw`/`?` 命中 Err 时跳到栈顶；栈空时从函数返回（向上传播）。
    pub(crate) err_stack: Vec<(String, String)>,
}

impl<'a> Codegen<'a> {
    // ============================================================
    // 顶层
    // ============================================================

    pub(crate) fn run(&mut self, prog: &Program) -> Result<String, String> {
        // 收集所有枚举变体（名字 → 变体表）
        for item in &prog.items {
            if let Item::Enum(en) = item {
                self.enum_variants.insert(en.name.clone(), en.variants.clone());
            }
        }
        // 收集结构体布局
        for item in &prog.items {
            if let Item::Struct(s) = item {
                let fields: Vec<(String, Ty)> = s
                    .fields
                    .iter()
                    .map(|(n, t, _)| (n.clone(), t.clone().unwrap_or(Ty::I64)))
                    .collect();
                self.structs.insert(s.name.clone(), fields);
            }
        }
        for item in &prog.items {
            if let Item::Fn(f) = item {
                let is_main = f.name == "main";
                self.fns.insert(
                    f.name.clone(),
                    FnInfo {
                        cname: if is_main { "main".into() } else { format!("gt_{}", mangle(&f.name)) },
                        params: f
                            .params
                            .iter()
                            .map(|p| p.ty.clone().unwrap_or(Ty::I64))
                            .collect(),
                        ret: if is_main { Ty::Void } else { f.ret_ty.clone() },
                    },
                );
            }
        }

        // 内联 C 块里的函数：用 C 原名直接声明为外部符号，
        // clang 会把 C 块一起编译进来完成链接，因此 GTLang 可直接调用。
        for cf in &prog.cfuncs {
            let params: Vec<String> =
                cf.params.iter().map(|t| t.llvm().to_string()).collect();
            let sym = Self::llvm_sym(&cf.name);
            let d = if cf.ret == Ty::Void {
                format!("declare void @{}({})", sym, params.join(", "))
            } else {
                format!("declare {} @{}({})", cf.ret.llvm(), sym, params.join(", "))
            };
            self.declare(&d);
            self.fns.insert(
                cf.name.clone(),
                FnInfo { cname: sym, params: cf.params.clone(), ret: cf.ret.clone() },
            );
        }

        for item in &prog.items {
            if let Item::Fn(f) = item {
                self.function(f)?;
            }
        }

        // C → GTLang：把可被 C 调用的 GTLang 函数地址写进桥接槽
        // （桥接源码由 `cblock::bridge_header` 生成，槽名固定为 gt_slot_<i>）
        self.fill_bridge_slots(prog)?;

        let mut out = String::new();
        out.push_str("; GTLang -> LLVM IR（gtc_rust）\n");
        // 显式声明目标平台：否则 clang 会警告 "overriding the module target triple"，
        // 且数据布局缺失会影响聚合类型的 ABI 决策。
        out.push_str("target datalayout = \"e-m:w-p270:32:32-p271:32:32-p272:64:64-");
        out.push_str("i64:64-i128:128-f80:128-n8:16:32:64-S128\"\n");
        out.push_str("target triple = \"x86_64-pc-windows-msvc\"\n\n");
        out.push_str(&format!(
            "source_filename = \"{}\"\n\n",
            self.file.replace('\\', "/")
        ));
        out.push_str(&self.globals);
        if !self.globals.is_empty() {
            out.push('\n');
        }
        let mut ds: Vec<&String> = self.declares.iter().collect();
        ds.sort();
        for d in ds {
            out.push_str(d);
            out.push('\n');
        }
        if !self.declares.is_empty() {
            out.push('\n');
        }
        out.push_str(&self.module);
        Ok(out)
    }

    /// C → GTLang：把可被 C 调用的 GTLang 函数地址写进桥接槽。
    ///
    /// 桥接源码（`cblock::bridge_header`）在 C 侧定义了一组 `void *gt_slot_<i>`，
    /// 每个槽对应 `cblock::gt_functions()` 里的第 i 个函数。这里在 `main` 的 entry
    /// 块最前面插入 store，把真实函数地址填进去；C 侧通过函数指针调用它。
    pub(crate) fn fill_bridge_slots(&mut self, prog: &Program) -> Result<(), String> {
        if prog.cblock.trim().is_empty() {
            return Ok(());
        }
        let fns = crate::cblock::gt_functions(&prog.items);
        if fns.is_empty() {
            return Ok(());
        }
        let mut decls = String::new();
        let mut stores = String::new();
        for (i, f) in fns.iter().enumerate() {
            let cname = self
                .fns
                .get(&f.name)
                .map(|x| x.cname.clone())
                .ok_or_else(|| format!("内部错误：桥接找不到函数 '{}'", f.name))?;
            decls.push_str(&format!("@gt_slot_{} = external global ptr\n", i));
            stores.push_str(&format!("  store ptr @{}, ptr @gt_slot_{}\n", cname, i));
        }
        self.globals.push_str(&decls);

        // 填在 main 的 `entry:` 之后（无 main 时用第一个 `entry:`）
        let at = match self.module.find("define i32 @main() {") {
            Some(p) => {
                let e = "entry:\n";
                self.module[p..].find(e).map(|o| p + o + e.len())
            }
            None => self.module.find("entry:\n").map(|p| p + "entry:\n".len()),
        };
        if let Some(at) = at {
            self.module.insert_str(at, &stores);
        }
        Ok(())
    }

    // ============================================================
    // 函数
    // ============================================================

    pub(crate) fn function(&mut self, f: &FnDef) -> Result<(), String> {
        let info = self.fns.get(&f.name).cloned().unwrap();
        let is_main = f.name == "main";

        self.entry_allocas.clear();
        self.body.clear();
        self.scopes.clear();
        self.scopes.push(HashMap::new());
        self.cur_ret = info.ret.clone();
        self.terminated = false;
        self.break_label = None;
        self.continue_label = None;
        self.var_cache.clear();
        self.perm_ptrs.clear();
        self.perm_cache.clear();

        // 找出"从不被重新赋值、且不在循环体内声明"的变量：它们可以跨基本块
        // 保持 SSA 值，从而在每个分支里省掉一次 load。
        let immutable = immutable_vars(&f.body);
        self.immutable_lets = immutable.clone();

        let mut sig = String::new();
        for (i, p) in f.params.iter().enumerate() {
            if i > 0 {
                sig.push_str(", ");
            }
            let pty = p.ty.clone().unwrap_or(Ty::I64);
            sig.push_str(&format!("{} %a{}", pty.llvm(), i));
        }

        for (i, p) in f.params.iter().enumerate() {
            let pty = p.ty.clone().unwrap_or(Ty::I64);
            let slot = self.new_alloca(&pty);
            let loc = Local { ptr: slot, ty: pty };
            if immutable.contains(&p.name) {
                self.perm_ptrs.insert(loc.ptr.clone());
            }
            // 形参先落栈，同时进缓存，避免函数开头就读一次自己的参数
            self.store(&loc, &Val::new(&loc.ty, format!("%a{}", i)))?;
            self.scopes.last_mut().unwrap().insert(p.name.clone(), loc);
        }

        if is_main {
            // 若以 zh 模式编译，让运行时诊断也用中文
            if crate::lang::is_zh() {
                self.declare("declare void @gt_rt_set_zh()");
                self.body.push_str("  call void @gt_rt_set_zh()\n");
                self.declare("declare void @gt_rt_init()");
                self.body.push_str("  call void @gt_rt_init()\n");
            }
            self.block(&f.body)?;
            if !self.terminated {
                self.body.push_str("  ret i32 0\n");
            }
        } else if info.ret == Ty::Void {
            self.block(&f.body)?;
            if !self.terminated {
                self.body.push_str("  ret void\n");
            }
        } else {
            let tail = self.block_ret(&f.body, &info.ret)?;
            if !self.terminated {
                match tail {
                    Some(v) => {
                        let v = self.coerce(&v, &info.ret)?;
                        self.body
                            .push_str(&format!("  ret {} {}\n", info.ret.llvm(), v.s));
                    }
                    None => self.body.push_str(&format!(
                        "  ret {} {}\n",
                        info.ret.llvm(),
                        info.ret.zero()
                    )),
                }
            }
        }

        let ret_llvm = if is_main { "i32".to_string() } else { info.ret.llvm() };
        self.module
            .push_str(&format!("define {} @{}({}) {{\n", ret_llvm, info.cname, sig));
        self.module.push_str("entry:\n");
        for a in &self.entry_allocas {
            self.module.push_str(a);
            self.module.push('\n');
        }
        self.module.push_str(&self.body);
        self.module.push_str("}\n\n");
        Ok(())
    }

    // ============================================================
    // 语句
    // ============================================================

    pub(crate) fn block(&mut self, b: &Block) -> Result<(), String> {
        for s in b {
            self.stmt(s)?;
        }
        Ok(())
    }

    /// 生成块；块尾表达式作为块的值返回
    pub(crate) fn block_ret(&mut self, b: &Block, want: &Ty) -> Result<Option<Val>, String> {
        if b.is_empty() {
            return Ok(None);
        }
        let n = b.len();
        for s in &b[..n - 1] {
            self.stmt(s)?;
        }
        self.stmt_value(&b[n - 1], want)
    }

    pub(crate) fn stmt_value(&mut self, s: &Stmt, want: &Ty) -> Result<Option<Val>, String> {
        if self.terminated {
            return Ok(None);
        }
        match s {
            // 块尾表达式即块的值：即使是纯字面量也必须求值（`fn f() { 42 }`）
            Stmt::Expr(e) if want != &Ty::Void => Ok(Some(self.expr(e)?)),
            Stmt::If { cond, then, els, line } if want != &Ty::Void => Ok(Some(self.if_value(
                cond,
                then,
                els.as_ref(),
                want,
                *line,
            )?)),
            Stmt::Block(inner) => self.block_ret(inner, want),
            other => {
                self.stmt(other)?;
                Ok(None)
            }
        }
    }

    pub(crate) fn stmt(&mut self, s: &Stmt) -> Result<(), String> {
        if self.terminated {
            return Ok(());
        }
        match s {
            Stmt::Let { name, value, .. } => {
                let v = self.expr(value)?;
                let ty = if v.ty == Ty::Unknown { Ty::I64 } else { v.ty.clone() };
                let slot = self.new_alloca(&ty);
                let v = self.coerce(&v, &ty)?;
                let loc = Local { ptr: slot, ty };
                if self.immutable_lets.contains(name) {
                    self.perm_ptrs.insert(loc.ptr.clone());
                }
                self.store(&loc, &v)?;
                self.scopes.last_mut().unwrap().insert(name.clone(), loc);
            }
            Stmt::Const { name, value, .. } => {
                let v = self.expr(value)?;
                let ty = if v.ty == Ty::Unknown { Ty::I64 } else { v.ty.clone() };
                let slot = self.new_alloca(&ty);
                let v = self.coerce(&v, &ty)?;
                let loc = Local { ptr: slot, ty };
                self.perm_ptrs.insert(loc.ptr.clone());
                self.store(&loc, &v)?;
                self.scopes.last_mut().unwrap().insert(name.clone(), loc);
            }
            Stmt::Go { func, args, line } => {
                // go f(args)：取函数地址 + 参数指针，调用运行时 thread_spawn
                let info = self.fns.get(func).cloned().ok_or_else(|| crate::lb!(line, "undefined function '{}'", "未定义的函数 '{}'", func))?;
                let fref = format!("@{}", info.cname);
                let _ = fref;
                // 参数打包到堆（简化：逐个存 i64）
                self.declare("declare ptr @gt_mem_alloc(i64)");
                let np = args.len().max(1);
                let pack = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_mem_alloc(i64 {})\n", pack, np * 8));
                for (i, a) in args.iter().enumerate() {
                    let v = self.expr(a)?;
                    let s = self.to_slot(&v);
                    let p = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 {}\n", p, pack, i));
                    self.body.push_str(&format!("  store i64 {}, ptr {}\n", s, p));
                }
                let fp = self.new_reg();
                self.body.push_str(&format!("  {} = ptrtoint ptr @{} to i64\n", fp, info.cname));
                self.declare("declare void @gt_thread_spawn(i64, ptr, i64)");
                self.body.push_str(&format!("  call void @gt_thread_spawn(i64 {}, ptr {}, i64 {})\n", fp, pack, args.len()));
            }
            Stmt::Asm { lines, .. } => {
                // 内联汇编：LLVM `call void asm sideeffect "指令", ""()`
                for ins in lines {
                    let escaped = ins.replace('\\', "\\\\").replace('"', "\\22");
                    self.body.push_str(&format!("  call void asm sideeffect \"{}\", \"\"()\n", escaped));
                }
            }
            Stmt::Throw(e, _line) => {
                // `throw e`：构造 Err(e)。在 try 内跳到捕获块；否则从函数返回（向上传播）。
                let v = self.expr(e)?;
                let slotv = self.to_slot(&v);
                self.declare("declare ptr @gt_result_new(i64, i64)");
                let errv = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_result_new(i64 1, i64 {})\n", errv, slotv));
                if let Some((lbl, slot)) = self.err_stack.last().cloned() {
                    self.declare("declare i64 @gt_result_val(ptr)");
                    let pv = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_result_val(ptr {})\n", pv, errv));
                    self.body.push_str(&format!("  store i64 {}, ptr {}\n", pv, slot));
                    self.body.push_str(&format!("  br label %{}\n", lbl));
                } else {
                    self.body.push_str(&format!("  ret ptr {}\n", errv));
                }
                self.terminated = true;
            }
            Stmt::Try { body, catches, fin, .. } => {
                // `try { body } expt e { h } fily { f }`：
                // body 内 throw/? 命中的 Err 跳到 expt；fily 总执行。
                let slot = self.new_alloca(&Ty::I64);   // try 的值槽
                let err_slot = self.new_alloca(&Ty::I64); // 错误值槽（throw/? 写入）
                let l_handler = self.new_label();       // 统一的 expt 入口
                let l_end = self.new_label();
                self.err_stack.push((l_handler.clone(), err_slot.clone()));
                // body 作为普通语句块执行；?/throw 命中 Err 时经 err_stack 跳到 l_handler。
                self.block(body)?;
                self.err_stack.pop();
                if !self.terminated {
                    // 正常结束：把 0 存入值槽（try 语句的值无意义）
                    self.body.push_str(&format!("  store i64 0, ptr {}\n", slot));
                    self.body.push_str(&format!("  br label %{}\n", l_end));
                }
                // ---- expt 处理 ----
                self.emit_label(&l_handler);
                self.terminated = false;
                let errv = self.new_reg();
                self.body.push_str(&format!("  {} = load i64, ptr {}\n", errv, err_slot));
                if let Some(ca) = catches.first() {
                    self.push_scope();
                    if let Some(binding) = &ca.binding {
                        let bslot = self.new_alloca(&Ty::I64);
                        self.body.push_str(&format!("  store i64 {}, ptr {}\n", errv, bslot));
                        self.scopes.last_mut().unwrap().insert(binding.clone(), Local { ptr: bslot, ty: Ty::I64 });
                    }
                    let hv = self.block_ret(&ca.body, &Ty::I64)?;
                    if let Some(hv) = hv {
                        let hs = self.as_i64(&hv);
                        self.body.push_str(&format!("  store i64 {}, ptr {}\n", hs, slot));
                    }
                    self.pop_scope();
                }
                if !self.terminated {
                    self.body.push_str(&format!("  br label %{}\n", l_end));
                }
                self.emit_label(&l_end);
                self.terminated = false;
                // fily
                if let Some(f) = fin {
                    self.push_scope();
                    self.block(f)?;
                    self.pop_scope();
                }
                let _ = slot;
            }
            Stmt::Assign { name, index, op, value, line } => {
                let loc = match self.lookup(name) {
                    Some(l) => l,
                    None if index.is_none() && op.is_none() => {
                        // 裸赋值 x = v：自动声明（类型由 RHS 推导）
                        let rhs = self.expr(value)?;
                        let ty = if rhs.ty == Ty::Unknown { Ty::I64 } else { rhs.ty.clone() };
                        let slot = self.new_alloca(&ty);
                        let v = self.coerce(&rhs, &ty)?;
                        self.store(&Local { ptr: slot.clone(), ty: ty.clone() }, &v)?;
                        let loc = Local { ptr: slot, ty };
                        self.scopes.last_mut().unwrap().insert(name.clone(), loc.clone());
                        return Ok(());
                    }
                    None => return Err(crate::lb!(line, "undefined variable '{}'", "未定义的变量 '{}'", name)),
                };
                match index {
                    // 数组元素赋值：`a[i] = v` / `a[i] op= v`
                    Some(idx) => {
                        // List / Map：走运行时 set / insert（引用语义句柄）
                        if matches!(loc.ty, Ty::List(_) | Ty::Map(..)) {
                            let base = self.load(&loc)?;
                            let iv = self.expr(idx)?;
                            let rhs = self.expr(value)?;
                            match &loc.ty {
                                Ty::List(el) => {
                                    let i = self.as_i64(&iv);
                                    let rv = self.coerce(&rhs, el)?;
                                    self.declare("declare void @gt_list_set(ptr, i64, i64)");
                                    self.body.push_str(&format!("  call void @gt_list_set(ptr {}, i64 {}, i64 {})\n", base.s, i, rv.s));
                                }
                                Ty::Map(_, v) => {
                                    let k = self.to_slot(&iv);
                                    let rv = self.coerce(&rhs, v)?;
                                    self.declare("declare void @gt_map_insert(ptr, i64, i64)");
                                    self.body.push_str(&format!("  call void @gt_map_insert(ptr {}, i64 {}, i64 {})\n", base.s, k, rv.s));
                                }
                                _ => unreachable!(),
                            }
                            return Ok(());
                        }
                        let (elem, p) = self.elem_ptr(&loc, idx, *line)?;
                        let rhs = self.expr(value)?;
                        let rs = match op {
                            Some(bop) => {
                                let cur = self.new_reg();
                                self.body.push_str(&format!(
                                    "  {} = load {}, ptr {}\n",
                                    cur,
                                    elem.llvm(),
                                    p
                                ));
                                let cur = Val::new(&elem, cur);
                                self.binary(*bop, &cur, &rhs, *line, false)?
                            }
                            None => rhs,
                        };
                        let rs = self.coerce(&rs, &elem)?;
                        self.body.push_str(&format!(
                            "  store {} {}, ptr {}\n",
                            elem.llvm(),
                            rs.s,
                            p
                        ));
                    }
                    // 整体赋值 `x = v`
                    None => {
                        let rhs = self.expr(value)?;
                        let rs = match op {
                            Some(bop) => {
                                let cur = self.load(&loc)?;
                                let safe = self.range_analysis.as_ref().map(|ra| ra.is_stmt_safe(s)).unwrap_or(false);
                                self.binary(*bop, &cur, &rhs, *line, safe)?
                            }
                            None => rhs,
                        };
                        // 类型改变：重新分配新类型的槽（自动推导 / let mut 可改类型）
                        if op.is_none() && rs.ty != Ty::Unknown && rs.ty != loc.ty {
                            let nty = rs.ty.clone();
                            let nslot = self.new_alloca(&nty);
                            let nloc = Local { ptr: nslot, ty: nty };
                            let rs = self.coerce(&rs, &nloc.ty)?;
                            self.store(&nloc, &rs)?;
                            self.scopes.last_mut().unwrap().insert(name.clone(), nloc);
                        } else {
                            let rs = self.coerce(&rs, &loc.ty)?;
                            self.store(&loc, &rs)?;
                        }
                    }
                }
            }
            Stmt::Expr(e) => {
                if !is_pure(e) {
                    self.expr(e)?;
                }
            }
            Stmt::If { cond, then, els, .. } => {
                self.if_value(cond, then, els.as_ref(), &Ty::Void, 0)?;
            }
            Stmt::While { cond, body, .. } => {
                let lcond = self.new_label();
                let lbody = self.new_label();
                let lend = self.new_label();

                self.body.push_str(&format!("  br label %{}\n", lcond));
                self.emit_label(&lcond);
                let c = self.cond(cond)?;
                self.body.push_str(&format!(
                    "  br i1 {}, label %{}, label %{}\n",
                    c, lbody, lend
                ));
                self.emit_label(&lbody);

                let (ob, oc) = (self.break_label.clone(), self.continue_label.clone());
                self.break_label = Some(lend.clone());
                self.continue_label = Some(lcond.clone());
                if let Some(lbl) = self.labeled.last().cloned() { self.label_targets.insert(lbl, (lend.clone(), lcond.clone())); }
                self.push_scope();
                self.block(body)?;
                self.pop_scope();
                self.break_label = ob;
                self.continue_label = oc;

                if !self.terminated {
                    self.body.push_str(&format!("  br label %{}\n", lcond));
                }
                self.emit_label(&lend);
            }
            Stmt::Labeled { label, inner, .. } => {
                // 标签循环：登记标签；内层循环创建时把该标签映射到自己的 break/continue 目标
                self.labeled.push(label.clone());
                let inner_blk: Block = vec![(**inner).clone()];
                self.block(&inner_blk)?;
                self.labeled.pop();
            }
            Stmt::DoWhile { body, cond, .. } => {
                // 先执行 body，末尾判 cond：真→回到 body，假→退出
                let lbody = self.new_label();
                let lcond = self.new_label();
                let lend = self.new_label();
                self.body.push_str(&format!("  br label %{}\n", lbody));
                self.emit_label(&lbody);
                let (ob, oc) = (self.break_label.clone(), self.continue_label.clone());
                self.break_label = Some(lend.clone());
                self.continue_label = Some(lcond.clone());
                self.push_scope();
                self.block(body)?;
                self.pop_scope();
                self.break_label = ob;
                self.continue_label = oc;
                if !self.terminated {
                    self.body.push_str(&format!("  br label %{}\n", lcond));
                }
                self.emit_label(&lcond);
                let c = self.cond(cond)?;
                self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", c, lbody, lend));
                self.emit_label(&lend);
            }
            Stmt::ForRange { var, from, to, body, els, .. } => {
                let fv = self.expr(from)?;
                let fv = self.coerce(&fv, &Ty::I64)?;
                let tv = self.expr(to)?;
                let tv = self.coerce(&tv, &Ty::I64)?;
                let iv = self.new_alloca(&Ty::I64);
                self.body
                    .push_str(&format!("  store i64 {}, ptr {}\n", fv.s, iv));

                let lcond = self.new_label();
                let lbody = self.new_label();
                let linc = self.new_label();
                let lend = self.new_label();

                self.body.push_str(&format!("  br label %{}\n", lcond));
                self.emit_label(&lcond);
                let cur = self.new_reg();
                self.body
                    .push_str(&format!("  {} = load i64, ptr {}\n", cur, iv));
                let cmp = self.new_reg();
                self.body
                    .push_str(&format!("  {} = icmp slt i64 {}, {}\n", cmp, cur, tv.s));
                self.body.push_str(&format!(
                    "  br i1 {}, label %{}, label %{}\n",
                    cmp, lbody, lend
                ));

                self.emit_label(&lbody);
                let (ob, oc) = (self.break_label.clone(), self.continue_label.clone());
                self.break_label = Some(lend.clone());
                self.continue_label = Some(linc.clone());
                if let Some(lbl) = self.labeled.last().cloned() { self.label_targets.insert(lbl, (lend.clone(), linc.clone())); }
                self.push_scope();
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(var.clone(), Local { ptr: iv.clone(), ty: Ty::I64 });
                // 记录 `for v in 0..N` 的上界，用于省略 a[v] 的边界检查
                let bounded = matches!(&from.kind, ExprKind::Int(0))
                    .then(|| if let ExprKind::Int(n) = &to.kind { Some(*n) } else { None })
                    .flatten();
                if let Some(n) = bounded { self.bounded.insert(var.clone(), n); }
                self.block(body)?;
                if bounded.is_some() { self.bounded.remove(var); }
                self.pop_scope();
                self.break_label = ob;
                self.continue_label = oc;

                if !self.terminated {
                    self.body.push_str(&format!("  br label %{}\n", linc));
                }
                self.emit_label(&linc);
                let c2 = self.new_reg();
                self.body
                    .push_str(&format!("  {} = load i64, ptr {}\n", c2, iv));
                let nx = self.new_reg();
                self.body
                    .push_str(&format!("  {} = add i64 {}, 1\n", nx, c2));
                self.body
                    .push_str(&format!("  store i64 {}, ptr {}\n", nx, iv));
                self.body.push_str(&format!("  br label %{}\n", lcond));
                self.emit_label(&lend);
                // for-else：正常结束（i 达到终值）才执行 else
                if let Some(els) = els {
                    let lsel = self.new_label();
                    let lskip = self.new_label();
                    let ci = self.new_reg();
                    self.body.push_str(&format!("  {} = load i64, ptr {}\n", ci, iv));
                    let done = self.new_reg();
                    self.body.push_str(&format!("  {} = icmp eq i64 {}, {}\n", done, ci, tv.s));
                    self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", done, lsel, lskip));
                    self.emit_label(&lsel);
                    self.terminated = false;
                    self.push_scope();
                    self.block(els)?;
                    self.pop_scope();
                    if !self.terminated { self.body.push_str(&format!("  br label %{}\n", lskip)); }
                    self.emit_label(&lskip);
                    self.terminated = false;
                }
            }
            Stmt::ForEach { var, iter, body, els: _, line } => {
                let arr = self.expr(iter)?;
                let is_list = matches!(&arr.ty, Ty::List(_));
                let elem = match &arr.ty {
                    Ty::Array(e, _) => (**e).clone(),
                    Ty::List(e) => (**e).clone(),
                    other => return Err(crate::lb!(line, "for can only iterate over arrays/lists, found {}", "for 只能遍历数组/列表，实际是 {}", other)),
                };
                let n: i64 = match &arr.ty { Ty::Array(_, n) => *n as i64, _ => 0 };
                let idx = self.new_alloca(&Ty::I64);
                let ev = self.new_alloca(&elem);
                self.body.push_str(&format!("  store i64 0, ptr {}\n", idx));
                let lcond = self.new_label();
                let lbody = self.new_label();
                let linc = self.new_label();
                let lend = self.new_label();
                self.body.push_str(&format!("  br label %{}\n", lcond));
                self.emit_label(&lcond);
                let i1 = self.new_reg();
                self.body.push_str(&format!("  {} = load i64, ptr {}\n", i1, idx));
                let c = self.new_reg();
                if is_list {
                    self.declare("declare i64 @gt_list_len(ptr)");
                    let lenr = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_list_len(ptr {})\n", lenr, arr.s));
                    self.body.push_str(&format!("  {} = icmp slt i64 {}, {}\n", c, i1, lenr));
                } else {
                    self.body.push_str(&format!("  {} = icmp slt i64 {}, {}\n", c, i1, n));
                }
                self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", c, lbody, lend));
                self.emit_label(&lbody);
                let (ob, oc) = (self.break_label.clone(), self.continue_label.clone());
                self.break_label = Some(lend.clone());
                self.continue_label = Some(linc.clone());
                // 取元素值写入 ev
                if is_list {
                    self.declare("declare i64 @gt_list_at(ptr, i64)");
                    let raw = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_list_at(ptr {}, i64 {})\n", raw, arr.s, i1));
                    let sv = self.from_slot(&raw, &elem);
                    self.body.push_str(&format!("  store {} {}, ptr {}\n", elem.llvm(), sv, ev));
                } else {
                    let off = self.new_reg();
                    self.body.push_str(&format!("  {} = mul i64 {}, {}\n", off, i1, elem.llvm().replace("ptr","8").replace("i64","8").replace("double","8").replace("i1","8")));
                    let p = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr i8, ptr {}, i64 {}\n", p, arr.s, off));
                    let ld = self.new_reg();
                    self.body.push_str(&format!("  {} = load {}, ptr {}\n", ld, elem.llvm(), p));
                    self.body.push_str(&format!("  store {} {}, ptr {}\n", elem.llvm(), ld, ev));
                }
                self.push_scope();
                self.scopes.last_mut().unwrap().insert(var.clone(), Local { ptr: ev.clone(), ty: elem.clone() });
                self.block(body)?;
                self.pop_scope();
                self.break_label = ob;
                self.continue_label = oc;
                self.body.push_str(&format!("  br label %{}\n", linc));
                self.emit_label(&linc);
                let i2 = self.new_reg();
                self.body.push_str(&format!("  {} = load i64, ptr {}\n", i2, idx));
                let nx = self.new_reg();
                self.body.push_str(&format!("  {} = add i64 {}, 1\n", nx, i2));
                self.body.push_str(&format!("  store i64 {}, ptr {}\n", nx, idx));
                self.body.push_str(&format!("  br label %{}\n", lcond));
                self.emit_label(&lend);
            }
            Stmt::Return(e, _) => {
                if self.cur_ret == Ty::Void {
                    self.body.push_str("  ret void\n");
                } else {
                    let v = match e {
                        Some(e) => self.expr(e)?,
                        None => Val::new(&self.cur_ret.clone(), self.cur_ret.zero()),
                    };
                    let rt = self.cur_ret.clone();
                    let v = self.coerce(&v, &rt)?;
                    self.body
                        .push_str(&format!("  ret {} {}\n", rt.llvm(), v.s));
                }
                self.terminated = true;
            }
            Stmt::Break(lbl, _) => {
                let l = match lbl {
                    Some(n) => self.label_targets.get(n).map(|(b, _)| b.clone()),
                    None => self.break_label.clone(),
                }
                .ok_or_else(|| "break 只能出现在循环内".to_string())?;
                self.body.push_str(&format!("  br label %{}\n", l));
                self.terminated = true;
            }
            Stmt::Continue(lbl, _) => {
                let l = match lbl {
                    Some(n) => self.label_targets.get(n).map(|(_, c)| c.clone()),
                    None => self.continue_label.clone(),
                }
                .ok_or_else(|| "continue 只能出现在循环内".to_string())?;
                self.body.push_str(&format!("  br label %{}\n", l));
                self.terminated = true;
            }
            Stmt::Block(b) => {
                // 不引入新作用域：与 sema/jit 对齐（解构块依赖此）
                self.block(b)?;
            }
            // 嵌套函数已在 hoist 阶段提升为顶层
            Stmt::LocalFn(_) => {}
            Stmt::FieldAssign { obj, field, op, value, line } => {
                let loc = self
                    .lookup(obj)
                    .ok_or_else(|| crate::lb!(line, "undefined variable '{}'", "未定义的变量 '{}'", obj))?;
                let sname = match &loc.ty {
                    Ty::Struct(n) => n.clone(),
                    other => {
                        return Err(crate::lb!(line, "{} is not a struct; cannot access field", "{} 不是结构体，不能访问字段", other))
                    }
                };
                let layout = self
                    .structs
                    .get(&sname)
                    .cloned()
                    .ok_or_else(|| crate::lb!(line, "undefined struct '{}'", "未定义的结构体 '{}'", sname))?;
                let idx = layout
                    .iter()
                    .position(|(n, _)| n == field)
                    .ok_or_else(|| crate::lb!(line, "struct '{}' has no field '{}'", "结构体 '{}' 没有字段 '{}'", sname, field))?;
                let fty = layout[idx].1.clone();
                let arrty = format!("[{} x i64]", layout.len().max(1));
                // 取结构体指针的**值**（alloca 里存的是指向结构体内存的指针）
                let basev = self.load(&loc)?;
                let base = basev.s;
                let p = self.new_reg();
                self.body.push_str(&format!(
                    "  {} = getelementptr inbounds {}, ptr {}, i64 0, i64 {}\n",
                    p, arrty, base, idx
                ));
                let rhs = self.expr(value)?;
                let res = match op {
                    Some(o) => {
                        let raw = self.new_reg();
                        self.body.push_str(&format!("  {} = load i64, ptr {}\n", raw, p));
                        let cur = self.from_slot(&raw, &fty);
                        let curv = Val::new(&fty, cur);
                        self.binary(*o, &curv, &rhs, *line, false)?
                    }
                    None => self.coerce(&rhs, &fty)?,
                };
                let slotv = self.to_slot(&res);
                self.body.push_str(&format!("  store i64 {}, ptr {}\n", slotv, p));
            }
        }
        Ok(())
    }

}

mod expr;
mod call;
#[allow(unused_imports)]
pub(crate) use expr::*;
pub(crate) use call::*;
