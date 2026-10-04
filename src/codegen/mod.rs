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

/// 容器的元素/键/值是否为"堆指针类型"（list/set/map/str/struct/enum → 1）。
/// 供 GC 引用计数与环检测判断"是否需要遍历子引用"。
pub fn elem_is_ptr(t: &Ty) -> i64 {
    match t {
        Ty::List(_) | Ty::Set(_) | Ty::Map(..) | Ty::Str | Ty::Struct(_) | Ty::Enum(_) | Ty::Tuple(_) => 1,
        _ => 0,
    }
}

/// 是否启用整数溢出检查（默认开）。`--no-overflow-check` 关闭后加减乘用回绕指令。
static OVERFLOW_CHECK: AtomicBool = AtomicBool::new(true);

/// 是否启用自动内存管理（默认开）。`--no-gc` 关闭后生成的程序不回收（靠进程结束回收）。
static GC_ENABLED: AtomicBool = AtomicBool::new(true);
pub fn set_gc(on: bool) { GC_ENABLED.store(on, Ordering::Relaxed); }
pub fn gc_enabled() -> bool { GC_ENABLED.load(Ordering::Relaxed) }

/// 供 JIT 查询"溢出检查是否启用"（双端同步）。
pub fn overflow_check_enabled_pub() -> bool { overflow_check_enabled() }

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
        is_main_fn: false,
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
    /// `s` 当前的 LLVM 表示：true 表示 `ptr`，false 表示 `i64`（含容器句柄以 i64 流转的情形）。
    is_ptr: bool,
}

impl Val {
    /// 自动形态：按类型的 LLVM 表示确定。**要求 `s` 的实际形态与之一致**
    /// （ptr 类类型必须传 ptr 值，数值/布尔必须传 i64/i1 值）。
    pub(crate) fn new(ty: &Ty, s: impl Into<String>) -> Val {
        let is_ptr = ty.llvm() == "ptr";
        let s = s.into();
        // 形态不变式：ptr 类的值不应是"纯数字字面量"（那多半是 i64 被错标）
        debug_assert!(
            !(is_ptr && !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit() || c == b'-') && s != "null"),
            "Val::new: ptr 类类型 {:?} 收到了数字字面量 '{}'（形态疑似错标）", ty, s
        );
        Val { ty: ty.clone(), s, is_ptr }
    }
    /// 以 i64（slot）形态构造：仅用于**非 ptr 类**类型。
    /// 容器/字符串句柄在运行期以 i64 流转，但取出后应立即 inttoptr 成 ptr 形态，
    /// 不应把 ptr 类类型标成 slot —— 这里用断言固化该不变式。
    pub(crate) fn new_slot(ty: &Ty, s: impl Into<String>) -> Val {
        debug_assert!(
            ty.llvm() != "ptr" || matches!(ty, Ty::Unknown),
            "Val::new_slot 不应作用于 ptr 类类型（{:?}）——请先 inttoptr 并用 Val::new",
            ty
        );
        Val { ty: ty.clone(), s: s.into(), is_ptr: false }
    }
    /// 以 ptr 形态构造：仅用于 ptr 类类型。
    pub(crate) fn new_ptr(ty: &Ty, s: impl Into<String>) -> Val {
        debug_assert!(
            ty.llvm() == "ptr" || matches!(ty, Ty::Unknown),
            "Val::new_ptr 只应用于 ptr 类类型（{:?}）",
            ty
        );
        Val { ty: ty.clone(), s: s.into(), is_ptr: true }
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
    /// 当前函数是否为 main（main 的 IR 签名是 i32，return 需生成 ret i32 0）
    pub(crate) is_main_fn: bool,
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
        self.is_main_fn = is_main;
        self.terminated = false;
        self.break_label = None;
        self.continue_label = None;
        self.var_cache.clear();
        self.perm_ptrs.clear();
        self.perm_cache.clear();

        // 找出"从不被重新赋值、且不在循环体内声明"的变量：它们可以跨基本块
        // 保持 SSA 值，从而在每个分支里省掉一次 load。
        let mut immutable = immutable_vars(&f.body);
        // 形参：默认也视为 immutable（除非函数体里被重新赋值）——
        // 这样其 SSA 值可跨块复用，IR 更接近纯 SSA，便于 LLVM 优化（如递归→循环）。
        {
            let mut assigned = std::collections::HashSet::new();
            let mut declared = std::collections::HashSet::new();
            crate::codegen::call::scan_mutation(&f.body, false, &mut assigned, &mut declared);
            for p in &f.params {
                if !assigned.contains(&p.name) {
                    immutable.insert(p.name.clone());
                }
            }
        }
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
            // --no-gc：生成程序里关闭自动内存管理
            if !gc_enabled() {
                self.declare("declare void @gt_rt_set_gc(i32)");
                self.body.push_str("  call void @gt_rt_set_gc(i32 0)\n");
            }
            // 进程初始化：设置控制台 UTF-8 + stdout 二进制（必须无条件调用，
            // 否则 Windows 文本模式会把 \n 转成 \r\n，与 JIT 不一致）
            self.declare("declare void @gt_rt_init()");
            self.body.push_str("  call void @gt_rt_init()\n");
            // 若以 zh 模式编译，让运行时诊断也用中文
            if crate::lang::is_zh() {
                self.declare("declare void @gt_rt_set_zh()");
                self.body.push_str("  call void @gt_rt_set_zh()\n");
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

}

mod expr;
mod call;
#[path = "stmt.rs"]
mod stmt;
#[path = "builtins.rs"]
mod builtins;
#[path = "value.rs"]
mod value;
#[allow(unused_imports)]
pub(crate) use expr::*;
#[allow(unused_imports)]
pub(crate) use call::*;
#[allow(unused_imports)]
pub(crate) use builtins::*;
#[allow(unused_imports)]
pub(crate) use value::*;

#[path = "aux_tests.rs"]
#[cfg(test)]
mod aux_tests;

