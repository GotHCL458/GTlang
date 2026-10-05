//! `boot.fs_fat8` 子模块：FAT8 文件系统（只读）。

pub const FUNCS: &[&str] = &[
    "boot_fat8_find",
    "boot_fat8_read",
    "boot_fat8_list",
];

