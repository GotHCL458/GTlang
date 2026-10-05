//! `boot.serial` 子模块：serial 组函数登记（实现见 `os/boot/rt_bare.c` / `boot16.asm`）。
//!
//! 命名空间调用：`import boot.serial` 后 `boot.serial.<fn>(...)`；也兼容扁平名。

/// 本组函数名（扁平形式）。
pub const FUNCS: &[&str] = &[
    "boot_serial_init",
    "boot_serial_putc",
    "boot_serial_puts",
    "boot_serial_getc",
];

