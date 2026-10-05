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

extern void irq0_stub(void);
extern void irq1_stub(void);

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
    set_gate(0x20, (unsigned long)irq0_stub);
    set_gate(0x21, (unsigned long)irq1_stub);
    boot_pit_init(1000);
    set_gate(0x20, (unsigned long)irq0_stub);
    set_gate(0x21, (unsigned long)irq1_stub);
    boot_pit_init(1000);
    set_gate(0x20, (unsigned long)irq0_stub);
    set_gate(0x21, (unsigned long)irq1_stub);
    boot_pit_init(1000);
    set_gate(0x20, (unsigned long)irq0_stub);
    set_gate(0x21, (unsigned long)irq1_stub);
    boot_pit_init(1000);
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
    unsigned long long div = 1193182ULL / (unsigned long long)(hz > 0 ? hz : 1000);
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
long long boot_fs_create(const char *name) {
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
static long long fs_find(const char *name) {
    for (int i = 0; i < FS_MAX_FILES; i++) {
        if (!fs_files[i].used) continue;
        int j = 0; int ok = 1;
        while (1) {
            if (fs_files[i].name[j] != name[j]) { ok = 0; break; }
            if (fs_files[i].name[j] == 0) break;
            j++;
        }
        if (ok) return i;
    }
    return -1;
}

long long boot_fs_write(const char *name, const char *data, long long n) {
    long long i = fs_find(name);
    if (i < 0) i = boot_fs_create(name);
    if (i < 0) return -1;
    if (n > FS_DATA_MAX) n = FS_DATA_MAX;
    for (long long k = 0; k < n; k++) fs_files[i].data[k] = (unsigned char)data[k];
    fs_files[i].size = (int)n;
    return n;
}

long long boot_fs_read(const char *name, char *out, long long cap) {
    long long i = fs_find(name);
    if (i < 0) return -1;
    long long n = fs_files[i].size;
    if (n > cap) n = cap;
    for (long long k = 0; k < n; k++) out[k] = (char)fs_files[i].data[k];
    return n;
}

long long boot_fs_size(const char *name) {
    long long i = fs_find(name);
    return i < 0 ? -1 : (long long)fs_files[i].size;
}

long long boot_fs_delete(const char *name) {
    long long i = fs_find(name);
    if (i < 0) return -1;
    fs_files[i].used = 0;
    fs_files[i].size = 0;
    return 0;
}

long long boot_fs_count(void) {
    long long c = 0;
    for (int i = 0; i < FS_MAX_FILES; i++) if (fs_files[i].used) c++;
    return c;
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


extern i64 main(void);
__attribute__((section(".text.entry"), naked)) void kernel_entry(void) {
    __asm__ volatile("call kernel_entry_c");
    __asm__ volatile("1: jmp 1b");
}
void kernel_entry_c(void) { gt_rt_init(); (void)main(); while (1) {} }

