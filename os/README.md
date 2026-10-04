# GTLang OS（裸机示例）

用 GTLang 写内核，`gtc --bare` 生成裸机目标，配合独立**引导库**在 QEMU 上运行。

## 组成

| 部分 | 说明 |
|---|---|
| `kernel.gt` | 内核（GTLang 源码，`import boot`）|
| `boot/stage1.asm` | MBR 引导扇区（512B）：加载 stage2 |
| `boot/stage2.asm` | 16→32 位保护模式，加载内核到 `0x10000`，跳转 |
| `boot/rt_bare.c` | 裸机运行时 + `boot` 标准库实现（串口/堆/停机）|
| `boot/kernel.ld` | 链接脚本（入口 `0x10000`）|
| `build.py` | 一键构建 + 启动 QEMU + 串口输出 |

## 用法

```bat
python os/build.py
```

输出经 COM1（`-serial stdio`）打印。

## 目标

- `gtc --bare --target x86_32 <内核.gt>` → 32 位裸机 `.o`
- `gtc --bare --target x86_64 <内核.gt>` → 64 位裸机 `.o`

## boot 标准库（gtlib）

`import boot` 后可用：`boot_serial_init` / `boot_serial_putc` / `boot_hlt` /
`boot_exit` / `boot_mem_alloc` / `boot_time_ms` / `boot_reboot`。
宿主程序不含这些符号（仅裸机目标链接引导库）。

