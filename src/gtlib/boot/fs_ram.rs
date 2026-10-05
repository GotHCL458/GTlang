//! `boot.fs_ram` 子模块：内存文件系统（ramfs）——完整 CRUD。
//!
//! 命名空间调用：`import boot.fs_ram` 后 `boot.fs_ram.write(...)`；也兼容扁平名 `boot_fs_ram_write(...)`。

pub const FUNCS: &[&str] = &[
    "boot_fs_ram_create",
    "boot_fs_ram_write",
    "boot_fs_ram_read",
    "boot_fs_ram_size",
    "boot_fs_ram_delete",
    "boot_fs_ram_count",
    "boot_fs_ram_list",
];

