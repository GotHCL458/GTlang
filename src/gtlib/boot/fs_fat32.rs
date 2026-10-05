//! `boot.fs_fat32` 子模块：FAT32 文件系统——完整读写。

pub const FUNCS: &[&str] = &[
    "boot_fat32_find",
    "boot_fat32_read",
    "boot_fat32_write",
    "boot_fat32_delete",
    "boot_fat32_list",
];

