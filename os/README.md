# GTLang OS（三架构裸机示例）

用 GTLang 写内核，`gtc` 生成裸机目标，配合独立**引导库**在 QEMU 上运行。
**x86_16 / x86_32 / x86_64 三套架构全部跑通。**

## 组成

| 部分 | 说明 |
|---|---|
| `kernel.gt` | 内核（GTLang，`import boot`）|
| `boot/stage1.asm` | MBR 引导扇区（512B）：加载 stage2 |
| `boot/stage2.asm` | 16→32 位保护模式→64 位长模式，加载内核到 `0x10000`，跳转 |
| `boot/boot16.asm` | **16 位 boot 库**（用 `gtc --asm16` 自组装，自举闭环）|
| `boot/rt_bare.c` | **32/64 位 boot 库**（25 函数：串口/端口/系统/内存/时间/磁盘/屏幕/键盘/信息）|
| `boot/kernel.ld` | 链接脚本（入口 `0x10000`）|
| `build.py` | 一键构建（32/64）+ QEMU 运行 |
| `run16.py` | 16 位镜像运行 |

## 用法

```bat
python os/build.py                              REM 32/64 位（默认 x86_64）
gtc --asm16 os/boot/boot16.asm -o boot16.bin    REM 16 位库自组装
gtc --asm16gen os/t16.gt -o t16.bin             REM 16 位内核
python os/run16.py t16.bin                      REM 16 位运行
```

输出经 COM1（`-serial stdio`）打印。

## 目标与后端

| 目标 | 命令 | 后端 |
|---|---|---|
| x86_16 | `gtc --asm16gen <内核.gt>` | AST → x86-16 机器码（`codegen_asm16`）|
| x86_16（汇编）| `gtc --asm16 <文件.asm>` | 内置 16 位汇编器（与 nasm 逐字节一致）|
| x86_32 | `gtc --bare --target x86_32 <内核.gt>` | clang → i386 目标文件 |
| x86_64 | `gtc --bare --target x86_64 <内核.gt>` | clang → x86_64 目标文件 + 长模式引导 |

## boot 标准库（gtlib: boot）

25 个函数 × 9 组（serial / screen / keyboard / memory / time / system / disk / port / info）：

```gt
import boot
fn main() {
    boot_serial_init()
    boot_serial_puts("hello")
    put(boot_arch())     // 16 / 32 / 64
    boot_hlt()
}
```

宿主机链接时不提供这些符号（仅裸机目标链接引导库）。

