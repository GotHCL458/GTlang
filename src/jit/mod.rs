//! Cranelift JIT 后端。
//!
//! 直接从 AST 生成机器码并在内存中执行，**完全不需要 clang / LLVM**。
//! 输出经 Rust 标准输出，Windows 控制台的 UTF-8 编码由 std 自动处理。
//!
//! 类型策略（简化 ABI，避免位宽细节）：
//! - 整数 / 布尔 / 字符串 / 数组 一律用 I64（布尔取 0/1，指针即地址）
//! - 浮点用 F64
//! - 分支条件用 `icmp ne x, 0` 转成 i8 供 brif 使用

use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    types, AbiParam, Block as ClBlock, InstBuilder, MemFlags, StackSlot, StackSlotData,
    StackSlotKind, TrapCode, Type, Value,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};

use crate::ast::*;
use crate::codegen::mangle;
use crate::sema::{Analysis, ConstVal, Value as CVal};
use crate::types::builtin_ret;

/// 取某段字符串数据的地址（I64 表示的指针）
fn data_ptr(jit: &mut Jit, b: &mut FunctionBuilder, id: u64) -> Value {
    let gv = jit
        .module
        .declare_data_in_func(DataId::from_u32(id as u32), b.func);
    b.ins().symbol_value(types::I64, gv)
}


mod rt;
pub(crate) mod symbol;
use rt::*;
use symbol::*;

// ============================================================
// JIT 主体
// ============================================================

struct FnInfo {
    fid: FuncId,
    params: Vec<Ty>,
    ret: Ty,
}

pub struct Jit<'a> {
    module: JITModule,
    fns: HashMap<String, FnInfo>,
    strings: HashMap<Vec<u8>, u64>,
    data_ids: Vec<u64>,
    consts: &'a HashMap<String, ConstVal>,
    /// 运行时符号的 FuncId
    rt: HashMap<&'static str, FuncId>,
    /// 结构体布局：名字 → [(字段名, 类型)]
    structs: HashMap<String, Vec<(String, Ty)>>,
    /// 枚举变体：名字 → [(变体名, 载荷类型)]
    enum_variants: HashMap<String, Vec<(String, Vec<Ty>)>>,
    /// trait 名 → 方法名列表（顺序即 vtable 索引）
    traits: HashMap<String, Vec<String>>,
    /// (类型, trait) → 展平方法名列表
    trait_impls: HashMap<(String, String), Vec<String>>,
    /// 整数范围分析（用于省略可证明安全的溢出检查；与 LLVM 后端一致）
    range: Option<crate::range::Analysis>,
}

/// JIT 内部统一用 I64 / F64
fn cl_ty(t: &Ty) -> Type {
    if t.is_float() {
        types::F64
    } else {
        types::I64
    }
}

fn cname(gt: &str) -> String {
    if gt == "main" {
        "gt_main".into()
    } else {
        format!("gt_{}", mangle(gt))
    }
}

/// 运行整个程序：JIT 编译后调用 main
pub fn run(prog: &Program, an: &Analysis, file: &str) -> Result<(), String> {
    enable_vt();

    // 内联 C 块：用 libtcc 在内存里动态编译，取出各函数地址供 JIT 调用。
    // 必须在构建 JIT 之前完成——符号地址要在 JITBuilder 阶段注册。
    let tcc_sess = compile_cblock(prog)?;
    let mut caddrs: Vec<(String, usize)> = Vec::new();
    for cf in &prog.cfuncs {
        // 优先从 tcc 会话取（C 块里定义的函数）；取不到再从 CRT / 当前进程解析
        // （extern "C" 声明的 libc / 系统函数）。
        let from_tcc = tcc_sess.as_ref().and_then(|s| s.symbol(&cf.name));
        let addr = match from_tcc {
            Some(a) => a,
            None => resolve_host_symbol(&cf.name).ok_or_else(|| {
                format!("函数 '{}' 找不到符号（C 块或系统库中均无）", cf.name)
            })?,
        };
        caddrs.push((cf.name.clone(), addr));
    }

    let mut jit = Jit::new(an, &caddrs)?;
    // 收集结构体布局
    for item in &prog.items {
        if let Item::Enum(en) = item {
            jit.enum_variants.insert(en.name.clone(), en.variants.clone());
        }
    }
    for item in &prog.items {
        if let Item::Struct(s) = item {
            let fields: Vec<(String, Ty)> = s
                .fields
                .iter()
                .map(|(n, t, _)| (n.clone(), t.clone().unwrap_or(Ty::I64)))
                .collect();
            jit.structs.insert(s.name.clone(), fields);
        }
    }
    jit.range = Some(crate::range::analyze(prog));
    jit.declare_all(prog)?;
    jit.intern_all(prog)?;

    let mut fbctx = FunctionBuilderContext::new();
    for item in &prog.items {
        if let Item::Fn(f) = item {
            jit.gen_function(f, &mut fbctx)?;
        }
    }

    jit.module
        .finalize_definitions()
        .map_err(|e| format!("JIT 最终化失败：{}", e))?;

    let info = jit
        .fns
        .get("main")
        .ok_or_else(|| format!("{}：未找到 main 函数（JIT 需要一个 main 入口）", file))?;
    let main_fid = info.fid;

    // C → GTLang：把刚 finalize 出来的 GTLang 函数地址写进桥接槽，
    // 这样 C 侧通过函数指针就能回调 GTLang（双向无缝交互）。
    if let Some(sess) = &tcc_sess {
        let fns = crate::cblock::gt_functions(&prog.items);
        for (i, f) in fns.iter().enumerate() {
            let slot = format!("gt_slot_{}", i);
            let slot_addr = sess.symbol(&slot).ok_or_else(|| {
                format!("内部错误：找不到桥接槽 {}（C 桥接头未生成？）", slot)
            })?;
            let fid = jit
                .fns
                .get(&f.name)
                .map(|x| x.fid)
                .ok_or_else(|| format!("内部错误：桥接找不到函数 '{}'", f.name))?;
            let faddr = jit.module.get_finalized_function(fid) as usize;
            // 槽是 void*，直接写入函数地址
            unsafe {
                *(slot_addr as *mut usize) = faddr;
            }
        }
    }

    let code = jit.module.get_finalized_function(main_fid);
    let entry: extern "C" fn() = unsafe { std::mem::transmute(code) };
    entry();
    let _ = std::io::Write::flush(&mut std::io::stdout());
    // TCC 会话必须活到最后：JIT 代码里存着指向它编译出的机器码的地址
    drop(tcc_sess);
    Ok(())
}

