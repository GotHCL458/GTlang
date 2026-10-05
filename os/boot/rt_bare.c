/* GTLang 裸机运行时（引导库提供）：最小实现，串口输出 + 简单堆。 */
typedef unsigned long long u64;
typedef long long i64;

static inline void outb(unsigned short port, unsigned char v) {
    __asm__ volatile("outb %0, %1" :: "a"(v), "Nd"(port));
}
static inline unsigned char inb(unsigned short port) {
    unsigned char v;
    __asm__ volatile("inb %1, %0" : "=a"(v) : "Nd"(port));
    return v;
}
static inline void outw(unsigned short port, unsigned short v) {
    __asm__ volatile("outw %0, %1" :: "a"(v), "Nd"(port));
}
static inline unsigned short inw(unsigned short port) {
    unsigned short v;
    __asm__ volatile("inw %1, %0" : "=a"(v) : "Nd"(port));
    return v;
}

static void serial_init(void) {
    outb(0x3F9, 0); outb(0x3FB, 0x80); outb(0x3F8, 3); outb(0x3F9, 0);
    outb(0x3FB, 3); outb(0x3FA, 0xC7); outb(0x3FC, 0x0B);
}

static void serial_putc(char c) {
    while (!(*(volatile unsigned char *)0x3FD & 0x20)) {}
    outb(0x3F8, (unsigned char)c);
}

void gt_rt_init(void) { serial_init(); }

/* 打印：支持 %lld / %llu / %s / %c / %% （裸机最小子集） */
static void print_ll(i64 v) {
    char buf[24]; int n = 0;
    unsigned long long u = v < 0 ? (unsigned long long)(-v) : (unsigned long long)v;
    if (v < 0) serial_putc('-');
    if (u == 0) { serial_putc('0'); return; }
    while (u) { buf[n++] = (char)('0' + u % 10); u /= 10; }
    while (n) serial_putc(buf[--n]);
}

int gt_printf(const char *fmt, ...) {
    __builtin_va_list ap; __builtin_va_start(ap, fmt);
    for (const char *p = fmt; *p; p++) {
        if (*p != '%') { serial_putc(*p); continue; }
        p++;
        if (*p == 'l') { p++; if (*p == 'l') p++; }
        switch (*p) {
            case 'd': case 'i': print_ll(__builtin_va_arg(ap, i64)); break;
            case 'u': print_ll((i64)__builtin_va_arg(ap, unsigned long long)); break;
            case 's': { const char *s = __builtin_va_arg(ap, const char *); while (*s) serial_putc(*s++); break; }
            case 'c': serial_putc((char)__builtin_va_arg(ap, int)); break;
            case '%': serial_putc('%'); break;
            default: serial_putc('%'); serial_putc(*p); break;
        }
    }
    __builtin_va_end(ap);
    return 0;
}

/* 简单 bump 堆（供 gt_mem_alloc / 容器用） */
static unsigned char heap[1 << 20];
static unsigned long long heap_used = 0;
void *gt_mem_alloc(u64 n) {
    unsigned long long a = (heap_used + 15) & ~15ULL;
    heap_used = a + n;
    return &heap[a];
}
void gt_mem_free(void *p) { (void)p; }

void gt_bounds(i64 i, i64 n, i64 line) { (void)line; serial_putc('!'); print_ll(i); serial_putc('/'); print_ll(n); }
void gt_overflow(i64 line) { (void)line; serial_putc('#'); }
void gt_panic(const char *m) { while (*m) serial_putc(*m++); while (1) {} }

/* ---- boot 标准库（gtlib: boot）---- */

/* ===== interrupt（IDT + PIC + 键盘）===== */

struct idt_entry { unsigned short off_lo; unsigned short sel; unsigned char ist; unsigned char flags; unsigned short off_mid; unsigned int off_hi; unsigned int zero; } __attribute__((packed));
struct idt_ptr { unsigned short limit; unsigned long base; } __attribute__((packed));

static struct idt_entry idt[256];
static struct idt_ptr idtp;

/* 32 个异常 + 16 个 IRQ 的 stub（汇编写，这里用 GCC 属性生成） */
extern void isr_stub_table(void);

/* 键盘环形缓冲 */
#define KBD_BUF 256
static volatile char kbd_buf[KBD_BUF];
static volatile int kbd_head = 0, kbd_tail = 0;

static void kbd_push(char c) {
    int n = (kbd_head + 1) % KBD_BUF;
    if (n != kbd_tail) { kbd_buf[kbd_head] = c; kbd_head = n; }
}

/* 键盘扫描码 -> ASCII（简化，仅字母/数字/回车） */
static const char kbd_map[128] = {
    0, 27, '1','2','3','4','5','6','7','8','9','0','-','=', '\b',
    '\t','q','w','e','r','t','y','u','i','o','p','[',']','\n',
    0,'a','s','d','f','g','h','j','k','l',';','\'', '`',
    0,'\\','z','x','c','v','b','n','m',',','.','/', 0, '*', 0, ' ',
};

#if defined(__x86_64__)
extern void irq0_stub(void);
extern void irq1_stub(void);
#endif

static volatile unsigned long long pit_ticks = 0;

/* 通用 IRQ 处理（由 stub 调用） */

void irq_handler(unsigned long irq) {
    if (irq == 0) { pit_ticks++; }
    if (irq == 1) {
        unsigned char sc = inb(0x60);
        if (!(sc & 0x80) && sc < 128) {
            char c = kbd_map[sc];
            if (c) kbd_push(c);
        }
    }
    /* 发 EOI */
    if (irq >= 8) outb(0xA0, 0x20);
    outb(0x20, 0x20);
}

