//! `boot.screen` 子模块：screen 组函数登记（实现见 `os/boot/rt_bare.c` / `boot16.asm`）。
//!
//! 命名空间调用：`import boot.screen` 后 `boot.screen.<fn>(...)`；也兼容扁平名。

/// 本组函数名（扁平形式）。
pub const FUNCS: &[&str] = &[
    "boot_clear",
    "boot_putc_at",
    "boot_puts",
];