/// 若程序里有内联 C 块，用 TCC **在内存中**编译它并返回会话（不落盘）。
///
/// 编译内容 = 自动生成的桥接头（C 调用 GTLang）+ 用户 C 块。
/// 返回的会话必须活到程序结束：JIT 代码里直接指向 TCC 编译出的机器码。
fn compile_cblock(prog: &Program) -> Result<Option<crate::tcc::TccSession>, String> {
    if prog.cblock.trim().is_empty() {
        return Ok(None);
    }
    let dir = crate::driver::find_tcc_dir().ok_or_else(|| {
        "程序包含内联 C 块，但未找到 TCC 工具链（需要 toolchain/tcc/libtcc.dll）。\n  \
         可用环境变量 GTC_TCC 指定 TCC 目录。"
            .to_string()
    })?;
    // 桥接头必须放在最前：C 块里可能调用 GTLang 函数。
    // 分两片编译，使 C 块的报错行号从 1 开始、直接对应 `C { ... }` 内的行。
    let fns = crate::cblock::gt_functions(&prog.items);
    let (bridge, _slots) = crate::cblock::bridge_header(&fns);

    let lib = std::rc::Rc::new(crate::tcc::LibTcc::load(&dir)?);
    let sess = lib.compile_parts(&[&bridge, &prog.cblock], &dir)?;
    // C 侧 printf 走 CRT 缓冲，需关掉才能与 GTLang 的输出保持先后顺序
    if let Some(addr) = sess.symbol("gt_flush_iob") {
        let f: extern "C" fn() = unsafe { std::mem::transmute(addr) };
        f();
    }
    Ok(Some(sess))
}

