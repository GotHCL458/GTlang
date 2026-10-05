/* GTLang 裸机运行时（引导库提供）：最小实现，串口输出 + 简单堆。 */
typedef unsigned long long u64;
typedef long long i64;

static inline void outb(unsigned short port, unsigned char v) {
    __asm__ volatile("outb %0, %1" :: "a"(v), "Nd"(port));
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
/* ===== boot 标准库完整实现（gtlib: boot）===== */

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

/* ---- time ---- */
long long boot_time_ms(void) { return 0; }
void boot_sleep_ms(long long ms) { (void)ms; }

/* ---- disk（LBA 暂未实现，返回 -1）---- */
long long boot_disk_read(long long lba, long long n, long long buf) { (void)lba; (void)n; (void)buf; return -1; }
long long boot_disk_write(long long lba, long long n, long long buf) { (void)lba; (void)n; (void)buf; return -1; }

/* ---- screen / keyboard（BIOS 中断，保护模式/长模式下不可用；占位）---- */
void boot_clear(void) {}
void boot_putc_at(long long c, long long x, long long y) { (void)c; (void)x; (void)y; }
void boot_puts(const char *s) { boot_serial_puts(s); }
long long boot_getkey(void) { return -1; }

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