/* 初始化 PIC（重映射到 0x20..0x2F） */
void boot_pic_init(void) {
    outb(0x20, 0x11); outb(0xA0, 0x11);
    outb(0x21, 0x20); outb(0xA1, 0x28);
    outb(0x21, 0x04); outb(0xA1, 0x02);
    outb(0x21, 0x01); outb(0xA1, 0x01);
    outb(0x21, 0xFC); outb(0xA1, 0xFF);  /* 只开 IRQ0(定时)+IRQ1(键盘) */
}

/* 设置一个中断门（0x20+irq -> handler） */
static void set_gate(int n, unsigned long handler) {
    idt[n].off_lo = handler & 0xFFFF;
    idt[n].sel = 0x18;   /* 长模式 64 位代码段 */
    idt[n].ist = 0;
    idt[n].flags = 0x8E;
    idt[n].off_mid = (handler >> 16) & 0xFFFF;
    idt[n].off_hi = (handler >> 32) & 0xFFFFFFFF;
    idt[n].zero = 0;
}

void boot_pit_init(long long hz);

void boot_idt_init(void) {
    /* 先清零 */
    for (int i = 0; i < 256; i++) set_gate(i, 0);
#if defined(__x86_64__)
    set_gate(0x20, (unsigned long)irq0_stub);
    set_gate(0x21, (unsigned long)irq1_stub);
#endif
    boot_pit_init(1000);
    // 使能 PS/2 键盘（i8042）：读配置字节 -> 开 IRQ1 -> 写回
    while (inb(0x64) & 0x02) {}
    outb(0x64, 0x20);
    while (!(inb(0x64) & 0x01)) {}
    unsigned char kcfg = inb(0x60);
    kcfg |= 0x01;
    kcfg &= ~0x10;
    while (inb(0x64) & 0x02) {}
    outb(0x64, 0x60);
    while (inb(0x64) & 0x02) {}
    outb(0x60, kcfg);
    while (inb(0x64) & 0x01) { inb(0x60); }
    idtp.limit = sizeof(idt) - 1;
    idtp.base = (unsigned long)&idt;
    __asm__ volatile("lidt %0" :: "m"(idtp));
    boot_pic_init();
}

void boot_irq_enable(void) { __asm__ volatile("sti"); }
void boot_irq_disable(void) { __asm__ volatile("cli"); }

/* 取一个键盘字符（无则返回 -1） */
long long boot_keyboard_handler(void) {
    if (kbd_tail == kbd_head) return -1;
    char c = kbd_buf[kbd_tail];
    kbd_tail = (kbd_tail + 1) % KBD_BUF;
    return (long long)c;
}

/* ===== boot 标准库完整实现（gtlib: boot）===== */


/* ---- serial ---- */
void boot_serial_init(void) { serial_init(); }
void boot_serial_putc(long long c) { serial_putc((char)c); }
void boot_serial_puts(const char *s) { while (*s) serial_putc(*s++); }
long long boot_serial_getc(void) {
    while (!(inb(0x3FD) & 0x01)) {}
    return (long long)inb(0x3F8);
}

/* ---- port ---- */
long long boot_inb(long long port) { return (long long)inb((unsigned short)port); }
void boot_outb(long long port, long long v) { outb((unsigned short)port, (unsigned char)v); }
long long boot_inw(long long port) { return (long long)inw((unsigned short)port); }
void boot_outw(long long port, long long v) { outw((unsigned short)port, (unsigned short)v); }

/* ---- system ---- */
void boot_hlt(void) { for (;;) { __asm__ volatile("cli; hlt"); } }
void boot_exit(void) { boot_hlt(); }
void boot_reboot(void) { outb(0x64, 0xFE); for (;;) {} }
void boot_shutdown(void) { outw(0x604, 0x2000); for (;;) {} }

/* ---- memory ---- */
long long boot_mem_alloc(long long n) { return (long long)gt_mem_alloc((u64)n); }
void boot_mem_free(long long p) { (void)p; }
long long boot_mem_size(void) { return 1 << 20; }

/* ---- time（PIT IRQ0 驱动）---- */
void boot_pit_init(long long hz) {
    unsigned int d = (unsigned int)(hz > 0 ? hz : 1000);
    unsigned int div = 1193182u / d;
    outb(0x43, 0x36);
    outb(0x40, (unsigned char)(div & 0xFF));
    outb(0x40, (unsigned char)((div >> 8) & 0xFF));
}
long long boot_time_ms(void) { return (long long)pit_ticks; }
void boot_sleep_ms(long long ms) {
    unsigned long long t = pit_ticks + (unsigned long long)ms;
    while (pit_ticks < t) { __asm__ volatile("hlt"); }
}