impl<'a> Jit<'a> {
    /// `cfuncs` 是内联 C 块编译后得到的 `(符号名, 地址)`，
    /// 在构建 JITBuilder 时注册，使 GTLang 能直接调用 C 函数。
    pub(crate) fn new(an: &'a Analysis, cfuncs: &[(String, usize)]) -> Result<Jit<'a>, String> {
        let mut flags = settings::builder();
        flags
            .set("use_colocated_libcalls", "false")
            .map_err(|e| e.to_string())?;
        flags.set("is_pic", "false").map_err(|e| e.to_string())?;
        // 打开优化：解释器性能直接取决于此
        flags
            .set("opt_level", "speed")
            .map_err(|e| e.to_string())?;
        // 性能调优：启用别名分析、关闭 Spectre 缓解（非沙箱），Release 下关闭 verifier
        let _ = flags.set("enable_alias_analysis", "true");
        let _ = flags.set("enable_heap_access_spectre_mitigation", "false");
        let _ = flags.set("enable_table_access_spectre_mitigation", "false");
        #[cfg(not(debug_assertions))]
        { let _ = flags.set("enable_verifier", "false"); }
        let isa = cranelift_native::builder()
            .map_err(|e| format!("无法探测本机 ISA：{}", e))?
            .finish(settings::Flags::new(flags))
            .map_err(|e| e.to_string())?;

        let mut jb = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        jb.symbol("rt_write", rt_write as *const u8);
        jb.symbol("rt_read_line", rt_read_line as *const u8);
        jb.symbol("rt_str_concat", rt_str_concat as *const u8);
        jb.symbol("rt_str_char_at", rt_str_char_at as *const u8);
        jb.symbol("rt_str_char_len", rt_str_char_len as *const u8);
        jb.symbol("rt_set_at", rt_set_at as *const u8);
        jb.symbol("rt_map_key_at", rt_map_key_at as *const u8);
        jb.symbol("rt_read_int", rt_read_int as *const u8);
        jb.symbol("rt_write_cstr", rt_write_cstr as *const u8);
        jb.symbol("rt_write_i64", rt_write_i64 as *const u8);
        jb.symbol("rt_write_f64", rt_write_f64 as *const u8);
        jb.symbol("rt_write_bool", rt_write_bool as *const u8);
        jb.symbol("rt_str_eq", rt_str_eq as *const u8);
        jb.symbol("rt_str_len", rt_str_len as *const u8);
        jb.symbol("rt_str_nonempty", rt_str_nonempty as *const u8);
        jb.symbol("rt_to_i64", rt_to_i64 as *const u8);
        jb.symbol("rt_to_f64", rt_to_f64 as *const u8);
        jb.symbol("rt_bounds", rt_bounds as *const u8);
        jb.symbol("rt_div_zero", rt_div_zero as *const u8);
        jb.symbol("rt_overflow", rt_overflow as *const u8);
        jb.symbol("rt_sb_new", rt_sb_new as *const u8);
        jb.symbol("rt_sb_push_str", rt_sb_push_str as *const u8);
        jb.symbol("rt_sb_push_i64", rt_sb_push_i64 as *const u8);
        jb.symbol("rt_sb_push_f64", rt_sb_push_f64 as *const u8);
        jb.symbol("rt_sb_push_bool", rt_sb_push_bool as *const u8);
        jb.symbol("rt_sb_finish", rt_sb_finish as *const u8);
        // 容器运行时
        jb.symbol("rt_list_new", rt_list_new as *const u8);
        jb.symbol("rt_range", rt_range as *const u8);
        jb.symbol("rt_assert", rt_assert as *const u8);
        jb.symbol("rt_thread_spawn", rt_thread_spawn as *const u8);
        jb.symbol("rt_sleep", rt_sleep as *const u8);
        jb.symbol("rt_rt_init", rt_rt_init as *const u8);
        // boot 模拟后端（宿主机）
        jb.symbol("boot_serial_init", boot_serial_init as *const u8);
        jb.symbol("boot_serial_putc", boot_serial_putc as *const u8);
        jb.symbol("boot_serial_puts", boot_serial_puts as *const u8);
        jb.symbol("boot_serial_getc", boot_serial_getc as *const u8);
        jb.symbol("boot_serial_poll", boot_serial_poll as *const u8);
        jb.symbol("boot_clear", boot_clear as *const u8);
        jb.symbol("boot_putc_at", boot_putc_at as *const u8);
        jb.symbol("boot_puts", boot_puts as *const u8);
        jb.symbol("boot_vga_clear", boot_vga_clear as *const u8);
        jb.symbol("boot_vga_set_color", boot_vga_set_color as *const u8);
        jb.symbol("boot_vga_putc", boot_vga_putc as *const u8);
        jb.symbol("boot_vga_puts", boot_vga_puts as *const u8);
        jb.symbol("boot_getkey", boot_getkey as *const u8);
        jb.symbol("boot_keyboard_handler", boot_keyboard_handler as *const u8);
        jb.symbol("boot_keyboard_modifiers", boot_keyboard_modifiers as *const u8);
        jb.symbol("boot_mem_alloc", boot_mem_alloc as *const u8);
        jb.symbol("boot_mem_free", boot_mem_free as *const u8);
        jb.symbol("boot_mem_size", boot_mem_size as *const u8);
        jb.symbol("boot_mem_free_bytes", boot_mem_free_bytes as *const u8);
        jb.symbol("boot_paging_init", boot_paging_init as *const u8);
        jb.symbol("boot_time_ms", boot_time_ms as *const u8);
        jb.symbol("boot_sleep_ms", boot_sleep_ms as *const u8);
        jb.symbol("boot_rtc_read", boot_rtc_read as *const u8);
        jb.symbol("boot_hlt", boot_hlt as *const u8);
        jb.symbol("boot_exit", boot_exit as *const u8);
        jb.symbol("boot_reboot", boot_reboot as *const u8);
        jb.symbol("boot_shutdown", boot_shutdown as *const u8);
        jb.symbol("boot_cpuid", boot_cpuid as *const u8);
        jb.symbol("boot_cpu_vendor", boot_cpu_vendor as *const u8);
        jb.symbol("boot_disk_read", boot_disk_read as *const u8);
        jb.symbol("boot_disk_write", boot_disk_write as *const u8);
        jb.symbol("boot_disk_partitions", boot_disk_partitions as *const u8);
        jb.symbol("boot_inb", boot_inb as *const u8);
        jb.symbol("boot_outb", boot_outb as *const u8);
        jb.symbol("boot_inw", boot_inw as *const u8);
        jb.symbol("boot_outw", boot_outw as *const u8);
        jb.symbol("boot_idt_init", boot_idt_init as *const u8);
        jb.symbol("boot_irq_enable", boot_irq_enable as *const u8);
        jb.symbol("boot_irq_disable", boot_irq_disable as *const u8);
        jb.symbol("boot_pic_init", boot_pic_init as *const u8);
        jb.symbol("boot_irq_register", boot_irq_register as *const u8);
        jb.symbol("boot_task_create", boot_task_create as *const u8);
        jb.symbol("boot_task_yield", boot_task_yield as *const u8);
        jb.symbol("boot_task_start", boot_task_start as *const u8);
        jb.symbol("boot_task_exit", boot_task_exit as *const u8);
        jb.symbol("boot_version", boot_version as *const u8);
        jb.symbol("boot_arch", boot_arch as *const u8);
        jb.symbol("boot_fs_ram_create", boot_fs_ram_create as *const u8);
        jb.symbol("boot_fs_ram_write", boot_fs_ram_write as *const u8);
        jb.symbol("boot_fs_ram_read", boot_fs_ram_read as *const u8);
        jb.symbol("boot_fs_ram_size", boot_fs_ram_size as *const u8);
        jb.symbol("boot_fs_ram_delete", boot_fs_ram_delete as *const u8);
        jb.symbol("boot_fs_ram_count", boot_fs_ram_count as *const u8);
        jb.symbol("boot_fs_ram_list", boot_fs_ram_list as *const u8);
        jb.symbol("rt_chan_new", rt_chan_new as *const u8);
        jb.symbol("rt_chan_send", rt_chan_send as *const u8);
        jb.symbol("rt_chan_recv", rt_chan_recv as *const u8);
        jb.symbol("rt_list_push", rt_list_push as *const u8);
        jb.symbol("rt_list_pop", rt_list_pop as *const u8);
        jb.symbol("rt_list_at", rt_list_at as *const u8);
        jb.symbol("rt_list_slice", rt_list_slice as *const u8);
        jb.symbol("rt_list_set", rt_list_set as *const u8);
        jb.symbol("rt_list_len", rt_list_len as *const u8);
        jb.symbol("rt_list_has", rt_list_has as *const u8);
        jb.symbol("rt_list_remove", rt_list_remove as *const u8);
        jb.symbol("rt_set_new", rt_set_new as *const u8);
        jb.symbol("rt_set_insert", rt_set_insert as *const u8);
        jb.symbol("rt_set_has", rt_set_has as *const u8);
        jb.symbol("rt_set_remove", rt_set_remove as *const u8);
        jb.symbol("rt_set_len", rt_set_len as *const u8);
        jb.symbol("rt_map_new", rt_map_new as *const u8);
        jb.symbol("rt_map_insert", rt_map_insert as *const u8);
        jb.symbol("rt_map_get", rt_map_get as *const u8);
        jb.symbol("rt_map_has", rt_map_has as *const u8);
        jb.symbol("rt_map_remove", rt_map_remove as *const u8);
        jb.symbol("rt_map_len", rt_map_len as *const u8);
        jb.symbol("rt_map_keys", rt_map_keys as *const u8);
        jb.symbol("rt_map_values", rt_map_values as *const u8);
        // 字符串内置
        jb.symbol("rt_str_substr", rt_str_substr as *const u8);
        jb.symbol("rt_str_find", rt_str_find as *const u8);
        jb.symbol("rt_str_upper", rt_str_upper as *const u8);
        jb.symbol("rt_str_lower", rt_str_lower as *const u8);
        jb.symbol("rt_str_trim", rt_str_trim as *const u8);
        jb.symbol("rt_str_repeat", rt_str_repeat as *const u8);
        jb.symbol("rt_pad_left", rt_pad_left as *const u8);
        jb.symbol("rt_pad_right", rt_pad_right as *const u8);
        jb.symbol("rt_fmt_int", rt_fmt_int as *const u8);
        jb.symbol("rt_str_replace", rt_str_replace as *const u8);
        jb.symbol("rt_str_split", rt_str_split as *const u8);
        jb.symbol("rt_str_join", rt_str_join as *const u8);
        // 数值内置
        jb.symbol("rt_abs_i", rt_abs_i as *const u8);
        jb.symbol("rt_abs_f", rt_abs_f as *const u8);
        jb.symbol("rt_min_i", rt_min_i as *const u8);
        jb.symbol("rt_max_i", rt_max_i as *const u8);
        jb.symbol("rt_min_f", rt_min_f as *const u8);
        jb.symbol("rt_max_f", rt_max_f as *const u8);
        jb.symbol("rt_sum_i", rt_sum_i as *const u8);
        jb.symbol("rt_sum_f", rt_sum_f as *const u8);
        // 裸内存
        jb.symbol("rt_mem_alloc", rt_mem_alloc as *const u8);
        jb.symbol("rt_result_new", rt_result_new as *const u8);
        jb.symbol("rt_result_tag", rt_result_tag as *const u8);
        jb.symbol("rt_result_val", rt_result_val as *const u8);
        jb.symbol("rt_mem_free", rt_mem_free as *const u8);
        jb.symbol("rt_mem_store_i64", rt_mem_store_i64 as *const u8);
        jb.symbol("rt_mem_load_i64", rt_mem_load_i64 as *const u8);
        jb.symbol("rt_mem_store_u8", rt_mem_store_u8 as *const u8);
        jb.symbol("rt_mem_load_u8", rt_mem_load_u8 as *const u8);
        jb.symbol("rt_mem_copy", rt_mem_copy as *const u8);
        jb.symbol("rt_mem_set", rt_mem_set as *const u8);
        // 标准库 libGT.dll：加载后取 py_* 符号地址注册
        for (name, addr) in load_gtlib() {
            jb.symbol(name, addr as *const u8);
        }
        // 内联 C 块的函数：TCC 编译出的机器码地址
        for (name, addr) in cfuncs {
            jb.symbol(name, *addr as *const u8);
        }

        Ok(Jit {
            module: JITModule::new(jb),
            fns: HashMap::new(),
            strings: HashMap::new(),
            data_ids: Vec::new(),
            consts: &an.consts,
            rt: HashMap::new(),
            structs: HashMap::new(),
            enum_variants: HashMap::new(),
            traits: an.traits.clone(),
            trait_impls: an.trait_impls.clone(),
            range: None,
        })
    }

    /// 声明运行时符号与全部 GTLang 函数
    pub(crate) fn declare_all(&mut self, prog: &Program) -> Result<(), String> {
        let decl = |m: &mut JITModule, name: &'static str, params: &[Type], ret: Option<Type>| {
            let mut sig = m.make_signature();
            for p in params {
                sig.params.push(AbiParam::new(*p));
            }
            if let Some(r) = ret {
                sig.returns.push(AbiParam::new(r));
            }
            let fid = m
                .declare_function(name, Linkage::Import, &sig)
                .map_err(|e| format!("声明运行时符号 {} 失败：{}", name, e))?;
            Ok::<FuncId, String>(fid)
        };

        self.rt.insert("put_str", decl(&mut self.module, "rt_write_cstr", &[types::I64], None)?);
        self.rt.insert(
            "put_bytes",
            decl(&mut self.module, "rt_write", &[types::I64, types::I64], None)?,
        );
        self.rt.insert(
            "put_i64",
            decl(&mut self.module, "rt_write_i64", &[types::I64], None)?,
        );
        self.rt.insert(
            "put_f64",
            decl(&mut self.module, "rt_write_f64", &[types::F64], None)?,
        );
        self.rt.insert(
            "put_bool",
            decl(&mut self.module, "rt_write_bool", &[types::I64], None)?,
        );
        self.rt.insert(
            "str_eq",
            decl(&mut self.module, "rt_str_eq", &[types::I64, types::I64], Some(types::I64))?,
        );
        self.rt.insert(
            "str_len",
            decl(&mut self.module, "rt_str_len", &[types::I64], Some(types::I64))?,
        );
        self.rt.insert(
            "str_nonempty",
            decl(&mut self.module, "rt_str_nonempty", &[types::I64], Some(types::I64))?,
        );
        self.rt.insert(
            "to_i64",
            decl(&mut self.module, "rt_to_i64", &[types::I64], Some(types::I64))?,
        );
        self.rt.insert(
            "to_f64",
            decl(&mut self.module, "rt_to_f64", &[types::I64], Some(types::F64))?,
        );
        self.rt.insert(
            "bounds",
            decl(
                &mut self.module,
                "rt_bounds",
                &[types::I64, types::I64, types::I64],
                None,
            )?,
        );
        self.rt.insert(
            "div_zero",
            decl(&mut self.module, "rt_div_zero", &[types::I64], None)?,
        );
        self.rt.insert(
            "overflow",
            decl(&mut self.module, "rt_overflow", &[types::I64], None)?,
        );
        self.rt.insert(
            "sb_new",
            decl(&mut self.module, "rt_sb_new", &[], Some(types::I64))?,
        );
        self.rt.insert(
            "sb_push_str",
            decl(&mut self.module, "rt_sb_push_str", &[types::I64, types::I64], None)?,
        );
        self.rt.insert(
            "sb_push_i64",
            decl(&mut self.module, "rt_sb_push_i64", &[types::I64, types::I64], None)?,
        );
        self.rt.insert(
            "sb_push_f64",
            decl(&mut self.module, "rt_sb_push_f64", &[types::I64, types::F64], None)?,
        );
        self.rt.insert(
            "sb_push_bool",
            decl(&mut self.module, "rt_sb_push_bool", &[types::I64, types::I64], None)?,
        );
        self.rt.insert(
            "sb_finish",
            decl(&mut self.module, "rt_sb_finish", &[types::I64], Some(types::I64))?,
        );
        // 容器运行时（参数/返回一律 I64，指针即地址）
        let i64v = types::I64;
        for (key, sym, params, ret) in [
            ("list_new", "rt_list_new", vec![i64v], Some(i64v)),
            ("range", "rt_range", vec![i64v, i64v], Some(i64v)),
            ("assert", "rt_assert", vec![i64v, i64v, i64v], None),
            ("thread_spawn", "rt_thread_spawn", vec![i64v, i64v, i64v], None),
            ("sleep", "rt_sleep", vec![i64v], None),
            ("read_line", "rt_read_line", vec![], Some(i64v)),
            ("str_concat", "rt_str_concat", vec![i64v, i64v], Some(i64v)),
            ("str_char_at", "rt_str_char_at", vec![i64v, i64v], Some(i64v)),
            ("str_char_len", "rt_str_char_len", vec![i64v], Some(i64v)),
            ("set_at", "rt_set_at", vec![i64v, i64v], Some(i64v)),
            ("map_key_at", "rt_map_key_at", vec![i64v, i64v], Some(i64v)),
            ("read_int", "rt_read_int", vec![], Some(i64v)),
            ("chan_new", "rt_chan_new", vec![], Some(i64v)),
            ("chan_send", "rt_chan_send", vec![i64v, i64v], None),
            ("chan_recv", "rt_chan_recv", vec![i64v], Some(i64v)),
            ("list_push", "rt_list_push", vec![i64v, i64v], None),
            ("list_pop", "rt_list_pop", vec![i64v], Some(i64v)),
            ("list_at", "rt_list_at", vec![i64v, i64v], Some(i64v)),
            ("list_slice", "rt_list_slice", vec![i64v, i64v, i64v], Some(i64v)),
            ("list_set", "rt_list_set", vec![i64v, i64v, i64v], None),
            ("list_len", "rt_list_len", vec![i64v], Some(i64v)),
            ("list_has", "rt_list_has", vec![i64v, i64v], Some(i64v)),
            ("list_remove", "rt_list_remove", vec![i64v, i64v], None),
            ("set_new", "rt_set_new", vec![i64v], Some(i64v)),
            ("set_insert", "rt_set_insert", vec![i64v, i64v], None),
            ("set_has", "rt_set_has", vec![i64v, i64v], Some(i64v)),
            ("set_remove", "rt_set_remove", vec![i64v, i64v], None),
            ("set_len", "rt_set_len", vec![i64v], Some(i64v)),
            ("map_new", "rt_map_new", vec![i64v], Some(i64v)),
            ("map_insert", "rt_map_insert", vec![i64v, i64v, i64v], None),
            ("map_get", "rt_map_get", vec![i64v, i64v], Some(i64v)),
            ("map_has", "rt_map_has", vec![i64v, i64v], Some(i64v)),
            ("map_remove", "rt_map_remove", vec![i64v, i64v], None),
            ("map_len", "rt_map_len", vec![i64v], Some(i64v)),
            ("map_keys", "rt_map_keys", vec![i64v], Some(i64v)),
            ("map_values", "rt_map_values", vec![i64v], Some(i64v)),
            ("str_substr", "rt_str_substr", vec![i64v, i64v, i64v], Some(i64v)),
            ("str_find", "rt_str_find", vec![i64v, i64v], Some(i64v)),
            ("str_upper", "rt_str_upper", vec![i64v], Some(i64v)),
            ("str_lower", "rt_str_lower", vec![i64v], Some(i64v)),
            ("str_trim", "rt_str_trim", vec![i64v], Some(i64v)),
            ("str_repeat", "rt_str_repeat", vec![i64v, i64v], Some(i64v)),
            ("pad_left", "rt_pad_left", vec![i64v, i64v, i64v], Some(i64v)),
            ("pad_right", "rt_pad_right", vec![i64v, i64v, i64v], Some(i64v)),
            ("fmt_int", "rt_fmt_int", vec![i64v, i64v], Some(i64v)),
            ("str_replace", "rt_str_replace", vec![i64v, i64v, i64v], Some(i64v)),
            ("str_split", "rt_str_split", vec![i64v, i64v], Some(i64v)),
            ("str_join", "rt_str_join", vec![i64v, i64v], Some(i64v)),
            ("abs_i", "rt_abs_i", vec![i64v], Some(i64v)),
            ("abs_f", "rt_abs_f", vec![types::F64], Some(types::F64)),
            ("min_i", "rt_min_i", vec![i64v, i64v], Some(i64v)),
            ("max_i", "rt_max_i", vec![i64v, i64v], Some(i64v)),
            ("min_f", "rt_min_f", vec![types::F64, types::F64], Some(types::F64)),
            ("max_f", "rt_max_f", vec![types::F64, types::F64], Some(types::F64)),
            ("sum_i", "rt_sum_i", vec![i64v], Some(i64v)),
            ("sum_f", "rt_sum_f", vec![i64v], Some(types::F64)),
            ("result_new", "rt_result_new", vec![i64v, i64v], Some(i64v)),
            ("result_tag", "rt_result_tag", vec![i64v], Some(i64v)),
            ("result_val", "rt_result_val", vec![i64v], Some(i64v)),
            ("mem_alloc", "rt_mem_alloc", vec![i64v], Some(i64v)),
            ("mem_free", "rt_mem_free", vec![i64v], None),
            ("mem_store_i64", "rt_mem_store_i64", vec![i64v, i64v, i64v], None),
            ("mem_load_i64", "rt_mem_load_i64", vec![i64v, i64v], Some(i64v)),
            ("mem_store_u8", "rt_mem_store_u8", vec![i64v, i64v, i64v], None),
            ("mem_load_u8", "rt_mem_load_u8", vec![i64v, i64v], Some(i64v)),
            ("mem_copy", "rt_mem_copy", vec![i64v, i64v, i64v], None),
            ("mem_set", "rt_mem_set", vec![i64v, i64v, i64v], None),
        ] {
            let fid = decl(&mut self.module, sym, &params, ret)?;
            self.rt.insert(key, fid);
        }

        // 标准库 libGT.dll：为每个 py_* 符号按签名声明（返回类型用 cl_ty）
        for name in GTLIB_NAMES {
            if let Some(sf) = crate::types::gtlib_fn(name) {
                let params: Vec<Type> = sf.params.iter().map(cl_ty).collect();
                let ret = if sf.ret == Ty::Void { None } else { Some(cl_ty(&sf.ret)) };
                if let Ok(fid) = decl(&mut self.module, sf.symbol, &params, ret) {
                    self.rt.insert(sf.symbol, fid);
                }
            }
        }

        for item in &prog.items {
            if let Item::Fn(f) = item {
                let params: Vec<Ty> = f
                    .params
                    .iter()
                    .map(|p| p.ty.clone().unwrap_or(Ty::I64))
                    .collect();
                let ret = f.ret_ty.clone();
                let mut sig = self.module.make_signature();
                for p in &params {
                    sig.params.push(AbiParam::new(cl_ty(p)));
                }
                if ret != Ty::Void {
                    sig.returns.push(AbiParam::new(cl_ty(&ret)));
                }
                let fid = self
                    .module
                    .declare_function(&cname(&f.name), Linkage::Export, &sig)
                    .map_err(|e| format!("声明函数 {} 失败：{}", f.name, e))?;
                self.fns.insert(f.name.clone(), FnInfo { fid, params, ret });
            }
        }

        // 内联 C 块里的函数：声明为 Import，实际机器码来自 TCC
        for cf in &prog.cfuncs {
            let mut sig = self.module.make_signature();
            for p in &cf.params {
                sig.params.push(AbiParam::new(cl_ty(p)));
            }
            if cf.ret != Ty::Void {
                sig.returns.push(AbiParam::new(cl_ty(&cf.ret)));
            }
            let fid = self
                .module
                .declare_function(&cf.name, Linkage::Import, &sig)
                .map_err(|e| format!("声明内联 C 函数 {} 失败：{}", cf.name, e))?;
            self.fns.insert(
                cf.name.clone(),
                FnInfo { fid, params: cf.params.clone(), ret: cf.ret.clone() },
            );
        }
        Ok(())
    }

