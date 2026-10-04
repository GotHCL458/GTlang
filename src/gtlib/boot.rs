//! `boot` 标准库模块：裸机引导（仅 `--bare` 目标链接）。
//!
//! 由引导库（`os/boot/`）在运行时提供：串口、简单堆、时间、停机等。
//! 这些函数的符号（`boot_*`）只在裸机镜像中存在，宿主程序请勿调用。


/// 引导库符号（由 `os/boot/rt_bare.c` 提供）。
pub const BOOT_FUNCS: &[(&str, &str)] = &[
    ("boot_serial_init", "boot_serial_init"),
    ("boot_serial_putc", "boot_serial_putc"),
    ("boot_hlt", "boot_hlt"),
    ("boot_exit", "boot_exit"),
    ("boot_mem_alloc", "boot_mem_alloc"),
    ("boot_time_ms", "boot_time_ms"),
    ("boot_reboot", "boot_reboot"),
];

/// 该模块是否仅用于裸机目标。
pub fn is_bare_only() -> bool { true }
