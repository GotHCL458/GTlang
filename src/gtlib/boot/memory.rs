//! `boot.memory` 子模块：memory 组函数登记（实现见 `os/boot/rt_bare.c` / `boot16.asm`）。
//!
//! 命名空间调用：`import boot.memory` 后 `boot.memory.<fn>(...)`；也兼容扁平名。

/// 本组函数名（扁平形式）。
pub const FUNCS: &[&str] = &[
    "boot_mem_alloc",
    "boot_mem_free",
    "boot_mem_size",
];