/* ---- disk（ATA PIO，主通道）---- */
static void ata_wait(void) {
    for (int i = 0; i < 4; i++) inb(0x1F7);
    while (inb(0x1F7) & 0x80) {}   /* BSY */
}
static int ata_wait_drq(void) {
    ata_wait();
    for (int t = 0; t < 100000; t++) {
        unsigned char st = inb(0x1F7);
        if (st & 0x08) return 0;   /* DRQ */
        if (st & 0x01) return -1;  /* ERR */
    }
    return -1;
}
long long boot_disk_read(long long lba, long long n, long long buf) {
    unsigned short *p = (unsigned short *)buf;
    for (long long s = 0; s < n; s++) {
        unsigned int l = (unsigned int)(lba + s);
        ata_wait();
        outb(0x1F6, (unsigned char)(0xE0 | ((l >> 24) & 0x0F)));
        outb(0x1F2, 1);
        outb(0x1F3, (unsigned char)(l & 0xFF));
        outb(0x1F4, (unsigned char)((l >> 8) & 0xFF));
        outb(0x1F5, (unsigned char)((l >> 16) & 0xFF));
        outb(0x1F7, 0x20);   /* READ SECTORS */
        if (ata_wait_drq() != 0) return -1;
        for (int i = 0; i < 256; i++) p[s * 256 + i] = inw(0x1F0);
    }
    return n;
}
long long boot_disk_write(long long lba, long long n, long long buf) {
    unsigned short *p = (unsigned short *)buf;
    for (long long s = 0; s < n; s++) {
        unsigned int l = (unsigned int)(lba + s);
        ata_wait();
        outb(0x1F6, (unsigned char)(0xE0 | ((l >> 24) & 0x0F)));
        outb(0x1F2, 1);
        outb(0x1F3, (unsigned char)(l & 0xFF));
        outb(0x1F4, (unsigned char)((l >> 8) & 0xFF));
        outb(0x1F5, (unsigned char)((l >> 16) & 0xFF));
        outb(0x1F7, 0x30);   /* WRITE SECTORS */
        if (ata_wait_drq() != 0) return -1;
        for (int i = 0; i < 256; i++) outw(0x1F0, p[s * 256 + i]);
    }
    return n;
}

/* ---- screen / keyboard（BIOS 中断，保护模式/长模式下不可用；占位）---- */
void boot_clear(void) {}
void boot_putc_at(long long c, long long x, long long y) { (void)c; (void)x; (void)y; }
void boot_puts(const char *s) { boot_serial_puts(s); }
long long boot_getkey(void) { return -1; }


/* ===== 简单内存文件系统（ramfs）===== */
#define FS_MAX_FILES 32
#define FS_NAME_MAX 32
#define FS_DATA_MAX 4096

struct fs_file {
    char name[FS_NAME_MAX];
    unsigned char data[FS_DATA_MAX];
    int size;
    int used;
};
static struct fs_file fs_files[FS_MAX_FILES];

/* 创建文件，返回索引（-1 失败） */
int boot_fs_ram_create(const char *name) {
    for (int i = 0; i < FS_MAX_FILES; i++) {
        if (!fs_files[i].used) {
            fs_files[i].used = 1;
            fs_files[i].size = 0;
            int j = 0;
            while (name[j] && j < FS_NAME_MAX - 1) { fs_files[i].name[j] = name[j]; j++; }
            fs_files[i].name[j] = 0;
            return i;
        }
    }
    return -1;
}

/* 按名查找，返回索引（-1 不存在） */
static int fs_find(const char *name) {
    for (int i = 0; i < FS_MAX_FILES; i++) {
        if (!fs_files[i].used) continue;
        int j = 0; int ok = 1;
        while (j < FS_NAME_MAX) {
            if (fs_files[i].name[j] != name[j]) { ok = 0; break; }
            if (fs_files[i].name[j] == 0) break;
            j++;
        }
        if (ok) return i;
    }
    return -1;
}

long long boot_fs_ram_write(const char *name, const char *data, int n) {
    int i = fs_find(name);
    if (i < 0) i = boot_fs_ram_create(name);
    if (i < 0) return -1;
    if (n < 0) n = 0;
    if (n > FS_DATA_MAX) n = FS_DATA_MAX;
    for (long long k = 0; k < n; k++) fs_files[i].data[k] = (unsigned char)data[k];
    fs_files[i].size = (int)n;
    return n;
}

long long boot_fs_ram_read(const char *name, char *out, long long cap) {
    int i = fs_find(name);
    if (i < 0) return -1;
    long long n = fs_files[i].size;
    if (n > cap) n = cap;
    for (long long k = 0; k < n; k++) out[k] = (char)fs_files[i].data[k];
    return n;
}

long long boot_fs_ram_size(const char *name) {
    int i = fs_find(name);
    return i < 0 ? -1 : (long long)fs_files[i].size;
}

long long boot_fs_ram_delete(const char *name) {
    int i = fs_find(name);
    if (i < 0) return -1;
    fs_files[i].used = 0;
    fs_files[i].size = 0;
    return 0;
}

long long boot_fs_ram_count(void) {
    long long c = 0;
    for (int i = 0; i < FS_MAX_FILES; i++) if (fs_files[i].used) c++;
    return c;
}

/* 列出文件名：按 NUL 分隔写入 out，返回文件数（out/cap 可传 0 仅取数量）*/
long long boot_fs_ram_list(long long out, long long cap) {
    char *p = (char *)out;
    long long used_bytes = 0;
    long long count = 0;
    for (int i = 0; i < FS_MAX_FILES; i++) {
        if (!fs_files[i].used) continue;
        count++;
        if (!out || used_bytes >= cap) continue;
        int j = 0;
        while (fs_files[i].name[j] && used_bytes + j + 1 < cap) { p[used_bytes + j] = fs_files[i].name[j]; j++; }
        p[used_bytes + j] = 0;
        used_bytes += j + 1;
    }
    return count;
}


/* ===== FAT16 只读（简化：读根目录 + 文件内容）===== */
static unsigned char fat_buf[512];

