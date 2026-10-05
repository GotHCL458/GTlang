# boot 标准库参考 / Boot Library Reference

> `gtlib` 的 `boot` 模块：**裸机引导库**，唯一可被导入用于引导的库。
> 三套实现：`boot16.asm`（16 位，`gtc --asm16` 自组装）+ `rt_bare.c`（32/64 位）。

## 用法

```gt
import boot

fn main() {
    boot_serial_init()
    boot_serial_puts("hello from GTLang OS\n")
    boot_serial_putc(48 + boot_arch() / 10)
    boot_hlt()
}
```

编译 + 运行：

```bat
gtc --bare --boot kernel.gt -o kernel.img   REM 一键出可启动镜像
qemu-system-x86_64 -drive format=raw,file=kernel.img -serial stdio
```

## 函数（38 个 × 11 组）

### serial（串口 COM1）
| 函数 | 说明 |
|---|---|
| `boot_serial_init()` | 初始化 COM1（8N1 + FIFO）|
| `boot_serial_putc(c)` | 输出一个字节 |
| `boot_serial_puts(s)` | 输出字符串 |
| `boot_serial_getc()` | 读一个字节（阻塞）|

### screen（BIOS 文本模式；仅 16 位）
| 函数 | 说明 |
|---|---|
| `boot_clear()` | 清屏 |
| `boot_putc_at(c, x, y)` | 指定位置输出 |
| `boot_puts(s)` | 输出字符串（32/64 位回退到串口）|

### keyboard（PS/2）
| 函数 | 说明 |
|---|---|
| `boot_getkey()` | 取键（BIOS，16 位）|
| `boot_keyboard_handler()` | 取键（IDT 环形缓冲，32/64 位）|

### memory（bump 堆）
| 函数 | 说明 |
|---|---|
| `boot_mem_alloc(n)` | 分配 n 字节 |
| `boot_mem_free(p)` | 释放（no-op）|
| `boot_mem_size()` | 堆大小 |

### time
| 函数 | 说明 |
|---|---|
| `boot_time_ms()` | 毫秒（BIOS tick / PIT）|
| `boot_sleep_ms(ms)` | 睡眠 |

### system
| 函数 | 说明 |
|---|---|
| `boot_hlt()` | 停机 |
| `boot_exit()` | 退出（= hlt）|
| `boot_reboot()` | 重启 |
| `boot_shutdown()` | 关机 |

### disk（LBA）
| 函数 | 说明 |
|---|---|
| `boot_disk_read(lba, n, buf)` | 读扇区 |
| `boot_disk_write(lba, n, buf)` | 写扇区 |

### port（端口 IO）
| 函数 | 说明 |
|---|---|
| `boot_inb(port)` / `boot_outb(port, v)` | 8 位端口 |
| `boot_inw(port)` / `boot_outw(port, v)` | 16 位端口 |

### interrupt（IDT/PIC）
| 函数 | 说明 |
|---|---|
| `boot_idt_init()` | 初始化 IDT + PIC |
| `boot_irq_enable()` / `boot_irq_disable()` | 开/关中断 |
| `boot_pic_init()` | 重映射 PIC |
| `boot_keyboard_handler()` | 键盘环形缓冲取键 |

### fs（内存文件系统 ramfs）
| 函数 | 说明 |
|---|---|
| `boot_fs_create(name)` | 创建文件 |
| `boot_fs_write(name, data, n)` | 写入 |
| `boot_fs_read(name, buf, cap)` | 读出（返回字节数）|
| `boot_fs_size(name)` | 文件大小 |
| `boot_fs_delete(name)` | 删除 |
| `boot_fs_count()` | 文件数 |
| `boot_fat16_find(name83)` | FAT16 按 8.3 名找文件（返回起始簇）|
| `boot_fat16_read(name83, buf, cap)` | FAT16 读文件 |

### info
| 函数 | 说明 |
|---|---|
| `boot_version()` | 版本串 |
| `boot_arch()` | 架构（16/32/64）|

## 三架构

| 架构 | 内核编译 | 引导 |
|---|---|---|
| **x86_16** | `gtc --asm16gen k.gt` | `gtc --asm16 boot16.asm` + `run16.py` |
| **x86_32** | `gtc --bare --target x86_32 k.gt` | stage1+stage2（保护模式）|
| **x86_64** | `gtc --bare --target x86_64 k.gt` | stage1+stage2（长模式）|

## `gtc --bare --boot`（一键可启动镜像）

```bat
gtc --bare --boot kernel.gt -o kernel.img                    REM x86_64（长模式）
gtc --bare --boot --target x86_32 kernel.gt -o k32.img       REM x86_32（保护模式）
gtc --asm16gen kernel.gt -o k16.bin                          REM x86_16
```

宿主机（Windows/Linux）**不提供**这些符号；仅裸机目标链接引导库。