    /// 预置所有字符串字面量为数据段
    /// 把所有字符串字面量登记到池（intern）。
    pub(crate) fn intern_all(&mut self, prog: &Program) -> Result<(), String> {
        let mut lits: Vec<Vec<u8>> = Vec::new();
        for item in &prog.items {
            match item {
                Item::Const { value, .. } => collect_strs(value, &mut lits),
                Item::Fn(f) => collect_strs_block(&f.body, &mut lits),
                Item::Struct(_) | Item::Enum(_) | Item::Macro { .. } | Item::Impl { .. } | Item::ExternC(_) | Item::Trait(_)
                | Item::TraitImpl { .. } => {}
            }
        }
        for bytes in lits {
            self.intern(&bytes)?;
        }
        // put 的换行符与布尔字面量也预置好
        self.intern(b"\n")?;
        Ok(())
    }

    /// 把一段 UTF-8 注册为 NUL 结尾的数据，返回 DataId
    pub(crate) fn intern(&mut self, bytes: &[u8]) -> Result<u64, String> {
        if let Some(id) = self.strings.get(bytes) {
            return Ok(*id);
        }
        let name = format!("gt_str_{}", self.strings.len());
        let did = self
            .module
            .declare_data(&name, Linkage::Local, false, false)
            .map_err(|e| e.to_string())?;
        let mut buf = bytes.to_vec();
        buf.push(0);
        let mut desc = DataDescription::new();
        desc.define(buf.into_boxed_slice());
        self.module.define_data(did, &desc).map_err(|e| e.to_string())?;
        // 用 as_u32 作为稳定键
        let key = did.as_u32() as u64;
        self.strings.insert(bytes.to_vec(), key);
        self.data_ids.push(key);
        Ok(key)
    }