/* 读第 n 个根目录项（32 字节），返回 0 成功 */
static int fat16_root_entry(long long idx, unsigned char *out) {
    // 根目录区起始：保留区(1) + FAT 表(2 * sectors_per_fat) —— 典型 FAT16 参数
    // 这里用固定布局（QEMU 生成的 FAT16 镜像）：保留 1 扇区，2 个 FAT，每个 32 扇区
    long long root_lba = 1 + 2 * 32;
    long long lba = root_lba + idx / 16;
    long long off = (idx % 16) * 32;
    if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) return -1;
    for (int i = 0; i < 32; i++) out[i] = fat_buf[off + i];
    return 0;
}

/* 在根目录找文件（8.3 名），返回起始簇（0 未找到） */
long long boot_fat16_find(const char *name83) {
    unsigned char e[32];
    for (long long i = 0; i < 512; i++) {
        if (fat16_root_entry(i, e) != 0) break;
        if (e[0] == 0) break;             // 目录结束
        if (e[0] == 0xE5) continue;       // 已删除
        if (e[11] & 0x08) continue;       // 卷标
        int ok = 1;
        for (int j = 0; j < 11; j++) {
            char c = name83[j];
            if (c >= 'a' && c <= 'z') c -= 32;
            if (e[j] != (unsigned char)c) { ok = 0; break; }
        }
        if (ok) return (long long)(e[26] | (e[27] << 8));
    }
    return 0;
}

long long boot_fat16_read(const char *name83, char *out, long long cap) {
    unsigned char e[32];
    for (long long i = 0; i < 512; i++) {
        if (fat16_root_entry(i, e) != 0) break;
        if (e[0] == 0) break;
        if (e[0] == 0xE5) continue;
        if (e[11] & 0x08) continue;
        int ok = 1;
        for (int j = 0; j < 11; j++) {
            char c = name83[j];
            if (c >= 'a' && c <= 'z') c -= 32;
            if (e[j] != (unsigned char)c) { ok = 0; break; }
        }
        if (!ok) continue;
        long long size = e[28] | (e[29] << 8) | (e[30] << 16) | ((long long)e[31] << 24);
        long long cluster = e[26] | (e[27] << 8);
        long long data_lba = 1 + 2 * 32 + 512 + (cluster - 2) * 1;
        long long n = size < cap ? size : cap;
        unsigned char *p = (unsigned char *)out;
        for (long long k = 0; k < n; k += 512) {
            if (boot_disk_read(data_lba + k / 512, 1, (long long)fat_buf) < 0) break;
            long long m = (n - k) < 512 ? (n - k) : 512;
            for (long long t = 0; t < m; t++) p[k + t] = fat_buf[t];
        }
        return n;
    }
    return -1;
}



/* ---- FAT16 写支持 ---- */
long long boot_fat16_delete(const char *name83);

/* 读 FAT 表项（簇 n 的下一簇） */
static unsigned short fat16_next(long long cluster) {
    long long fat_lba = 1 + (cluster * 2) / 512;
    long long off = (cluster * 2) % 512;
    if (boot_disk_read(fat_lba, 1, (long long)fat_buf) < 0) return 0xFFFF;
    return (unsigned short)(fat_buf[off] | (fat_buf[off + 1] << 8));
}

/* 写 FAT 表项（两个 FAT 副本都写） */
static int fat16_set(long long cluster, unsigned short val) {
    long long fat_lba = 1 + (cluster * 2) / 512;
    long long off = (cluster * 2) % 512;
    for (int copy = 0; copy < 2; copy++) {
        long long lba = fat_lba + copy * 32;
        if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) return -1;
        fat_buf[off] = (unsigned char)(val & 0xFF);
        fat_buf[off + 1] = (unsigned char)((val >> 8) & 0xFF);
        if (boot_disk_write(lba, 1, (long long)fat_buf) < 0) return -1;
    }
    return 0;
}

/* 找一个空闲簇（FAT 表项为 0），返回簇号或 0 */
static long long fat16_alloc_cluster(void) {
    for (long long c = 2; c < 2000; c++) {
        if (fat16_next(c) == 0) return c;
    }
    return 0;
}

/* 在根目录找空项（返回索引）或已删除项 */
static long long fat16_free_entry(void) {
    unsigned char e[32];
    for (long long i = 0; i < 512; i++) {
        if (fat16_root_entry(i, e) != 0) break;
        if (e[0] == 0 || e[0] == 0xE5) return i;
    }
    return -1;
}

/* 写第 idx 个根目录项（32 字节） */
static int fat16_put_entry(long long idx, const unsigned char *e) {
    long long root_lba = 1 + 2 * 32;
    long long lba = root_lba + idx / 16;
    long long off = (idx % 16) * 32;
    if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) return -1;
    for (int i = 0; i < 32; i++) fat_buf[off + i] = e[i];
    return boot_disk_write(lba, 1, (long long)fat_buf);
}

