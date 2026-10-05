//! `boot.system` 子模块：system 组函数登记（实现见 `os/boot/rt_bare.c` / `boot16.asm`）。
//!
//! 命名空间调用：`import boot.system` 后 `boot.system.<fn>(...)`；也兼容扁平名。

/// 本组函数名（扁平形式）。
pub const FUNCS: &[&str] = &[
    "boot_hlt",
    "boot_exit",
    "boot_reboot",
    "boot_shutdown",
];

