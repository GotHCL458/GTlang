//! `boot` 公共类型（架构枚举、函数元信息）与 `boot.boot.load` 入口。

/// 目标架构（由引导库在编译期决定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootArch {
    /// 16 位实模式
    X86_16,
    /// 32 位保护模式
    X86_32,
    /// 64 位长模式
    X86_64,
}

impl BootArch {
    pub fn as_i64(self) -> i64 {
        match self { BootArch::X86_16 => 16, BootArch::X86_32 => 32, BootArch::X86_64 => 64 }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "x86_16" | "16" => Some(BootArch::X86_16),
            "x86_32" | "32" => Some(BootArch::X86_32),
            "x86_64" | "64" => Some(BootArch::X86_64),
            _ => None,
        }
    }
}

/// 一个 boot 函数：GTLang 侧名、底层符号名、所属分组。
pub struct BootFn {
    /// GTLang 里使用的名字（`import boot` 后直接调用）
    pub name: &'static str,
    /// 链接期的 C/汇编符号
    pub symbol: &'static str,
    /// 分组（serial/screen/…）
    pub group: &'static str,
}

// ===== boot.boot.load（引导/内核加载入口）=====
//
// A 方案（编译期登记）：`boot.boot.load("模块", "入口")` 的两个参数为字符串字面量，
// 编译器解析成"模块.入口"符号并登记为引导入口；运行时直接 `call` 该符号。
// 生成的登记表由 codegen 输出到 `__gt_boot_entries`（见 codegen 的 boot_load 分支）。

/// 编译期解析出的一个引导入口（模块名、入口名、扁平符号）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootEntry {
    pub module: String,
    pub entry: String,
    pub symbol: String,
}

/// 把 ("内核主程序", "主入口") 解析为扁平符号。
///
/// 符号必须能出现在 LLVM IR 裸标识符里（ASCII 安全），因此做转义：
/// 非 ASCII 字节写成 `_xx`（两位十六进制）。
/// 引导入口符号：`gt_<mangle(入口)>`（顶层函数名，与 codegen 的函数符号规则一致）。
/// `module` 仅用于登记表记录。
pub fn entry_symbol(_module: &str, entry: &str) -> String {
    format!("gt_{}", mangle(entry))
}

/// 与 `codegen::mangle` 相同的转义（此处复制一份，避免 gtlib 依赖 codegen）。
fn mangle(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'_' { out.push(b as char); }
        else { out.push_str(&format!("_x{:02X}", b)); }
    }
    out
}
