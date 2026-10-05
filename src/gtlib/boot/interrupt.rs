//! `boot.interrupt` 子模块：interrupt 组函数登记（实现见 `os/boot/rt_bare.c` / `boot16.asm`）。
//!
//! 命名空间调用：`import boot.interrupt` 后 `boot.interrupt.<fn>(...)`；也兼容扁平名。

/// 本组函数名（扁平形式）。
pub const FUNCS: &[&str] = &[
    "boot_idt_init",
    "boot_irq_enable",
    "boot_irq_disable",
    "boot_pic_init",
    "boot_keyboard_handler",
];