/* 写文件（创建或覆盖）：name83 是 11 字节 8.3 名，返回写入字节数 */
long long boot_fat16_write(const char *name83, const char *data, long long n) {
    // 已存在则删掉旧簇链
    boot_fat16_delete(name83);
    long long idx = fat16_free_entry();
    if (idx < 0) return -1;
    long long first = fat16_alloc_cluster();
    if (first == 0) return -1;
    fat16_set(first, 0xFFFF);   // 先标记为链尾
    long long cluster = first;
    long long written = 0;
    const unsigned char *p = (const unsigned char *)data;
    while (written < n) {
        long long data_lba = 1 + 2 * 32 + 512 + (cluster - 2) * 1;
        long long m = (n - written) < 512 ? (n - written) : 512;
        for (long long k = 0; k < 512; k++) fat_buf[k] = (k < m) ? p[written + k] : 0;
        if (boot_disk_write(data_lba, 1, (long long)fat_buf) < 0) return -1;
        written += m;
        if (written < n) {
            long long next = fat16_alloc_cluster();
            if (next == 0) break;
            fat16_set(cluster, (unsigned short)next);
            cluster = next;
            fat16_set(cluster, 0xFFFF);
        }
    }
    // 写目录项
    unsigned char e[32];
    for (int i = 0; i < 32; i++) e[i] = 0;
    for (int j = 0; j < 11; j++) { char c = name83[j]; if (c >= 'a' && c <= 'z') c -= 32; e[j] = (unsigned char)c; }
    e[11] = 0x20;   // 归档属性
    e[26] = (unsigned char)(first & 0xFF);
    e[27] = (unsigned char)((first >> 8) & 0xFF);
    e[28] = (unsigned char)(n & 0xFF);
    e[29] = (unsigned char)((n >> 8) & 0xFF);
    e[30] = (unsigned char)((n >> 16) & 0xFF);
    e[31] = (unsigned char)((n >> 24) & 0xFF);
    if (fat16_put_entry(idx, e) < 0) return -1;
    return n;
}

/* 删除文件（标记目录项 0xE5 + 释放簇链） */
long long boot_fat16_delete(const char *name83) {
    unsigned char e[32];
    for (long long i = 0; i < 512; i++) {
        if (fat16_root_entry(i, e) != 0) break;
        if (e[0] == 0) break;
        if (e[0] == 0xE5) continue;
        if (e[11] & 0x08) continue;
        int ok = 1;
        for (int j = 0; j < 11; j++) {
            char c = name83[j];
            if (c >= 'a' && c <= 'z') c -= 32;
            if (e[j] != (unsigned char)c) { ok = 0; break; }
        }
        if (!ok) continue;
        // 释放簇链
        long long cluster = e[26] | (e[27] << 8);
        while (cluster >= 2 && cluster < 0xFFF0) {
            unsigned short next = fat16_next(cluster);
            fat16_set(cluster, 0);
            cluster = next;
        }
        e[0] = 0xE5;
        fat16_put_entry(i, e);
        return 0;
    }
    return -1;
}

/* 列出根目录文件名：按 NUL 分隔写入 out，返回文件数 */
long long boot_fat16_list(long long out, long long cap) {
    char *p = (char *)out;
    long long used = 0, count = 0;
    unsigned char e[32];
    for (long long i = 0; i < 512; i++) {
        if (fat16_root_entry(i, e) != 0) break;
        if (e[0] == 0) break;
        if (e[0] == 0xE5) continue;
        if (e[11] & 0x08) continue;
        count++;
        if (!out || used + 13 >= cap) continue;
        for (int j = 0; j < 11; j++) { p[used + j] = e[j] ? (char)e[j] : ' '; }
        p[used + 11] = 0;
        used += 12;
    }
    return count;
}

/* ===== FAT8（简化：8 位簇号，根目录 8.3）===== */
long long boot_fat8_find(const char *name83) {
    // FAT8 布局：保留 1 扇区，1 个 FAT（1 扇区），根目录 16 项
    unsigned char e[32];
    for (long long i = 0; i < 16; i++) {
        long long lba = 2 + i / 16;
        long long off = (i % 16) * 32;
        if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) return 0;
        for (int k = 0; k < 32; k++) e[k] = fat_buf[off + k];
        if (e[0] == 0) break;
        if (e[0] == 0xE5) continue;
        int ok = 1;
        for (int j = 0; j < 11; j++) { char c = name83[j]; if (c >= 'a' && c <= 'z') c -= 32; if (e[j] != (unsigned char)c) { ok = 0; break; } }
        if (ok) return (long long)e[26];
    }
    return 0;
}

long long boot_fat8_read(const char *name83, char *out, long long cap) {
    unsigned char e[32];
    for (long long i = 0; i < 16; i++) {
        long long lba = 2 + i / 16;
        long long off = (i % 16) * 32;
        if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) return -1;
        for (int k = 0; k < 32; k++) e[k] = fat_buf[off + k];
        if (e[0] == 0) break;
        if (e[0] == 0xE5) continue;
        int ok = 1;
        for (int j = 0; j < 11; j++) { char c = name83[j]; if (c >= 'a' && c <= 'z') c -= 32; if (e[j] != (unsigned char)c) { ok = 0; break; } }
        if (!ok) continue;
        long long size = e[28] | (e[29] << 8);
        long long n = size < cap ? size : cap;
        unsigned char *p = (unsigned char *)out;
        for (long long k = 0; k < n; k += 512) {
            if (boot_disk_read(18 + k / 512, 1, (long long)fat_buf) < 0) break;
            long long m = (n - k) < 512 ? (n - k) : 512;
            for (long long t = 0; t < m; t++) p[k + t] = fat_buf[t];
        }
        return n;
    }
    return -1;
}

long long boot_fat8_list(long long out, long long cap) {
    char *p = (char *)out;
    long long used = 0, count = 0;
    unsigned char e[32];
    for (long long i = 0; i < 16; i++) {
        long long lba = 2 + i / 16;
        long long off = (i % 16) * 32;
        if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) break;
        for (int k = 0; k < 32; k++) e[k] = fat_buf[off + k];
        if (e[0] == 0) break;
        if (e[0] == 0xE5) continue;
        count++;
        if (!out || used + 13 >= cap) continue;
        for (int j = 0; j < 11; j++) p[used + j] = e[j] ? (char)e[j] : ' ';
        p[used + 11] = 0;
        used += 12;
    }
    return count;
}

