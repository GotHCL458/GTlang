//! `boot.fs_fat16` 子模块：FAT16 文件系统——完整读写。

pub const FUNCS: &[&str] = &[
    "boot_fat16_find",
    "boot_fat16_read",
    "boot_fat16_write",
    "boot_fat16_delete",
    "boot_fat16_list",
];