    pub(crate) fn data_id_of(&self, bytes: &[u8]) -> Option<u64> {
        self.strings.get(bytes).copied()
    }
    /// 生成一个 GTLang 函数的机器码
    pub(crate) fn gen_function(&mut self, f: &FnDef, fbctx: &mut FunctionBuilderContext) -> Result<(), String> {
        let info_params = self.fns.get(&f.name).map(|i| i.params.clone()).unwrap_or_default();
        let info_ret = self.fns.get(&f.name).map(|i| i.ret.clone()).unwrap_or(Ty::Void);
        let fid = match self.fns.get(&f.name) { Some(i) => i.fid, None => return Ok(()) };
        let mut sig = self.module.make_signature();
        for p in &info_params { sig.params.push(AbiParam::new(cl_ty(p))); }
        if info_ret != Ty::Void { sig.returns.push(AbiParam::new(cl_ty(&info_ret))); }
        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;
        {
            let mut b = FunctionBuilder::new(&mut ctx.func, fbctx);
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let mut st = FnState::new(info_ret.clone(), self.range.clone());
            let mut idx = 0usize;
            for p in &f.params {
                let pty = p.ty.clone().unwrap_or(Ty::I64);
                let var = st.new_var(&mut b, &pty);
                let pv = b.block_params(entry)[idx];
                b.def_var(var, pv);
                st.bind(&p.name, var, pty);
                idx += 1;
            }
            let tail = if info_ret == Ty::Void {
                st.gen_block(self, &mut b, &f.body)?; None
            } else {
                let want = info_ret.clone();
                st.gen_block_value(self, &mut b, &f.body, &want)?
            };
            if !st.terminated {
                match tail {
                    Some(v) => { b.ins().return_(&[v]); }
                    None => {
                        if info_ret == Ty::Void { b.ins().return_(&[]); }
                        else {
                            let zero = if info_ret.is_float() { b.ins().f64const(0.0) } else { b.ins().iconst(types::I64, 0) };
                            b.ins().return_(&[zero]);
                        }
                    }
                }
            }
            b.seal_all_blocks();
            b.finalize();
        }
        self.module.define_function(fid, &mut ctx).map_err(|e| format!("JIT 编译函数 {} 失败：{}", f.name, e))?;
        self.module.clear_context(&mut ctx);
        Ok(())
    }
}