/* ===== FAT32（简化：32 位簇号，根目录为簇链）===== */
static unsigned int fat32_next(long long cluster, long long fat_lba) {
    long long off = (cluster * 4) % 512;
    long long lba = fat_lba + (cluster * 4) / 512;
    if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) return 0xFFFFFFFF;
    return (unsigned int)(fat_buf[off] | (fat_buf[off+1] << 8) | (fat_buf[off+2] << 16) | ((unsigned int)fat_buf[off+3] << 24));
}

long long boot_fat32_find(const char *name83) {
    // 简化：根目录簇固定从 2 开始，数据区从 LBA 100 开始
    unsigned char e[32];
    long long cluster = 2;
    long long fat_lba = 32;
    while (cluster >= 2 && cluster < 0x0FFFFFF0) {
        long long lba = 100 + (cluster - 2) * 1;
        if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) break;
        for (long long i = 0; i < 16; i++) {
            for (int k = 0; k < 32; k++) e[k] = fat_buf[i * 32 + k];
            if (e[0] == 0) return 0;
            if (e[0] == 0xE5) continue;
            if (e[11] & 0x08) continue;
            int ok = 1;
            for (int j = 0; j < 11; j++) { char c = name83[j]; if (c >= 'a' && c <= 'z') c -= 32; if (e[j] != (unsigned char)c) { ok = 0; break; } }
            if (ok) return (long long)(e[26] | (e[27] << 8) | (e[20] << 16) | ((unsigned int)e[21] << 24));
        }
        cluster = fat32_next(cluster, fat_lba);
    }
    return 0;
}

long long boot_fat32_read(const char *name83, char *out, long long cap) {
    long long cluster = boot_fat32_find(name83);
    if (cluster < 2) return -1;
    long long fat_lba = 32;
    long long total = 0;
    unsigned char *p = (unsigned char *)out;
    while (cluster >= 2 && cluster < 0x0FFFFFF0 && total < cap) {
        long long lba = 100 + (cluster - 2) * 1;
        if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) break;
        long long m = (cap - total) < 512 ? (cap - total) : 512;
        for (long long k = 0; k < m; k++) p[total + k] = fat_buf[k];
        total += m;
        cluster = fat32_next(cluster, fat_lba);
    }
    return total;
}

long long boot_fat32_list(long long out, long long cap) {
    char *p = (char *)out;
    long long used = 0, count = 0;
    unsigned char e[32];
    long long cluster = 2;
    long long fat_lba = 32;
    while (cluster >= 2 && cluster < 0x0FFFFFF0) {
        long long lba = 100 + (cluster - 2) * 1;
        if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) break;
        for (long long i = 0; i < 16; i++) {
            for (int k = 0; k < 32; k++) e[k] = fat_buf[i * 32 + k];
            if (e[0] == 0) return count;
            if (e[0] == 0xE5) continue;
            if (e[11] & 0x08) continue;
            count++;
            if (!out || used + 13 >= cap) continue;
            for (int j = 0; j < 11; j++) p[used + j] = e[j] ? (char)e[j] : ' ';
            p[used + 11] = 0;
            used += 12;
        }
        cluster = fat32_next(cluster, fat_lba);
    }
    return count;
}

long long boot_fat32_delete(const char *name83);

/* 写 FAT32 表项（简化：只写第一个 FAT 副本） */
static int fat32_set(long long cluster, unsigned int val) {
    long long off = (cluster * 4) % 512;
    long long lba = 32 + (cluster * 4) / 512;
    if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) return -1;
    fat_buf[off] = (unsigned char)(val & 0xFF);
    fat_buf[off+1] = (unsigned char)((val >> 8) & 0xFF);
    fat_buf[off+2] = (unsigned char)((val >> 16) & 0xFF);
    fat_buf[off+3] = (unsigned char)((val >> 24) & 0xFF);
    return boot_disk_write(lba, 1, (long long)fat_buf);
}

/* 找空闲簇（FAT 表项为 0） */
static long long fat32_alloc_cluster(void) {
    for (long long c = 3; c < 1000; c++) {
        if (fat32_next(c, 32) == 0) return c;
    }
    return 0;
}

/* 在根目录簇链里找空项（返回簇 + 项索引的高低位） */
static long long fat32_free_slot(long long *out_lba, long long *out_off) {
    unsigned char e[32];
    long long cluster = 2;
    while (cluster >= 2 && cluster < 0x0FFFFFF0) {
        long long lba = 100 + (cluster - 2) * 1;
        if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) return -1;
        for (long long i = 0; i < 16; i++) {
            for (int k = 0; k < 32; k++) e[k] = fat_buf[i * 32 + k];
            if (e[0] == 0 || e[0] == 0xE5) { *out_lba = lba; *out_off = i * 32; return 0; }
        }
        cluster = fat32_next(cluster, 32);
    }
    return -1;
}

