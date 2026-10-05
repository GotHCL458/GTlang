//! `boot` 标准库：裸机引导库（Bare-metal Boot Library）。
//!
//! 见各子模块：serial/screen/keyboard/memory/time/system/disk/port/interrupt/fs/task/info/boot(load)。
//! 命名空间调用：`import boot.fs` 后 `boot.fs.write(...)`（也兼容扁平 `boot_fs_write(...)`）。

mod boot_mod;
pub mod disk;
pub mod fs_fat16;
pub mod fs_fat32;
pub mod fs_fat8;
pub mod fs_ram;
pub mod info;
pub mod interrupt;
pub mod keyboard;
pub mod memory;
pub mod port;
pub mod screen;
pub mod serial;
pub mod system;
pub mod task;
pub mod time;

pub use boot_mod::BootArch;
pub use boot_mod::BootEntry;
pub use boot_mod::BootFn;
pub use boot_mod::entry_symbol;

/// 按分组归集各子模块的函数表（顺序：串口/屏幕/…/fs/task/info/boot）。
pub const BOOT_FUNCS: &[BootFn] = &[
    BootFn { name: "boot_serial_init", symbol: "boot_serial_init", group: "serial" },
    BootFn { name: "boot_serial_putc", symbol: "boot_serial_putc", group: "serial" },
    BootFn { name: "boot_serial_puts", symbol: "boot_serial_puts", group: "serial" },
    BootFn { name: "boot_serial_getc", symbol: "boot_serial_getc", group: "serial" },
    BootFn { name: "boot_clear", symbol: "boot_clear", group: "screen" },
    BootFn { name: "boot_putc_at", symbol: "boot_putc_at", group: "screen" },
    BootFn { name: "boot_puts", symbol: "boot_puts", group: "screen" },
    BootFn { name: "boot_vga_clear", symbol: "boot_vga_clear", group: "screen" },
    BootFn { name: "boot_vga_set_color", symbol: "boot_vga_set_color", group: "screen" },
    BootFn { name: "boot_vga_putc", symbol: "boot_vga_putc", group: "screen" },
    BootFn { name: "boot_vga_puts", symbol: "boot_vga_puts", group: "screen" },
    BootFn { name: "boot_getkey", symbol: "boot_getkey", group: "keyboard" },
    BootFn { name: "boot_mem_alloc", symbol: "boot_mem_alloc", group: "memory" },
    BootFn { name: "boot_mem_free", symbol: "boot_mem_free", group: "memory" },
    BootFn { name: "boot_mem_size", symbol: "boot_mem_size", group: "memory" },
    BootFn { name: "boot_time_ms", symbol: "boot_time_ms", group: "time" },
    BootFn { name: "boot_sleep_ms", symbol: "boot_sleep_ms", group: "time" },
    BootFn { name: "boot_hlt", symbol: "boot_hlt", group: "system" },
    BootFn { name: "boot_exit", symbol: "boot_exit", group: "system" },
    BootFn { name: "boot_reboot", symbol: "boot_reboot", group: "system" },
    BootFn { name: "boot_shutdown", symbol: "boot_shutdown", group: "system" },
    BootFn { name: "boot_disk_read", symbol: "boot_disk_read", group: "disk" },
    BootFn { name: "boot_disk_write", symbol: "boot_disk_write", group: "disk" },
    BootFn { name: "boot_inb", symbol: "boot_inb", group: "port" },
    BootFn { name: "boot_outb", symbol: "boot_outb", group: "port" },
    BootFn { name: "boot_inw", symbol: "boot_inw", group: "port" },
    BootFn { name: "boot_outw", symbol: "boot_outw", group: "port" },
    BootFn { name: "boot_cpuid", symbol: "boot_cpuid", group: "system" },
    BootFn { name: "boot_cpu_vendor", symbol: "boot_cpu_vendor", group: "system" },
    BootFn { name: "boot_rtc_read", symbol: "boot_rtc_read", group: "time" },
    BootFn { name: "boot_disk_partitions", symbol: "boot_disk_partitions", group: "disk" },
    BootFn { name: "boot_version", symbol: "boot_version", group: "info" },
    BootFn { name: "boot_arch", symbol: "boot_arch", group: "info" },
    BootFn { name: "boot_idt_init", symbol: "boot_idt_init", group: "interrupt" },
    BootFn { name: "boot_irq_enable", symbol: "boot_irq_enable", group: "interrupt" },
    BootFn { name: "boot_irq_disable", symbol: "boot_irq_disable", group: "interrupt" },
    BootFn { name: "boot_irq_register", symbol: "boot_irq_register", group: "interrupt" },
    BootFn { name: "boot_paging_init", symbol: "boot_paging_init", group: "memory" },
    BootFn { name: "boot_mem_free_bytes", symbol: "boot_mem_free_bytes", group: "memory" },
    BootFn { name: "boot_task_exit", symbol: "boot_task_exit", group: "task" },
    BootFn { name: "boot_pic_init", symbol: "boot_pic_init", group: "interrupt" },
    BootFn { name: "boot_keyboard_handler", symbol: "boot_keyboard_handler", group: "interrupt" },
    // fs_ram（内存文件系统）
    BootFn { name: "boot_fs_ram_create", symbol: "boot_fs_ram_create", group: "fs_ram" },
    BootFn { name: "boot_fs_ram_write", symbol: "boot_fs_ram_write", group: "fs_ram" },
    BootFn { name: "boot_fs_ram_read", symbol: "boot_fs_ram_read", group: "fs_ram" },
    BootFn { name: "boot_fs_ram_size", symbol: "boot_fs_ram_size", group: "fs_ram" },
    BootFn { name: "boot_fs_ram_delete", symbol: "boot_fs_ram_delete", group: "fs_ram" },
    BootFn { name: "boot_fs_ram_count", symbol: "boot_fs_ram_count", group: "fs_ram" },
    BootFn { name: "boot_fs_ram_list", symbol: "boot_fs_ram_list", group: "fs_ram" },
    // fs_fat8（只读）
    BootFn { name: "boot_fat8_find", symbol: "boot_fat8_find", group: "fs_fat8" },
    BootFn { name: "boot_fat8_read", symbol: "boot_fat8_read", group: "fs_fat8" },
    BootFn { name: "boot_fat8_list", symbol: "boot_fat8_list", group: "fs_fat8" },
    // fs_fat16（完整读写）
    BootFn { name: "boot_fat16_find", symbol: "boot_fat16_find", group: "fs_fat16" },
    BootFn { name: "boot_fat16_read", symbol: "boot_fat16_read", group: "fs_fat16" },
    BootFn { name: "boot_fat16_write", symbol: "boot_fat16_write", group: "fs_fat16" },
    BootFn { name: "boot_fat16_delete", symbol: "boot_fat16_delete", group: "fs_fat16" },
    BootFn { name: "boot_fat16_list", symbol: "boot_fat16_list", group: "fs_fat16" },
    // fs_fat32（完整读写）
    BootFn { name: "boot_fat32_find", symbol: "boot_fat32_find", group: "fs_fat32" },
    BootFn { name: "boot_fat32_read", symbol: "boot_fat32_read", group: "fs_fat32" },
    BootFn { name: "boot_fat32_write", symbol: "boot_fat32_write", group: "fs_fat32" },
    BootFn { name: "boot_fat32_delete", symbol: "boot_fat32_delete", group: "fs_fat32" },
    BootFn { name: "boot_fat32_list", symbol: "boot_fat32_list", group: "fs_fat32" },
    BootFn { name: "boot_task_create", symbol: "boot_task_create", group: "task" },
    BootFn { name: "boot_task_yield", symbol: "boot_task_yield", group: "task" },
    BootFn { name: "boot_task_start", symbol: "boot_task_start", group: "task" },
    BootFn { name: "boot_load", symbol: "boot_load", group: "boot" },
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
