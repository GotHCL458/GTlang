//! `boot` 标准库：裸机引导库（Bare-metal Boot Library）。
//!
//! ## 定位
//!
//! `boot` 是**唯一可被导入用于引导**的库：它把「GTLang 内核」与「硬件/固件」隔离开。
//! GTLang 侧只写 `import boot` + 业务逻辑；引导/固件细节由本库的**实现**提供。
//!
//! ## 三套实现（按目标架构）
//!
//! | 架构 | 实现文件 | 说明 |
//! |---|---|---|
//! | `x86_16` | `boot16.asm`（可用 `gtc --asm16` 自组装） | 实模式 / BIOS |
//! | `x86_32` | `boot32.asm` + `rt32.c` | 32 位保护模式 |
//! | `x86_64` | `boot64.asm` + `rt64.c` | 长模式 |
//!
//! ## 函数分组
//!
//! - **serial** 串口（COM1）：`boot_serial_init/putc/puts/getc`
//! - **screen** 屏幕（BIOS 文本模式）：`boot_clear/putc_at/puts`
//! - **keyboard** 键盘：`boot_getkey`
//! - **memory** 堆：`boot_mem_alloc/free/size`
//! - **time** 时间：`boot_time_ms/sleep_ms`
//! - **system** 系统：`boot_hlt/exit/reboot/shutdown`
//! - **disk** 磁盘（LBA）：`boot_disk_read/write`
//! - **port** 端口 IO：`boot_inb/outb/inw/outw`
//! - **info** 信息：`boot_version/arch`

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

/// 完整的 boot 库函数表（GTLang 名 -> 符号）。
pub const BOOT_FUNCS: &[BootFn] = &[
    // serial：串口 COM1（0x3F8）
    BootFn { name: "boot_serial_init", symbol: "boot_serial_init", group: "serial" },
    BootFn { name: "boot_serial_putc", symbol: "boot_serial_putc", group: "serial" },
    BootFn { name: "boot_serial_puts", symbol: "boot_serial_puts", group: "serial" },
    BootFn { name: "boot_serial_getc", symbol: "boot_serial_getc", group: "serial" },
    // screen：BIOS 文本模式
    BootFn { name: "boot_clear", symbol: "boot_clear", group: "screen" },
    BootFn { name: "boot_putc_at", symbol: "boot_putc_at", group: "screen" },
    BootFn { name: "boot_puts", symbol: "boot_puts", group: "screen" },
    // keyboard
    BootFn { name: "boot_getkey", symbol: "boot_getkey", group: "keyboard" },
    // memory
    BootFn { name: "boot_mem_alloc", symbol: "boot_mem_alloc", group: "memory" },
    BootFn { name: "boot_mem_free", symbol: "boot_mem_free", group: "memory" },
    BootFn { name: "boot_mem_size", symbol: "boot_mem_size", group: "memory" },
    // time
    BootFn { name: "boot_time_ms", symbol: "boot_time_ms", group: "time" },
    BootFn { name: "boot_sleep_ms", symbol: "boot_sleep_ms", group: "time" },
    // system
    BootFn { name: "boot_hlt", symbol: "boot_hlt", group: "system" },
    BootFn { name: "boot_exit", symbol: "boot_exit", group: "system" },
    BootFn { name: "boot_reboot", symbol: "boot_reboot", group: "system" },
    BootFn { name: "boot_shutdown", symbol: "boot_shutdown", group: "system" },
    // disk
    BootFn { name: "boot_disk_read", symbol: "boot_disk_read", group: "disk" },
    BootFn { name: "boot_disk_write", symbol: "boot_disk_write", group: "disk" },
    // port
    BootFn { name: "boot_inb", symbol: "boot_inb", group: "port" },
    BootFn { name: "boot_outb", symbol: "boot_outb", group: "port" },
    BootFn { name: "boot_inw", symbol: "boot_inw", group: "port" },
    BootFn { name: "boot_outw", symbol: "boot_outw", group: "port" },
    // info
    BootFn { name: "boot_version", symbol: "boot_version", group: "info" },
    BootFn { name: "boot_arch", symbol: "boot_arch", group: "info" },
    // interrupt（IDT/PIC）
    BootFn { name: "boot_idt_init", symbol: "boot_idt_init", group: "interrupt" },
    BootFn { name: "boot_irq_enable", symbol: "boot_irq_enable", group: "interrupt" },
    BootFn { name: "boot_irq_disable", symbol: "boot_irq_disable", group: "interrupt" },
    BootFn { name: "boot_pic_init", symbol: "boot_pic_init", group: "interrupt" },
    BootFn { name: "boot_keyboard_handler", symbol: "boot_keyboard_handler", group: "interrupt" },
];

/// 该模块是否仅用于裸机目标（宿主机链接时不提供这些符号）。
pub fn is_bare_only() -> bool { true }

/// 按分组列出函数名（文档/工具用）。
pub fn funcs_of(group: &str) -> Vec<&'static str> {
    BOOT_FUNCS.iter().filter(|f| f.group == group).map(|f| f.name).collect()
}

/// 全部函数名（`import boot` 后可直接调用）。
pub fn all_funcs() -> Vec<&'static str> {
    BOOT_FUNCS.iter().map(|f| f.name).collect()
}
