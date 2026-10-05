//! `boot.fs` 子模块：fs 组函数登记（实现见 `os/boot/rt_bare.c` / `boot16.asm`）。
//!
//! 命名空间调用：`import boot.fs` 后 `boot.fs.<fn>(...)`；也兼容扁平名。

/// 本组函数名（扁平形式）。
pub const FUNCS: &[&str] = &[
    "boot_fs_create",
    "boot_fs_write",
    "boot_fs_read",
    "boot_fs_size",
    "boot_fs_delete",
    "boot_fs_count",
    "boot_fat16_find",
    "boot_fat16_read",
];