long long boot_fat32_write(const char *name83, const char *data, long long n) {
    boot_fat32_delete(name83);
    long long lba = 0, off = 0;
    if (fat32_free_slot(&lba, &off) != 0) return -1;
    long long first = fat32_alloc_cluster();
    if (first == 0) return -1;
    fat32_set(first, 0x0FFFFFFF);
    long long cluster = first;
    long long written = 0;
    const unsigned char *p = (const unsigned char *)data;
    while (written < n) {
        long long dlba = 100 + (cluster - 2) * 1;
        long long m = (n - written) < 512 ? (n - written) : 512;
        for (long long k = 0; k < 512; k++) fat_buf[k] = (k < m) ? p[written + k] : 0;
        if (boot_disk_write(dlba, 1, (long long)fat_buf) < 0) return -1;
        written += m;
        if (written < n) {
            long long next = fat32_alloc_cluster();
            if (next == 0) break;
            fat32_set(cluster, (unsigned int)next);
            cluster = next;
            fat32_set(cluster, 0x0FFFFFFF);
        }
    }
    unsigned char e[32];
    for (int i = 0; i < 32; i++) e[i] = 0;
    for (int j = 0; j < 11; j++) { char c = name83[j]; if (c >= 'a' && c <= 'z') c -= 32; e[j] = (unsigned char)c; }
    e[11] = 0x20;
    e[26] = (unsigned char)(first & 0xFF);
    e[27] = (unsigned char)((first >> 8) & 0xFF);
    e[20] = (unsigned char)((first >> 16) & 0xFF);
    e[21] = (unsigned char)((first >> 24) & 0xFF);
    e[28] = (unsigned char)(n & 0xFF);
    e[29] = (unsigned char)((n >> 8) & 0xFF);
    e[30] = (unsigned char)((n >> 16) & 0xFF);
    e[31] = (unsigned char)((n >> 24) & 0xFF);
    if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) return -1;
    for (int i = 0; i < 32; i++) fat_buf[off + i] = e[i];
    if (boot_disk_write(lba, 1, (long long)fat_buf) < 0) return -1;
    return n;
}

long long boot_fat32_delete(const char *name83) {
    unsigned char e[32];
    long long cluster = 2;
    while (cluster >= 2 && cluster < 0x0FFFFFF0) {
        long long lba = 100 + (cluster - 2) * 1;
        if (boot_disk_read(lba, 1, (long long)fat_buf) < 0) break;
        for (long long i = 0; i < 16; i++) {
            for (int k = 0; k < 32; k++) e[k] = fat_buf[i * 32 + k];
            if (e[0] == 0) return -1;
            if (e[0] == 0xE5) continue;
            int ok = 1;
            for (int j = 0; j < 11; j++) { char c = name83[j]; if (c >= 'a' && c <= 'z') c -= 32; if (e[j] != (unsigned char)c) { ok = 0; break; } }
            if (!ok) continue;
            long long fc = e[26] | (e[27] << 8) | (e[20] << 16) | ((unsigned int)e[21] << 24);
            while (fc >= 2 && fc < 0x0FFFFFF0) {
                unsigned int next = fat32_next(fc, 32);
                fat32_set(fc, 0);
                fc = next;
            }
            fat_buf[i * 32] = 0xE5;
            return boot_disk_write(lba, 1, (long long)fat_buf);
        }
        cluster = fat32_next(cluster, 32);
    }
    return -1;
}

/* ===== VGA 文本模式直写（0xB8000，80x25）===== */
#define VGA_BASE 0xB8000
#define VGA_COLS 80
#define VGA_ROWS 25
static int vga_row = 0, vga_col = 0;
static unsigned char vga_attr = 0x07;   /* 浅灰字 / 黑底 */

static void vga_scroll(void) {
    unsigned short *v = (unsigned short *)VGA_BASE;
    for (int i = 0; i < (VGA_ROWS - 1) * VGA_COLS; i++) v[i] = v[i + VGA_COLS];
    for (int i = (VGA_ROWS - 1) * VGA_COLS; i < VGA_ROWS * VGA_COLS; i++) v[i] = (vga_attr << 8) | ' ';
    vga_row = VGA_ROWS - 1;
}

void boot_vga_clear(void) {
    unsigned short *v = (unsigned short *)VGA_BASE;
    for (int i = 0; i < VGA_ROWS * VGA_COLS; i++) v[i] = (vga_attr << 8) | ' ';
    vga_row = 0; vga_col = 0;
}

void boot_vga_set_color(long long fg, long long bg) {
    vga_attr = (unsigned char)(((bg & 0x0F) << 4) | (fg & 0x0F));
}

void boot_vga_putc(long long c) {
    unsigned short *v = (unsigned short *)VGA_BASE;
    if (c == '\n') { vga_col = 0; vga_row++; }
    else if (c == '\r') { vga_col = 0; }
    else {
        v[vga_row * VGA_COLS + vga_col] = (unsigned short)((vga_attr << 8) | (c & 0xFF));
        vga_col++;
        if (vga_col >= VGA_COLS) { vga_col = 0; vga_row++; }
    }
    if (vga_row >= VGA_ROWS) vga_scroll();
    /* 更新硬件光标 */
    unsigned short pos = (unsigned short)(vga_row * VGA_COLS + vga_col);
    outb(0x3D4, 0x0F); outb(0x3D5, (unsigned char)(pos & 0xFF));
    outb(0x3D4, 0x0E); outb(0x3D5, (unsigned char)((pos >> 8) & 0xFF));
}

void boot_vga_puts(const char *s) { while (*s) boot_vga_putc((unsigned char)*s++); }

/* ===== system：CPUID ===== */
void boot_cpuid(long long leaf, long long out) {
    unsigned int *p = (unsigned int *)out;
    unsigned int a, b, c, d;
    __asm__ volatile("cpuid" : "=a"(a), "=b"(b), "=c"(c), "=d"(d) : "a"((unsigned int)leaf), "c"(0));
    if (p) { p[0] = a; p[1] = b; p[2] = c; p[3] = d; }
}

