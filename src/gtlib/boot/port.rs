//! `boot.port` 子模块：port 组函数登记（实现见 `os/boot/rt_bare.c` / `boot16.asm`）。
//!
//! 命名空间调用：`import boot.port` 后 `boot.port.<fn>(...)`；也兼容扁平名。

/// 本组函数名（扁平形式）。
pub const FUNCS: &[&str] = &[
    "boot_inb",
    "boot_outb",
    "boot_inw",
    "boot_outw",
];