pub(crate) struct VarBind { var: Variable, ty: Ty }

pub(crate) struct FnState {
    pub(crate) var_count: u32,
    pub(crate) scopes: Vec<Vec<(String, VarBind)>>,
    pub(crate) array_slots: HashMap<usize, StackSlot>,
    pub(crate) loops: Vec<(ClBlock, ClBlock)>,
    pub(crate) cur_ret: Ty,
    pub(crate) terminated: bool,
    /// 已知上界的循环变量：名字 → 排他上界（`for i in 0..N` 中的 N）。
    /// 用于省略数组下标的冗余边界检查（当数组长度 ≥ N 时 i 必在范围内）。
    pub(crate) bounded: HashMap<String, i64>,
    /// 活跃的 `try` 故障处理栈：故障时跳转的块 + 故障码变量。
    /// （阶段 4C：仅在 `try` 块内，运行时故障可被捕获）
    pub(crate) fault_stack: Vec<(ClBlock, Variable)>,
    /// 整数范围分析结果（省略可证明安全的溢出检查；与 LLVM 后端一致）
    pub(crate) range: Option<crate::range::Analysis>,
}

impl FnState {
    pub(crate) fn new(cur_ret: Ty, range: Option<crate::range::Analysis>) -> FnState {
        FnState { var_count: 0, scopes: vec![Vec::new()], array_slots: HashMap::new(), loops: Vec::new(), cur_ret, terminated: false, bounded: HashMap::new(), fault_stack: Vec::new(), range }
    }
    pub(crate) fn new_block(&mut self, b: &mut FunctionBuilder) -> ClBlock { b.create_block() }
    pub(crate) fn new_var(&mut self, b: &mut FunctionBuilder, ty: &Ty) -> Variable {
        let v = Variable::from_u32(self.var_count);
        self.var_count += 1;
        b.declare_var(v, cl_ty(ty));
        v
    }
    pub(crate) fn push_scope(&mut self) { self.scopes.push(Vec::new()); }
    pub(crate) fn pop_scope(&mut self) { self.scopes.pop(); }
    pub(crate) fn bind(&mut self, name: &str, var: Variable, ty: Ty) {
        self.scopes.last_mut().unwrap().push((name.to_string(), VarBind { var, ty }));
    }
    /// 重新绑定已存在的变量（改类型时用）：更新最近作用域里的那条绑定。
    pub(crate) fn rebind(&mut self, name: &str, var: Variable, ty: Ty) {
        for sc in self.scopes.iter_mut().rev() {
            for (n, vb) in sc.iter_mut().rev() {
                if n == name {
                    vb.var = var;
                    vb.ty = ty;
                    return;
                }
            }
        }
        // 未找到（不应发生）：追加到当前作用域
        self.bind(name, var, ty);
    }
    pub(crate) fn lookup(&self, name: &str) -> Option<(Variable, Ty)> {
        for sc in self.scopes.iter().rev() {
            for (n, vb) in sc.iter().rev() { if n == name { return Some((vb.var, vb.ty.clone())); } }
        }
        None
    }
    pub(crate) fn rt_ref(&self, jit: &mut Jit, b: &mut FunctionBuilder, key: &str) -> Result<cranelift_codegen::ir::FuncRef, String> {
        let fid = *jit.rt.get(key).ok_or_else(|| format!("内部错误：缺少运行时符号 {}", key))?;
        Ok(jit.module.declare_func_in_func(fid, b.func))
    }
    pub(crate) fn gen_block(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, body: &Block) -> Result<(), String> {
        for s in body { if self.terminated { break; } self.gen_stmt(jit, b, s)?; }
        Ok(())
    }
    pub(crate) fn gen_block_value(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, body: &Block, want: &Ty) -> Result<Option<Value>, String> {
        if body.is_empty() { return Ok(None); }
        let n = body.len();
        self.push_scope();
        for s in &body[..n - 1] { if self.terminated { break; } self.gen_stmt(jit, b, s)?; }
        let res = if self.terminated { Ok(None) } else {
            match &body[n - 1] {
                Stmt::Expr(e) => self.gen_expr(jit, b, e).map(|v| Some(self.convert(b, &v, want))),
                Stmt::If { cond, then, els, .. } => self.gen_if_value(jit, b, cond, then, els.as_ref(), want),
                Stmt::Block(inner) => self.gen_block_value(jit, b, inner, want),
                other => self.gen_stmt(jit, b, other).map(|_| None),
            }
        };
        self.pop_scope();
        res
    }
}

mod stmt;
mod expr;
mod call;
#[allow(unused_imports)]
pub(crate) use stmt::*;
#[allow(unused_imports)]
pub(crate) use expr::*;
#[allow(unused_imports)]
pub(crate) use call::*;