long long boot_cpu_vendor(long long out) {
    unsigned int *p = (unsigned int *)out;
    unsigned int a, b, c, d;
    __asm__ volatile("cpuid" : "=a"(a), "=b"(b), "=c"(c), "=d"(d) : "a"(0u), "c"(0u));
    if (p) { p[0] = b; p[1] = d; p[2] = c; }   // EBX,EDX,ECX -> 12 字节厂商串
    return 0;
}

/* ===== time：RTC（CMOS）===== */
static unsigned char cmos_read(unsigned char reg) {
    outb(0x70, reg);
    return inb(0x71);
}
static int bcd2bin(unsigned char v) { return (v & 0x0F) + ((v >> 4) * 10); }

/* 读 RTC 时间：out 指向 6 字节（秒 分 时 日 月 年），返回 0 */
long long boot_rtc_read(long long out) {
    unsigned char *p = (unsigned char *)out;
    while (cmos_read(0x0A) & 0x80) {}   // 等 UIP 清零
    if (p) {
        p[0] = (unsigned char)bcd2bin(cmos_read(0x00));  // 秒
        p[1] = (unsigned char)bcd2bin(cmos_read(0x02));  // 分
        p[2] = (unsigned char)bcd2bin(cmos_read(0x04));  // 时
        p[3] = (unsigned char)bcd2bin(cmos_read(0x07));  // 日
        p[4] = (unsigned char)bcd2bin(cmos_read(0x08));  // 月
        p[5] = (unsigned char)bcd2bin(cmos_read(0x09));  // 年
    }
    return 0;
}

/* ===== disk：MBR 分区表解析 ===== */
/* 读 MBR（LBA 0），解析 4 个分区项；out 指向 4x2 的 u32（起始 LBA, 扇区数）*/
long long boot_disk_partitions(long long out) {
    unsigned int *p = (unsigned int *)out;
    if (boot_disk_read(0, 1, (long long)fat_buf) < 0) return -1;
    for (int i = 0; i < 4; i++) {
        unsigned char *e = &fat_buf[446 + i * 16];
        unsigned int lba = e[8] | (e[9] << 8) | (e[10] << 16) | ((unsigned int)e[11] << 24);
        unsigned int cnt = e[12] | (e[13] << 8) | (e[14] << 16) | ((unsigned int)e[15] << 24);
        if (p) { p[i*2] = lba; p[i*2+1] = cnt; }
    }
    return 0;
}
/* ===== 简单协作式多任务（轮转调度）===== */
#define TASK_MAX 8
#define TASK_STACK 8192

struct task {
    unsigned long rsp;       /* 保存的栈指针 */
    unsigned long stack[TASK_STACK / 8];
    int used;
    int state;               /* 0=free 1=ready 2=running 3=done */
};
static struct task tasks[TASK_MAX];
static int cur_task = -1;
static unsigned long dummy_rsp = 0;

#if defined(__x86_64__)
extern void task_switch(unsigned long *old_rsp, unsigned long new_rsp);
#endif

long long boot_task_create(void (*entry)(void)) {
    // slot 0 保留给主任务
    for (int i = 1; i < TASK_MAX; i++) {
        if (tasks[i].used) continue;
        tasks[i].used = 1;
        tasks[i].state = 1;
        // 预留"6 个被保存寄存器 + 返回地址"（与 task_switch 的 pop/ret 顺序对应）
        unsigned long *sp = &tasks[i].stack[TASK_STACK / 8 - 7];
        for (int k = 0; k < 6; k++) sp[k] = 0;   // r15,r14,r13,r12,rbx,rbp
        sp[6] = (unsigned long)entry;            // 返回地址 = 入口
        tasks[i].rsp = (unsigned long)sp;
        return i;
    }
    return -1;
}

void boot_task_yield(void) {
    int n = TASK_MAX;
    for (int k = 1; k <= n; k++) {
        int i = (cur_task + k + n) % n;
        if (tasks[i].used && tasks[i].state == 1) {
            int prev = cur_task;
            cur_task = i;
            tasks[i].state = 2;
#if defined(__x86_64__)
            if (prev >= 0) { tasks[prev].state = 1; task_switch(&tasks[prev].rsp, tasks[i].rsp); }
            else { task_switch(&dummy_rsp, tasks[i].rsp); }
#else
            (void)prev;   /* 32 位暂无 task_switch 实现 */
#endif
            return;
        }
    }
}

void boot_task_start(void) {
    // 主任务登记为 tasks[0]（占一个槽），这样它能被轮转切回。
    cur_task = 0;
    tasks[0].used = 1;
    tasks[0].state = 2;   // running
    boot_task_yield();
}

/* ---- info ---- */
const char *boot_version(void) { return "boot 0.0.1d"; }
long long boot_arch(void) {
#if defined(__x86_64__)
    return 64;
#elif defined(__i386__)
    return 32;
#else
    return 16;
#endif
}


extern char __bss_start[];
extern char __bss_end[];
__attribute__((noinline, optimize("O0"))) static void clear_bss(void) {
    unsigned long *p = (unsigned long *)__bss_start;
    unsigned long *e = (unsigned long *)__bss_end;
    while (p < e) { *p++ = 0; }
}

extern i64 main(void);
__attribute__((section(".text.entry"), naked)) void kernel_entry(void) {
    __asm__ volatile("call kernel_entry_c");
    __asm__ volatile("1: jmp 1b");
}
void kernel_entry_c(void) { clear_bss(); gt_rt_init(); (void)main(); while (1) {} }

