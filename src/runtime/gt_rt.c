#define _CRT_SECURE_NO_WARNINGS 1

/* ============================================================
 * gt_rt.c —— gtc_rust 内置运行时（随编译器二进制内嵌分发）
 *
 * 输出编码策略（覆盖中文 / 日文 / 韩文 / emoji 等任意 Unicode）：
 *   1) stdout 是控制台 → MultiByteToWideChar(UTF-8→UTF-16) + WriteConsoleW
 *      直接写宽字符，完全绕过控制台代码页(GBK/Shift-JIS/...)，不会乱码；
 *   2) stdout 被重定向到文件/管道 → 按原始 UTF-8 字节 fwrite，
 *      产出标准 UTF-8 文件，便于后续处理。
 * 因此无需修改调用方控制台的 chcp，也不会污染父 shell 的代码页。
 * ============================================================ */

#include <stdio.h>
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include <windows.h>
#include <io.h>
#include <fcntl.h>

/* 统一分配器：用 Windows 进程堆（HeapAlloc），而非 CRT 的 malloc。
 * 好处：进程堆是「整个进程共享」的，不同 CRT（libcmt / msvcrt）都能
 * 用 HeapFree 释放对方分配的内存 —— 跨 CRT 释放不再崩溃。
 * （GTLang 与标准库 DLL 处于不同 CRT，serve_fn 等回调需要这种可跨 CRT 释放的内存。） */
#define gt_alloc(n)   HeapAlloc(GetProcessHeap(), 0, (n))
#define gt_free(p)    HeapFree(GetProcessHeap(), 0, (p))
/* realloc 不能用于 HeapAlloc 内存（不同堆）；用 HeapReAlloc 替代。 */
#define gt_realloc(p, n) HeapReAlloc(GetProcessHeap(), 0, (p), (n))

/* 自动内存管理（引用计数 + 预留环检测）。--no-gc 时退化为裸分配。
 * 直接包含实现（gc.c 内含 gc.h），使 gt_rt.c 自成编译单元；
 * driver.rs / build.bat 只需把 gc.c、gc.h 与 gt_rt.c 放在同一目录即可。 */
#include "gc.c"

/* 运行时消息语言：默认英文；编译器在用 `zh` 模式编译时会调用 gt_rt_set_zh() 切到中文。 */
static int GT_ZH = 0;
void gt_rt_set_zh(void) { GT_ZH = 1; }

/* 关闭自动内存管理（--no-gc 编译时由生成程序调用；不回收，靠进程结束回收）。 */
void gt_rt_set_gc(int on) { gc_set_enabled(on); }

/* 进程初始化：设控制台输出为 UTF-8（让 C 的 printf 也能正确显示中文），
 * 并把 stdout 设为二进制模式——否则 Windows 文本模式会把 \n 转成 \r\n，
 * 使 AOT 产物（含内联 C 的 printf）与 JIT（Rust print!）换行字节不一致。 */
void gt_rt_init(void) {
    static int done = 0;
    if (done) return;
    done = 1;
    SetConsoleOutputCP(65001 /* CP_UTF8 */);
    _setmode(_fileno(stdout), _O_BINARY);
}

/* 开启控制台的 VT 序列处理，否则 Windows 控制台不认 ANSI 转义（首次调用时生效） */
static void gt_enable_vt(void) {
    static int done = 0;
    if (done) {
        return;
    }
    done = 1;
    /* 把控制台输出代码页设为 UTF-8：C 的 printf 直接写 UTF-8 字节也能正确显示 */
    SetConsoleOutputCP(65001 /* CP_UTF8 */);
    HANDLE h = GetStdHandle(STD_OUTPUT_HANDLE);
    DWORD mode = 0;
    if (h != NULL && h != INVALID_HANDLE_VALUE && GetConsoleMode(h, &mode)) {
        SetConsoleMode(h, mode | 0x0004 /* ENABLE_VIRTUAL_TERMINAL_PROCESSING */);
    }
}

/* 按“控制台感知”的方式输出一段 UTF-8 字节 */
static void gt_write(const char *s, int len) {
    if (s == NULL || len <= 0) {
        return;
    }
    gt_enable_vt();

    HANDLE h = GetStdHandle(STD_OUTPUT_HANDLE);
    DWORD mode = 0;
    if (h != NULL && h != INVALID_HANDLE_VALUE && GetConsoleMode(h, &mode)) {
        /* 控制台：转 UTF-16 后用 WriteConsoleW 输出 */
        int n = MultiByteToWideChar(CP_UTF8, 0, s, len, NULL, 0);
        if (n > 0) {
            wchar_t *w = (wchar_t *)gt_alloc(sizeof(wchar_t) * (size_t)n);
            if (w != NULL) {
                MultiByteToWideChar(CP_UTF8, 0, s, len, w, n);
                DWORD written = 0;
                WriteConsoleW(h, w, (DWORD)n, &written, NULL);
                gt_free(w);
                return;
            }
        }
    }

    /* 重定向 / 转换失败：走 CRT stdout（与内联 C 的 printf 共用同一缓冲，
     * 保证输出顺序一致）；stdout 已在 gt_rt_init 设为二进制，\n 不会被翻成 \r\n。 */
    fwrite(s, 1, (size_t)len, stdout);
}

/* 把格式化结果写入调用方缓冲（供“插值字符串作为值”使用）。
 * 不用 sprintf：UCRT 下 sprintf 并非可链接的真实导出符号。 */
int gt_sprintf(char *buf, unsigned long long cap, const char *fmt, ...) {
    va_list ap;
    if (buf == NULL || cap == 0) {
        return -1;
    }
    va_start(ap, fmt);
    int n = vsnprintf(buf, (size_t)cap, fmt, ap);
    va_end(ap);
    if (n < 0) {
        buf[0] = '\0';
        return -1;
    }
    if ((unsigned long long)n >= cap) {
        buf[cap - 1] = '\0';
    }
    return n;
}

/* 字符串 → 整数（int() 内置）。
 * 语义：解析开头的可选数值前缀，忽略尾部非数字字符；无有效前缀返回 0。
 * 固定十进制，与解释器（Cranelift）侧的实现保持一致。 */
long long gt_to_i64(const char *s) {
    if (s == NULL) {
        return 0;
    }
    return (long long)strtoll(s, NULL, 10);
}

/* 字符串 → 浮点（f64() 内置）。非法输入返回 0 */
double gt_to_f64(const char *s) {
    if (s == NULL) {
        return 0.0;
    }
    return strtod(s, NULL);
}

/* 整数除零：打印诊断并终止。
 * 必须显式检查——LLVM 的 sdiv/srem 除零是 UB（会产出任意值），
 * 而 CPU 的 idiv 会直接崩溃，两边表现不一致。 */
void gt_div_zero(long long line) {
    char buf[192];
    int n = GT_ZH
        ? snprintf(buf, sizeof buf, "\n[运行时错误] 第 %lld 行：整数除数为 0\n", line)
        : snprintf(buf, sizeof buf, "\n[runtime error] line %lld: integer division by zero\n", line);
    if (n > 0) {
        gt_write(buf, n);
    }
    exit(1);
}

/* 数组/字符串下标越界：打印诊断并终止程序。
 * 编译器后端把它插在每次下标访问前（无符号比较判定），解释器后端调用同一语义。 */
void gt_bounds(long long idx, long long len, long long line) {
    char buf[256];
    int n = GT_ZH
        ? snprintf(buf, sizeof buf, "\n[运行时错误] 第 %lld 行：下标 %lld 越界（长度 %lld）\n", line, idx, len)
        : snprintf(buf, sizeof buf, "\n[runtime error] line %lld: index %lld out of bounds (length %lld)\n", line, idx, len);
    if (n > 0) {
        gt_write(buf, n);
    }
    exit(1);
}

/* 未捕获异常：打印诊断并终止 */
void gt_uncaught(long long code) {
    char buf[160];
    int n = GT_ZH
        ? snprintf(buf, sizeof buf, "\n[运行时错误] 未捕获的异常（错误码 %lld）\n", code)
        : snprintf(buf, sizeof buf, "\n[runtime error] uncaught exception (code %lld)\n", code);
    if (n > 0) {
        gt_write(buf, n);
    }
    exit(1);
}

/* 整数算术溢出：打印诊断并终止。
 * 编译器后端在 add/sub/mul 后用溢出内建检查；解释器后端调用 rt_overflow（同语义）。 */
void gt_overflow(long long line) {
    char buf[192];
    int n = GT_ZH
        ? snprintf(buf, sizeof buf, "\n[运行时错误] 第 %lld 行：整数运算溢出\n", line)
        : snprintf(buf, sizeof buf, "\n[runtime error] line %lld: integer arithmetic overflow\n", line);
    if (n > 0) {
        gt_write(buf, n);
    }
    exit(1);
}

/* Result[T,E] 运行时：堆块 [tag, payload]（tag=0 Ok / 1 Err） */
void *gt_result_new(long long tag, long long payload) {
    long long *p = (long long *)gt_alloc(16);
    if (p) { p[0] = tag; p[1] = payload; }
    return p;
}
long long gt_result_tag(void *p) { return p ? ((long long *)p)[0] : 0; }
long long gt_result_val(void *p) { return p ? ((long long *)p)[1] : 0; }

/* printf 兼容的格式化输出入口：所有 GTLang 的 put/print 都走这里 */
int gt_printf(const char *fmt, ...) {
    va_list ap;
    char stackbuf[1024];

    va_start(ap, fmt);
    int n = vsnprintf(stackbuf, sizeof stackbuf, fmt, ap);
    va_end(ap);
    if (n < 0) {
        return n;
    }

    if (n < (int)sizeof stackbuf) {
        gt_write(stackbuf, n);
        return n;
    }

    /* 超长输出：按需扩容，避免截断 */
    char *big = (char *)gt_alloc((size_t)n + 1);
    if (big == NULL) {
        gt_write(stackbuf, (int)sizeof stackbuf - 1);
        return n;
    }
    va_start(ap, fmt);
    vsnprintf(big, (size_t)n + 1, fmt, ap);
    va_end(ap);
    gt_write(big, n);
    gt_free(big);
    return n;
}

/* 立即刷出（重定向场景使用 stdout 缓冲） */
void gt_flush(void) {
    fflush(stdout);
}

/* ============================================================
 * 容器运行时：list / set / map（引用语义，堆分配）
 *
 * 统一用 `void*` 句柄在 GTLang 侧表示；元素/键/值一律是 8 字节槽
 * （i64 / f64 位模式 / 指针），与解释器后端的 Rust 实现逐字节一致。
 *
 * 语义对齐解释器（jit.rs 的 gt_list_* 等）：
 *   - list：可增长数组，push/pop/at/len/has/remove
 *   - set ：去重（线性扫描，相等按 8 字节槽比较），insert/has/remove/len
 *   - map ：键值对（线性扫描），insert/get/has/remove/keys/values/len
 * 线性扫描对小规模容器足够，且实现简单、行为可预测。
 * ============================================================ */

typedef struct { long long rc; long long *data; long long len; long long cap; long long elem_ptr; } GtList;
typedef struct {
    long long rc;
    long long *data;   /* 插入顺序数组（for 遍历按此顺序） */
    long long len;
    long long cap;
    long long *ht;     /* 开放寻址哈希桶：存 data 下标，-1 表示空；hcap 为 2 的幂 */
    long long hcap;
    long long elem_ptr; /* 元素是否为指针（GC 环检测用） */
} GtSet;
typedef struct {
    long long rc;
    long long *keys;   /* 插入顺序数组（keys/vals/for 遍历按此顺序） */
    long long *vals;
    long long len;
    long long cap;
    long long *ht;     /* 开放寻址哈希桶：存 keys 下标，-1 表示空；大小 hcap 为 2 的幂 */
    long long hcap;
    long long elem_ptr; /* 键/值是否为指针（GC 环检测用） */
    long long key_str;  /* 键是否为字符串（按内容比较/哈希）；0 按整数 */
} GtMap;

/* map 哈希：64 位键 → 桶下标（Fibonacci hashing） */
static unsigned long long gt_hash64(unsigned long long x) {
    x ^= x >> 33;
    x *= 0xff51afd7ed558ccdULL;
    x ^= x >> 33;
    x *= 0xc4ceb9fe1a85ec53ULL;
    x ^= x >> 33;
    return x;
}
static unsigned long long gt_hash_str(const char *s);
static void gt_map_rehash(GtMap *m, long long newcap) {
    gt_free(m->ht);
    m->hcap = newcap;
    m->ht = (long long *)gt_alloc(sizeof(long long) * (size_t)newcap);
    for (long long i = 0; i < newcap; i++) m->ht[i] = -1;
    for (long long i = 0; i < m->len; i++) {
        unsigned long long h = (m->key_str
            ? gt_hash_str((const char *)(size_t)m->keys[i])
            : gt_hash64((unsigned long long)m->keys[i])) & (unsigned long long)(newcap - 1);
        while (m->ht[h] != -1) h = (h + 1) & (unsigned long long)(newcap - 1);
        m->ht[h] = i;
    }
}
static long long gt_map_find(GtMap *m, long long k) {
    if (m->hcap == 0) return -1;
    unsigned long long mask = (unsigned long long)(m->hcap - 1);
    if (m->key_str) {
        const char *ks = (const char *)(size_t)k;
        unsigned long long h = gt_hash_str(ks) & mask;
        while (m->ht[h] != -1) {
            long long idx = m->ht[h];
            const char *cur = (const char *)(size_t)m->keys[idx];
            if (cur && ks && strcmp(cur, ks) == 0) return idx;
            h = (h + 1) & mask;
        }
        return -1;
    }
    unsigned long long h = gt_hash64((unsigned long long)k) & mask;
    while (m->ht[h] != -1) {
        long long idx = m->ht[h];
        if (m->keys[idx] == k) return idx;
        h = (h + 1) & mask;
    }
    return -1;
}

/* 字符串键：内容哈希（FNV-1a），配合 gt_map_find 的 key_str 分支使用。 */
static unsigned long long gt_hash_str(const char *s) {
    unsigned long long h = 1469598103934665603ULL;
    while (s && *s) { h ^= (unsigned char)*s++; h *= 1099511628211ULL; }
    return h;
}

/* 前向声明：容器释放与分派（定义在文件后部） */
static void gt_dispatch_free(void *p);
void gt_list_free(void *p);
void gt_set_free(void *p);
void gt_map_free(void *p);

/* ---------- list ---------- */
GtList *gt_list_new(long long elem_ptr) {
    GtList *l = (GtList *)gc_alloc(sizeof(GtList));
    l->rc = 1;
    l->cap = 4;
    l->len = 0;
    l->elem_ptr = elem_ptr;
    l->data = (long long *)gt_alloc(sizeof(long long) * (size_t)l->cap);
    gc_set_meta(l, 1, elem_ptr);   /* kind=1(list) */
    return l;
}

static void gt_list_grow(GtList *l) {
    if (l->len >= l->cap) {
        l->cap *= 2;
        l->data = (long long *)gt_realloc(l->data, sizeof(long long) * (size_t)l->cap);
    }
}

void gt_list_push(GtList *l, long long v) {
    gt_list_grow(l);
    l->data[l->len++] = v;
    /* 元素自动引用计数：元素是指针类型时 inc（容器持有它） */
    if (l->elem_ptr && v) gc_inc((void *)(size_t)v);
}

long long gt_list_pop(GtList *l) {
    if (l->len == 0) return 0;
    return l->data[--l->len];
}

/* list 切片：返回新 list，元素 [lo, hi)。 */
GtList *gt_list_slice(GtList *l, long long lo, long long hi) {
    GtList *out = gt_list_new(l ? l->elem_ptr : 0);
    if (!l) return out;
    if (lo < 0) lo = 0;
    if (hi > l->len) hi = l->len;
    for (long long i = lo; i < hi; i++) gt_list_push(out, l->data[i]);
    return out;
}

long long gt_list_at(GtList *l, long long i) {
    if (i < 0 || i >= l->len) return 0;
    return l->data[i];
}

void gt_list_set(GtList *l, long long i, long long v) {
    if (i >= 0 && i < l->len) {
        if (l->elem_ptr) {
            long long old = l->data[i];
            if (old) gc_dec((void *)(size_t)old, gt_dispatch_free);
            if (v) gc_inc((void *)(size_t)v);
        }
        l->data[i] = v;
    }
}

long long gt_list_len(GtList *l) { return l ? l->len : 0; }

long long gt_list_has(GtList *l, long long v) {
    for (long long i = 0; i < l->len; i++) {
        if (l->data[i] == v) return 1;
    }
    return 0;
}

void gt_list_remove(GtList *l, long long i) {
    if (i < 0 || i >= l->len) return;
    for (long long k = i; k + 1 < l->len; k++) l->data[k] = l->data[k + 1];
    l->len--;
}

/* ---------- set ---------- */
/* set 哈希：复用 gt_hash64；桶存 data 下标，-1 空 */
static void gt_set_rehash(GtSet *s, long long newcap) {
    gt_free(s->ht);
    s->hcap = newcap;
    s->ht = (long long *)gt_alloc(sizeof(long long) * (size_t)newcap);
    for (long long i = 0; i < newcap; i++) s->ht[i] = -1;
    for (long long i = 0; i < s->len; i++) {
        unsigned long long mask = (unsigned long long)(newcap - 1);
        unsigned long long h = gt_hash64((unsigned long long)s->data[i]) & mask;
        while (s->ht[h] != -1) h = (h + 1) & mask;
        s->ht[h] = i;
    }
}
static long long gt_set_find(GtSet *s, long long v) {
    if (s->hcap == 0) return -1;
    unsigned long long mask = (unsigned long long)(s->hcap - 1);
    unsigned long long h = gt_hash64((unsigned long long)v) & mask;
    while (s->ht[h] != -1) {
        long long idx = s->ht[h];
        if (s->data[idx] == v) return idx;
        h = (h + 1) & mask;
    }
    return -1;
}

GtSet *gt_set_new(long long elem_ptr) {
    GtSet *s = (GtSet *)gc_alloc(sizeof(GtSet));
    s->rc = 1;
    s->cap = 4;
    s->len = 0;
    s->elem_ptr = elem_ptr;
    s->data = (long long *)gt_alloc(sizeof(long long) * (size_t)s->cap);
    gc_set_meta(s, 2, elem_ptr);   /* kind=2(set) */
    s->hcap = 8;
    s->ht = (long long *)gt_alloc(sizeof(long long) * (size_t)s->hcap);
    for (long long i = 0; i < s->hcap; i++) s->ht[i] = -1;
    return s;
}

void gt_set_insert(GtSet *s, long long v) {
    if (gt_set_find(s, v) >= 0) return; /* 已存在 */
    if ((s->len + 1) * 10 >= s->hcap * 7) {
        gt_set_rehash(s, s->hcap * 2);
    }
    if (s->len >= s->cap) {
        s->cap *= 2;
        s->data = (long long *)gt_realloc(s->data, sizeof(long long) * (size_t)s->cap);
    }
    long long idx = s->len;
    s->data[idx] = v;
    s->len++;
    if (s->elem_ptr && v) gc_inc((void *)(size_t)v); /* 元素自动引用计数 */
    unsigned long long mask = (unsigned long long)(s->hcap - 1);
    unsigned long long h = gt_hash64((unsigned long long)v) & mask;
    while (s->ht[h] != -1) h = (h + 1) & mask;
    s->ht[h] = idx;
}

long long gt_set_has(GtSet *s, long long v) {
    return gt_set_find(s, v) >= 0 ? 1 : 0;
}

void gt_set_remove(GtSet *s, long long v) {
    long long i = gt_set_find(s, v);
    if (i < 0) return;
    for (long long k = i; k + 1 < s->len; k++) s->data[k] = s->data[k + 1];
    s->len--;
    gt_set_rehash(s, s->hcap); /* 下标全变，重建哈希 */
}

long long gt_set_len(GtSet *s) { return s ? s->len : 0; }

/* ---------- map ---------- */
GtMap *gt_map_new(long long elem_ptr, long long key_str) {
    GtMap *m = (GtMap *)gc_alloc(sizeof(GtMap));
    m->rc = 1;
    m->cap = 4;
    m->len = 0;
    m->elem_ptr = elem_ptr;
    m->key_str = key_str;
    gc_set_meta(m, 3, elem_ptr);   /* kind=3(map) */
    m->keys = (long long *)gt_alloc(sizeof(long long) * (size_t)m->cap);
    m->vals = (long long *)gt_alloc(sizeof(long long) * (size_t)m->cap);
    m->hcap = 8;
    m->ht = (long long *)gt_alloc(sizeof(long long) * (size_t)m->hcap);
    for (long long i = 0; i < m->hcap; i++) m->ht[i] = -1;
    return m;
}

static void gt_map_sync_key(GtMap *m, long long key_str);
void gt_map_insert(GtMap *m, long long k, long long v);
void gt_map_insert_key(GtMap *m, long long k, long long v, long long key_str) {
    if (m == NULL) return;
    gt_map_sync_key(m, key_str);
    gt_map_insert(m, k, v);
}

void gt_map_insert(GtMap *m, long long k, long long v) {
    long long i = gt_map_find(m, k);
    if (i >= 0) {
        m->vals[i] = v;
        return;
    }
    /* 装载因子 > 0.7 时扩容并重建哈希 */
    if ((m->len + 1) * 10 >= m->hcap * 7) {
        gt_map_rehash(m, m->hcap * 2);
    }
    if (m->len >= m->cap) {
        m->cap *= 2;
        m->keys = (long long *)gt_realloc(m->keys, sizeof(long long) * (size_t)m->cap);
        m->vals = (long long *)gt_realloc(m->vals, sizeof(long long) * (size_t)m->cap);
    }
    long long idx = m->len;
    m->keys[idx] = k;
    m->vals[idx] = v;
    m->len++;
    if (m->elem_ptr) {
        if (k) gc_inc((void *)(size_t)k);
        if (v) gc_inc((void *)(size_t)v);
    }
    unsigned long long mask = (unsigned long long)(m->hcap - 1);
    unsigned long long h = (m->key_str
        ? gt_hash_str((const char *)(size_t)k)
        : gt_hash64((unsigned long long)k)) & mask;
    while (m->ht[h] != -1) h = (h + 1) & mask;
    m->ht[h] = idx;
}

/* 若调用点声明的键类型与 map 当前记录的不同，先切换并重建哈希。 */
static void gt_map_sync_key(GtMap *m, long long key_str) {
    if (m->key_str != key_str) { m->key_str = key_str; gt_map_rehash(m, m->hcap); }
}

long long gt_map_get_key(GtMap *m, long long k, long long key_str) {
    if (m == NULL) return 0;
    gt_map_sync_key(m, key_str);
    long long i = gt_map_find(m, k);
    return i >= 0 ? m->vals[i] : 0;
}

long long gt_map_get(GtMap *m, long long k) {
    long long i = gt_map_find(m, k);
    return i >= 0 ? m->vals[i] : 0;
}

long long gt_map_has_key(GtMap *m, long long k, long long key_str) {
    if (m == NULL) return 0;
    gt_map_sync_key(m, key_str);
    return gt_map_find(m, k) >= 0 ? 1 : 0;
}

long long gt_map_has(GtMap *m, long long k) {
    return gt_map_find(m, k) >= 0 ? 1 : 0;
}

void gt_map_remove_key(GtMap *m, long long k, long long key_str) {
    if (m == NULL) return;
    gt_map_sync_key(m, key_str);
    long long i = gt_map_find(m, k);
    if (i < 0) return;
    for (long long j = i; j + 1 < m->len; j++) {
        m->keys[j] = m->keys[j + 1];
        m->vals[j] = m->vals[j + 1];
    }
    m->len--;
    gt_map_rehash(m, m->hcap);
}

void gt_map_remove(GtMap *m, long long k) {
    long long i = gt_map_find(m, k);
    if (i < 0) return;
    for (long long j = i; j + 1 < m->len; j++) {
        m->keys[j] = m->keys[j + 1];
        m->vals[j] = m->vals[j + 1];
    }
    m->len--;
    /* 删除后重建哈希（下标全变） */
    gt_map_rehash(m, m->hcap);
}

long long gt_map_len(GtMap *m) { return m ? m->len : 0; }

/* keys/values 返回新的 list */
GtList *gt_map_keys(GtMap *m) {
    GtList *l = gt_list_new(0);
    for (long long i = 0; i < m->len; i++) gt_list_push(l, m->keys[i]);
    return l;
}

GtList *gt_map_values(GtMap *m) {
    GtList *l = gt_list_new(0);
    for (long long i = 0; i < m->len; i++) gt_list_push(l, m->vals[i]);
    return l;
}

/* ============================================================
 * 大整数（bignum）与 tagged int
 *
 * 表示（8 字节，低位 tag）：
 *   偶数 (v & 1 == 0)：小整数，值 = v >> 1（算术右移，i63 范围）
 *   奇数 (v & 1 == 1)：bignum 指针，值 = (GtBig*)(v ^ 1)
 * 小整数运算走寄存器快路径；溢出才升 bignum。比较/打印/转换/哈希按此分派。
 * ============================================================ */
typedef struct GtBig {
    long long rc;      /* 引用计数（-1 = 不回收，便于简单起见先常驻） */
    int neg;           /* 是否负数 */
    long long n;       /* limb 个数 */
    unsigned int *limb;/* 基 2^32 limb（小端） */
} GtBig;

/* tagged 值构造/判定 */
#define GT_INT_TAG(v)   ((v) & 1LL)
#define GT_SMALL(n)     ((long long)((unsigned long long)(n) << 1))
#define GT_BIGP(v)      ((GtBig *)(size_t)((v) & ~1LL))
#define GT_FROM_BIG(p)  ((long long)(size_t)((unsigned long long)(size_t)(p) | 1ULL))

static void gt_big_normalize(GtBig *b);
static GtBig *gt_big_from_i64(long long v);
static long long gt_big_to_i64_if_fits(const GtBig *b, int *ok);

/* 小整数（tagged）→ 有符号 i63 值 */
static long long gt_small_val(long long v) { return v >> 1; }

/* bignum 分配（n 个 limb 全 0） */
static GtBig *gt_big_alloc(long long n) {
    GtBig *b = (GtBig *)gt_alloc(sizeof(GtBig));
    b->rc = -1; b->neg = 0; b->n = n;
    b->limb = n > 0 ? (unsigned int *)gt_alloc(sizeof(unsigned int) * (size_t)n) : 0;
    for (long long i = 0; i < n; i++) b->limb[i] = 0;
    return b;
}

/* 去掉高位 0，维护 0 的符号 */
static void gt_big_normalize(GtBig *b) {
    while (b->n > 0 && b->limb[b->n - 1] == 0) b->n--;
    if (b->n == 0) b->neg = 0;
}

/* i64 → bignum（绝对值） */
static GtBig *gt_big_from_i64(long long v) {
    unsigned long long a = v < 0 ? (unsigned long long)(-(v + 1)) + 1ULL : (unsigned long long)v;
    long long n = a == 0 ? 0 : (a <= 0xFFFFFFFFULL ? 1 : 2);
    GtBig *b = gt_big_alloc(n);
    b->neg = v < 0 ? 1 : 0;
    if (n >= 1) b->limb[0] = (unsigned int)(a & 0xFFFFFFFFULL);
    if (n >= 2) b->limb[1] = (unsigned int)(a >> 32);
    gt_big_normalize(b);
    return b;
}

/* 若 bignum 落在 i63（tagged 小整数）范围，返回其值并置 ok=1 */
static long long gt_big_to_i64_if_fits(const GtBig *b, int *ok) {
    if (b->n > 2) { *ok = 0; return 0; }
    unsigned long long a = 0;
    if (b->n >= 1) a |= (unsigned long long)b->limb[0];
    if (b->n >= 2) a |= ((unsigned long long)b->limb[1]) << 32;
    /* tagged 小整数可表示 [-2^62, 2^62-1]（左移 1 位 + tag） */
    if (b->neg) {
        if (a <= 0x4000000000000000ULL) { *ok = 1; return -(long long)a; }
    } else {
        if (a <= 0x3FFFFFFFFFFFFFFFULL) { *ok = 1; return (long long)a; }
    }
    *ok = 0;
    return 0;
}

/* 把 tagged 值读成"符号 + 绝对值 limb 视图"（用于统一运算）；返回临时 bignum 或小整数 */
static GtBig *gt_int_to_big(long long v) {
    if (GT_INT_TAG(v)) return GT_BIGP(v);
    return gt_big_from_i64(gt_small_val(v));
}

/* 可能"把小整数结果收窄回 tagged 小整数"：给定 bignum，能收窄则返回 tagged 小整数 */
static long long gt_big_maybe_narrow(GtBig *b) {
    int ok = 0;
    long long v = gt_big_to_i64_if_fits(b, &ok);
    if (ok) return GT_SMALL(v);
    return GT_FROM_BIG(b);
}

/* 归一化一个 tagged 结果：若是 bignum 且能收窄为小整数，则收窄 */
static long long gt_int_narrow(long long v) {
    if (GT_INT_TAG(v)) return gt_big_maybe_narrow(GT_BIGP(v));
    return v;
}

/* ---------- bignum 加法（|a| + |b| 或差），带符号 ---------- */
/* 比较绝对值：a>b 返回 1，a<b 返回 -1，相等 0 */
static int gt_big_cmp_abs(const GtBig *a, const GtBig *b) {
    if (a->n != b->n) return a->n > b->n ? 1 : -1;
    for (long long i = a->n - 1; i >= 0; i--) {
        if (a->limb[i] != b->limb[i]) return a->limb[i] > b->limb[i] ? 1 : -1;
    }
    return 0;
}

/* |a| + |b|（结果符号另定） */
static GtBig *gt_big_add_abs(const GtBig *a, const GtBig *b) {
    long long n = (a->n > b->n ? a->n : b->n) + 1;
    GtBig *r = gt_big_alloc(n);
    unsigned long long carry = 0;
    for (long long i = 0; i < n; i++) {
        unsigned long long s = carry;
        if (i < a->n) s += a->limb[i];
        if (i < b->n) s += b->limb[i];
        r->limb[i] = (unsigned int)(s & 0xFFFFFFFFULL);
        carry = s >> 32;
    }
    gt_big_normalize(r);
    return r;
}

/* |a| - |b|（要求 |a| >= |b|） */
static GtBig *gt_big_sub_abs(const GtBig *a, const GtBig *b) {
    GtBig *r = gt_big_alloc(a->n);
    long long borrow = 0;
    for (long long i = 0; i < a->n; i++) {
        long long d = (long long)a->limb[i] - (i < b->n ? (long long)b->limb[i] : 0) - borrow;
        if (d < 0) { d += 0x100000000LL; borrow = 1; } else { borrow = 0; }
        r->limb[i] = (unsigned int)d;
    }
    gt_big_normalize(r);
    return r;
}

/* 带符号加 */
static GtBig *gt_big_add_signed(const GtBig *a, const GtBig *b) {
    if (a->neg == b->neg) {
        GtBig *r = gt_big_add_abs(a, b);
        r->neg = a->neg;
        gt_big_normalize(r);
        return r;
    }
    int c = gt_big_cmp_abs(a, b);
    if (c == 0) { GtBig *z = gt_big_alloc(0); return z; }
    if (c > 0) { GtBig *r = gt_big_sub_abs(a, b); r->neg = a->neg; return r; }
    GtBig *r = gt_big_sub_abs(b, a); r->neg = b->neg; return r;
}

/* 带符号乘 */
static GtBig *gt_big_mul_signed(const GtBig *a, const GtBig *b) {
    if (a->n == 0 || b->n == 0) return gt_big_alloc(0);
    GtBig *r = gt_big_alloc(a->n + b->n);
    for (long long i = 0; i < a->n; i++) {
        unsigned long long carry = 0;
        for (long long j = 0; j < b->n; j++) {
            unsigned long long cur = (unsigned long long)r->limb[i + j]
                + (unsigned long long)a->limb[i] * b->limb[j] + carry;
            r->limb[i + j] = (unsigned int)(cur & 0xFFFFFFFFULL);
            carry = cur >> 32;
        }
        long long k = i + b->n;
        while (carry) {
            unsigned long long cur = (unsigned long long)r->limb[k] + carry;
            r->limb[k] = (unsigned int)(cur & 0xFFFFFFFFULL);
            carry = cur >> 32;
            k++;
        }
    }
    r->neg = (a->neg != b->neg);
    gt_big_normalize(r);
    return r;
}

/* ---------- 对外运算（tagged 入口，快路径优先） ---------- */
long long gt_int_add(long long a, long long b) {
    if (!GT_INT_TAG(a) && !GT_INT_TAG(b)) {
        long long x = gt_small_val(a), y = gt_small_val(b);
        long long r;
        if (!__builtin_add_overflow(x, y, &r) && r <= 0x3FFFFFFFFFFFFFFFLL && r >= -0x4000000000000000LL) {
            return GT_SMALL(r);
        }
    }
    GtBig *A = gt_int_to_big(a), *B = gt_int_to_big(b);
    return gt_big_maybe_narrow(gt_big_add_signed(A, B));
}
long long gt_int_sub(long long a, long long b) {
    if (!GT_INT_TAG(a) && !GT_INT_TAG(b)) {
        long long x = gt_small_val(a), y = gt_small_val(b);
        long long r;
        if (!__builtin_sub_overflow(x, y, &r) && r <= 0x3FFFFFFFFFFFFFFFLL && r >= -0x4000000000000000LL) {
            return GT_SMALL(r);
        }
    }
    GtBig *A = gt_int_to_big(a);
    GtBig *B = gt_int_to_big(b);
    B->neg = !B->neg; /* a - b = a + (-b) */
    return gt_big_maybe_narrow(gt_big_add_signed(A, B));
}
long long gt_int_mul(long long a, long long b) {
    if (!GT_INT_TAG(a) && !GT_INT_TAG(b)) {
        long long x = gt_small_val(a), y = gt_small_val(b);
        long long r;
        if (!__builtin_mul_overflow(x, y, &r) && r <= 0x3FFFFFFFFFFFFFFFLL && r >= -0x4000000000000000LL) {
            return GT_SMALL(r);
        }
    }
    GtBig *A = gt_int_to_big(a), *B = gt_int_to_big(b);
    return gt_big_maybe_narrow(gt_big_mul_signed(A, B));
}

long long gt_int_fmt(long long v, char *buf, long long cap);
/* tagged int 打印到 stdout。 */
void gt_int_print(long long v) {
    char buf[4096];
    long long n = gt_int_fmt(v, buf, (long long)sizeof(buf));
    if (n > 0) { gt_write(buf, n); }
}

/* ---------- 打印 / 转字符串 ---------- */
/* tagged int → 十进制字符串（写入调用方缓冲；返回写入长度）。 */
long long gt_int_fmt(long long v, char *buf, long long cap) {
    if (!GT_INT_TAG(v)) {
        return (long long)snprintf(buf, (size_t)cap, "%lld", gt_small_val(v));
    }
    GtBig *b = GT_BIGP(v);
    if (b->n == 0) { if (cap > 1) { buf[0] = '0'; buf[1] = 0; return 1; } return 0; }
    /* 复制 limb 到临时，反复除以 10 取余 */
    unsigned int *tmp = (unsigned int *)gt_alloc(sizeof(unsigned int) * (size_t)b->n);
    for (long long i = 0; i < b->n; i++) tmp[i] = b->limb[i];
    long long n = b->n;
    char rev[2048]; long long rn = 0;
    while (n > 0 && rn < (long long)sizeof(rev) - 2) {
        unsigned long long rem = 0;
        for (long long i = n - 1; i >= 0; i--) {
            unsigned long long cur = (rem << 32) | tmp[i];
            tmp[i] = (unsigned int)(cur / 10);
            rem = cur % 10;
        }
        while (n > 0 && tmp[n - 1] == 0) n--;
        rev[rn++] = (char)('0' + (int)rem);
    }
    long long out = 0;
    if (b->neg && cap > 1) buf[out++] = '-';
    while (rn > 0 && out < cap - 1) buf[out++] = rev[--rn];
    buf[out] = 0;
    gt_free(tmp);
    return out;
}

/* ---------- 除法 / 取余（tagged；结果收窄） ---------- */
static GtBig *gt_big_copy(const GtBig *b) {
    GtBig *r = gt_big_alloc(b->n);
    r->neg = b->neg;
    for (long long i = 0; i < b->n; i++) r->limb[i] = b->limb[i];
    return r;
}
/* 长除法：a / b（带符号，截断向零），返回商；余数可选写入 *rem */
static GtBig *gt_big_divmod(const GtBig *a, const GtBig *b, GtBig **rem) {
    /* 简化：若都能放进 i64 且不越界，走原生 */
    /* 通用：逐位二进制长除法（够用，非最快） */
    GtBig *q = gt_big_alloc(a->n);
    GtBig *r = gt_big_alloc(0);
    for (long long bit = a->n * 32 - 1; bit >= 0; bit--) {
        /* r = r*2 + bit(a) */
        /* r <<= 1 */
        long long rn = r->n + 1;
        unsigned int *nl = (unsigned int *)gt_alloc(sizeof(unsigned int) * (size_t)rn);
        for (long long i = 0; i < rn; i++) nl[i] = 0;
        unsigned long long carry = 0;
        for (long long i = 0; i < r->n; i++) {
            unsigned long long s = ((unsigned long long)r->limb[i] << 1) | carry;
            nl[i] = (unsigned int)(s & 0xFFFFFFFFULL);
            carry = s >> 32;
        }
        if (carry) nl[r->n] = (unsigned int)carry;
        gt_free(r->limb); r->limb = nl; r->n = rn;
        unsigned int abit = (a->limb[bit / 32] >> (bit % 32)) & 1u;
        r->limb[0] |= abit;
        gt_big_normalize(r);
        if (gt_big_cmp_abs(r, b) >= 0) {
            /* r -= b */
            GtBig *nr = gt_big_sub_abs(r, b);
            gt_free(r->limb); r->limb = nr->limb; r->n = nr->n;
            gt_free(nr);
            gt_big_normalize(r);
            q->limb[bit / 32] |= (1u << (bit % 32));
        }
    }
    gt_big_normalize(q);
    if (rem) { *rem = r; } else { gt_free(r->limb); gt_free(r); }
    return q;
}
long long gt_int_div(long long a, long long b) {
    if (!GT_INT_TAG(a) && !GT_INT_TAG(b)) {
        long long x = gt_small_val(a), y = gt_small_val(b);
        if (y != 0) {
            long long r = x / y;
            if (r <= 0x3FFFFFFFFFFFFFFFLL && r >= -0x4000000000000000LL) return GT_SMALL(r);
        }
    }
    GtBig *A = gt_int_to_big(a), *B = gt_int_to_big(b);
    if (B->n == 0) return GT_SMALL(0); /* 除零由 codegen 的 gt_bounds/div0 检查拦 */
    GtBig *q = gt_big_divmod(A, B, 0);
    q->neg = (A->neg != B->neg);
    gt_big_normalize(q);
    return gt_big_maybe_narrow(q);
}
long long gt_int_rem(long long a, long long b) {
    if (!GT_INT_TAG(a) && !GT_INT_TAG(b)) {
        long long x = gt_small_val(a), y = gt_small_val(b);
        if (y != 0) {
            long long r = x % y;
            if (r <= 0x3FFFFFFFFFFFFFFFLL && r >= -0x4000000000000000LL) return GT_SMALL(r);
        }
    }
    GtBig *A = gt_int_to_big(a), *B = gt_int_to_big(b);
    if (B->n == 0) return GT_SMALL(0);
    GtBig *r = 0;
    gt_big_divmod(A, B, &r);
    r->neg = A->neg;
    gt_big_normalize(r);
    return gt_big_maybe_narrow(r);
}

/* ---------- 比较（tagged）：返回 -1/0/1 ---------- */
long long gt_int_cmp(long long a, long long b) {
    if (!GT_INT_TAG(a) && !GT_INT_TAG(b)) {
        long long x = gt_small_val(a), y = gt_small_val(b);
        return x < y ? -1 : (x > y ? 1 : 0);
    }
    GtBig *A = gt_int_to_big(a), *B = gt_int_to_big(b);
    if (A->n == 0 && B->n == 0) return 0;
    if (A->neg != B->neg) return A->neg ? -1 : 1;
    int c = gt_big_cmp_abs(A, B);
    return A->neg ? -c : c;
}
long long gt_int_eq(long long a, long long b) { return gt_int_cmp(a, b) == 0 ? 1 : 0; }

/* tagged int → C long long（溢出则饱和？此处仅用于转换；返回 i64 视角的值） */
long long gt_int_to_i64(long long v) {
    if (!GT_INT_TAG(v)) return gt_small_val(v);
    GtBig *b = GT_BIGP(v);
    unsigned long long a = 0;
    if (b->n >= 1) a |= (unsigned long long)b->limb[0];
    if (b->n >= 2) a |= ((unsigned long long)b->limb[1]) << 32;
    return b->neg ? -(long long)a : (long long)a;
}
/* C long long → tagged int */
long long gt_i64_to_int(long long v) {
    if (v <= 0x3FFFFFFFFFFFFFFFLL && v >= -0x4000000000000000LL) return GT_SMALL(v);
    return GT_FROM_BIG(gt_big_from_i64(v));
}

/* ============================================================
 * 高精度十进制（decimal）
 *
 * 表示（8 字节指针，指向 GtDec）：
 *   sign: 0=正 1=负
 *   scale: 小数点后位数（>=0）
 *   n: 有效数字个数（digits）
 *   d: 十进制数字数组（0..9，高位在前，不含前导 0；0 表示为 n=0）
 * 值为 (sign?-1:1) * (digits 组成的整数) / 10^scale。
 * 运算按十进制；除法保留足够位数（DEC_DIV_PREC）。
 * ============================================================ */
#define DEC_DIV_PREC 40
typedef struct GtDec {
    int sign;
    long long scale;
    long long n;
    unsigned char *d; /* 每字节一个 0..9 */
} GtDec;

static GtDec *gt_dec_alloc(long long n) {
    GtDec *x = (GtDec *)gt_alloc(sizeof(GtDec));
    x->sign = 0; x->scale = 0; x->n = n;
    x->d = n > 0 ? (unsigned char *)gt_alloc((size_t)n) : 0;
    for (long long i = 0; i < n; i++) x->d[i] = 0;
    return x;
}
static void gt_dec_trim(GtDec *x) {
    long long lead = 0;
    while (lead < x->n && x->d[lead] == 0) lead++;
    if (lead > 0) {
        for (long long i = lead; i < x->n; i++) x->d[i - lead] = x->d[i];
        x->n -= lead;
    }
    if (x->n == 0) x->sign = 0;
}
/* 复制 */
static GtDec *gt_dec_copy(const GtDec *a) {
    GtDec *r = gt_dec_alloc(a->n);
    r->sign = a->sign; r->scale = a->scale;
    for (long long i = 0; i < a->n; i++) r->d[i] = a->d[i];
    return r;
}
/* 按 scale 对齐：返回按 max_scale 扩展后的新数（尾随补 0） */
static GtDec *gt_dec_align(const GtDec *a, long long max_scale) {
    long long extra = max_scale - a->scale;
    GtDec *r = gt_dec_alloc(a->n + extra);
    r->sign = a->sign; r->scale = max_scale;
    for (long long i = 0; i < a->n; i++) r->d[i] = a->d[i];
    for (long long i = 0; i < extra; i++) r->d[a->n + i] = 0;
    return r;
}
/* 十进制字符串解析（可带符号、小数点、科学计数法 E/e） */
GtDec *gt_dec_from_str(const char *s) {
    if (!s) return gt_dec_alloc(0);
    int neg = 0; const char *p = s;
    while (*p == ' ' || *p == '\t') p++;
    if (*p == '+' || *p == '-') { neg = (*p == '-'); p++; }
    /* 收集数字与小数点位置 */
    char buf[4096]; long long bn = 0; long long dot = -1; int seen = 0;
    while (*p && (isdigit((unsigned char)*p) || *p == '.')) {
        if (*p == '.') { if (dot >= 0) break; dot = bn; }
        else { if (bn < (long long)sizeof(buf)) buf[bn++] = *p; }
        p++; seen = 1;
    }
    (void)seen;
    int exp = 0;
    if (*p == 'e' || *p == 'E') { p++; exp = atoi(p); }
    long long scale = 0;
    if (dot >= 0) scale = bn - dot;
    /* 应用指数 */
    scale -= exp;
    if (scale < 0) { /* 补 0 到整数位 */ long long add = -scale; for (long long i = 0; i < add && bn < (long long)sizeof(buf); i++) buf[bn++] = '0'; scale = 0; }
    GtDec *x = gt_dec_alloc(bn);
    for (long long i = 0; i < bn; i++) x->d[i] = (unsigned char)(buf[i] - '0');
    x->scale = scale;
    x->sign = neg ? 1 : 0;
    /* 去掉多余小数尾 0 */
    while (x->scale > 0 && x->n > 0 && x->d[x->n - 1] == 0) { x->n--; x->scale--; }
    gt_dec_trim(x);
    return x;
}
/* 从 i64 */
GtDec *gt_dec_from_i64(long long v) {
    char tmp[32]; snprintf(tmp, sizeof tmp, "%lld", v);
    return gt_dec_from_str(tmp);
}
/* 从 double（按 %.17g 精确回读） */
GtDec *gt_dec_from_f64(double v) {
    char tmp[64]; snprintf(tmp, sizeof tmp, "%.17g", v);
    return gt_dec_from_str(tmp);
}
/* 转字符串（调用方缓冲） */
long long gt_dec_fmt(const GtDec *x, char *buf, long long cap) {
    long long o = 0;
    if (x->sign && !(x->n == 0)) { if (o < cap - 1) buf[o++] = '-'; }
    if (x->n == 0) { if (o < cap - 1) buf[o++] = '0'; buf[o] = 0; return o; }
    if (x->scale == 0) {
        for (long long i = 0; i < x->n && o < cap - 1; i++) buf[o++] = (char)('0' + x->d[i]);
        buf[o] = 0; return o;
    }
    /* 整数部分位数 = n - scale（可能 <=0 → 0.xxx） */
    long long intpart = x->n - x->scale;
    if (intpart <= 0) {
        if (o < cap - 1) buf[o++] = '0';
        if (o < cap - 1) buf[o++] = '.';
        for (long long i = 0; i < -intpart && o < cap - 1; i++) buf[o++] = '0';
        for (long long i = 0; i < x->n && o < cap - 1; i++) buf[o++] = (char)('0' + x->d[i]);
    } else {
        for (long long i = 0; i < intpart && o < cap - 1; i++) buf[o++] = (char)('0' + x->d[i]);
        if (o < cap - 1) buf[o++] = '.';
        for (long long i = intpart; i < x->n && o < cap - 1; i++) buf[o++] = (char)('0' + x->d[i]);
    }
    buf[o] = 0; return o;
}
/* 绝对大小比较：|a| vs |b| */
static int gt_dec_cmp_abs(const GtDec *a, const GtDec *b) {
    long long ia = a->n - a->scale, ib = b->n - b->scale;
    if (ia != ib) return ia > ib ? 1 : -1;
    long long m = a->n > b->n ? a->n : b->n;
    for (long long i = 0; i < m; i++) {
        unsigned char da = i < a->n ? a->d[i] : 0;
        unsigned char db = i < b->n ? b->d[i] : 0;
        if (da != db) return da > db ? 1 : -1;
    }
    return 0;
}
/* 有符号比较 */
long long gt_dec_cmp(const GtDec *a, const GtDec *b) {
    if (a->n == 0 && b->n == 0) return 0;
    if (a->sign != b->sign) return a->sign ? -1 : 1;
    int c = gt_dec_cmp_abs(a, b);
    return a->sign ? -c : c;
}
/* 加绝对值（同 scale 对齐后逐位） */
static GtDec *gt_dec_add_abs(const GtDec *a, const GtDec *b, long long scale) {
    long long m = a->n > b->n ? a->n : b->n;
    GtDec *r = gt_dec_alloc(m + 1);
    r->scale = scale;
    int carry = 0;
    for (long long i = 0; i < m; i++) {
        long long ia = a->n - 1 - i, ib = b->n - 1 - i;
        int da = ia >= 0 ? a->d[ia] : 0, db = ib >= 0 ? b->d[ib] : 0;
        int s = da + db + carry;
        r->d[m - i] = (unsigned char)(s % 10); carry = s / 10;
    }
    r->d[0] = (unsigned char)carry;
    gt_dec_trim(r);
    return r;
}
/* 减绝对值（要求 |a|>=|b|） */
static GtDec *gt_dec_sub_abs(const GtDec *a, const GtDec *b, long long scale) {
    GtDec *r = gt_dec_alloc(a->n);
    r->scale = scale;
    int borrow = 0;
    for (long long i = 0; i < a->n; i++) {
        long long ia = a->n - 1 - i, ib = b->n - 1 - i;
        int da = a->d[ia];
        int db = ib >= 0 ? b->d[ib] : 0;
        int s = da - db - borrow;
        if (s < 0) { s += 10; borrow = 1; } else borrow = 0;
        r->d[a->n - 1 - i] = (unsigned char)s;
    }
    gt_dec_trim(r);
    return r;
}
/* 对齐 scale 后相加 */
GtDec *gt_dec_add(const GtDec *a, const GtDec *b) {
    long long sc = a->scale > b->scale ? a->scale : b->scale;
    GtDec *A = gt_dec_align(a, sc), *B = gt_dec_align(b, sc);
    GtDec *r;
    if (A->sign == B->sign) { r = gt_dec_add_abs(A, B, sc); r->sign = A->sign; }
    else {
        int c = gt_dec_cmp_abs(A, B);
        if (c == 0) r = gt_dec_alloc(0);
        else if (c > 0) { r = gt_dec_sub_abs(A, B, sc); r->sign = A->sign; }
        else { r = gt_dec_sub_abs(B, A, sc); r->sign = B->sign; }
    }
    gt_dec_trim(r);
    return r;
}
GtDec *gt_dec_sub(const GtDec *a, const GtDec *b) {
    GtDec *nb = gt_dec_copy(b); nb->sign = nb->n ? !nb->sign : 0;
    return gt_dec_add(a, nb);
}
GtDec *gt_dec_neg(const GtDec *a) { GtDec *r = gt_dec_copy(a); if (r->n) r->sign = !r->sign; return r; }
/* 乘法：逐位卷积（十进制），结果 scale = a.scale + b.scale */
GtDec *gt_dec_mul(const GtDec *a, const GtDec *b) {
    if (a->n == 0 || b->n == 0) return gt_dec_alloc(0);
    long long n = a->n + b->n;
    int *acc = (int *)gt_alloc(sizeof(int) * (size_t)n);
    for (long long i = 0; i < n; i++) acc[i] = 0;
    for (long long i = 0; i < a->n; i++)
        for (long long j = 0; j < b->n; j++)
            acc[i + j + 1] += a->d[i] * b->d[j];
    for (long long i = n - 1; i > 0; i--) { acc[i - 1] += acc[i] / 10; acc[i] %= 10; }
    GtDec *r = gt_dec_alloc(n);
    r->scale = a->scale + b->scale;
    r->sign = (a->sign != b->sign);
    for (long long i = 0; i < n; i++) r->d[i] = (unsigned char)acc[i];
    gt_free(acc);
    gt_dec_trim(r);
    return r;
}
/* 除法：保留 DEC_DIV_PREC 位小数（结果 scale = a.scale + prec） */
GtDec *gt_dec_div(const GtDec *a, const GtDec *b) {
    if (b->n == 0) { return gt_dec_alloc(0); }
    if (a->n == 0) return gt_dec_alloc(0);
    /* 转为整数长除：把 a/b 扩展 prec 位 */
    long long prec = DEC_DIV_PREC;
    /* 被除数 = a.digits 后补 (prec + b.scale - a.scale) 个 0（确保精度） */
    long long extra = prec + b->scale - a->scale + 1;
    if (extra < 0) extra = 0;
    long long dn = a->n + extra;
    unsigned char *divid = (unsigned char *)gt_alloc((size_t)dn);
    for (long long i = 0; i < a->n; i++) divid[i] = a->d[i];
    for (long long i = 0; i < extra; i++) divid[a->n + i] = 0;
    long long rn = b->n;
    unsigned char *divis = (unsigned char *)gt_alloc((size_t)rn);
    for (long long i = 0; i < rn; i++) divis[i] = b->d[i];
    /* 长除 */
    unsigned char *q = (unsigned char *)gt_alloc((size_t)dn);
    unsigned char *rem = (unsigned char *)gt_alloc((size_t)(rn + 2));
    long long remn = 0;
    for (long long i = 0; i < dn; i++) {
        /* rem = rem*10 + divid[i] */
        if (remn == 0 && divid[i] == 0) { q[i] = 0; continue; }
        rem[remn++] = divid[i];
        while (remn > 1 && rem[0] == 0) { for (long long k = 0; k < remn - 1; k++) rem[k] = rem[k+1]; remn--; }
        int qd = 0;
        while (remn > rn || (remn == rn && ({ int cc = 0; for (long long k = 0; k < rn; k++) { if (rem[k] != divis[k]) { cc = rem[k] > divis[k] ? 1 : -1; break; } } cc; }) >= 0)) {
            int borrow = 0;
            for (long long k = 0; k < remn; k++) {
                long long ri = remn - 1 - k, di = rn - 1 - k;
                int dv = di >= 0 ? divis[di] : 0;
                int s = rem[ri] - dv - borrow;
                if (s < 0) { s += 10; borrow = 1; } else borrow = 0;
                rem[ri] = (unsigned char)s;
            }
            while (remn > 1 && rem[0] == 0) { for (long long k = 0; k < remn - 1; k++) rem[k] = rem[k+1]; remn--; }
            qd++;
        }
        q[i] = (unsigned char)qd;
    }
    GtDec *r = gt_dec_alloc(dn);
    for (long long i = 0; i < dn; i++) r->d[i] = q[i];
    r->scale = a->scale + extra - b->scale;
    if (r->scale < 0) r->scale = 0;
    r->sign = (a->sign != b->sign);
    gt_dec_trim(r);
    /* 去掉多余小数尾 0 */
    while (r->scale > 0 && r->n > 0 && r->d[r->n - 1] == 0) { r->n--; r->scale--; }
    gt_free(divid); gt_free(divis); gt_free(q); gt_free(rem);
    return r;
}

/* decimal → i64（截断向零）/ f64 */
long long gt_dec_to_i64(GtDec *x) {
    /* 取整数部分 */
    long long intpart = x->n - x->scale;
    unsigned long long v = 0;
    if (intpart > 0) {
        for (long long i = 0; i < intpart; i++) {
            if (v > (0x7FFFFFFFFFFFFFFFULL - 9) / 10) { v = 0x7FFFFFFFFFFFFFFFULL; break; }
            v = v * 10 + x->d[i];
        }
    }
    return x->sign ? -(long long)v : (long long)v;
}
double gt_dec_to_f64(GtDec *x) {
    char buf[8192]; gt_dec_fmt(x, buf, (long long)sizeof(buf));
    return atof(buf);
}
/* decimal 打印到 stdout */
void gt_dec_print(GtDec *x) {
    char buf[8192];
    long long n = gt_dec_fmt(x, buf, (long long)sizeof(buf));
    if (n > 0) gt_write(buf, n);
}
/* decimal → 字符串（返回新分配的 NUL 结尾串） */
char *gt_dec_to_str(GtDec *x) {
    char buf[8192];
    long long n = gt_dec_fmt(x, buf, (long long)sizeof(buf));
    char *p = (char *)gt_alloc((size_t)n + 1);
    for (long long i = 0; i < n; i++) p[i] = buf[i];
    p[n] = 0;
    return p;
}

/* ---------- 数值内置 ---------- */
long long gt_abs_i(long long v) { return v < 0 ? -v : v; }
double gt_abs_f(double v) { return v < 0 ? -v : v; }
long long gt_min_i(long long a, long long b) { return a < b ? a : b; }
long long gt_max_i(long long a, long long b) { return a > b ? a : b; }
double gt_min_f(double a, double b) { return a < b ? a : b; }
double gt_max_f(double a, double b) { return a > b ? a : b; }


/* ============================================================
 * 字符串内置（返回新分配的 NUL 结尾字符串；程序结束前不释放）
 * 与解释器 jit.rs 的 gt_str_* 语义一致。
 * ============================================================ */
static char *gt_str_dup(const char *s) {
    if (s == NULL) return NULL;
    size_t n = strlen(s);
    char *p = (char *)gt_alloc(n + 1);
    memcpy(p, s, n + 1);
    return p;
}

/* substr(s, start, len)：按字节；越界钳制 */
char *gt_str_substr(const char *s, long long start, long long len) {
    if (s == NULL) return gt_str_dup("");
    long long n = (long long)strlen(s);
    if (start < 0) start = 0;
    if (start > n) start = n;
    if (len < 0) len = 0;
    if (start + len > n) len = n - start;
    char *p = (char *)gt_alloc((size_t)len + 1);
    memcpy(p, s + start, (size_t)len);
    p[len] = 0;
    return p;
}

/* find(s, sub)：返回字节下标，找不到 -1 */
long long gt_str_find(const char *s, const char *sub) {
    if (s == NULL || sub == NULL) return -1;
    const char *p = strstr(s, sub);
    return p == NULL ? -1 : (long long)(p - s);
}

/* upper/lower：仅 ASCII */
char *gt_str_upper(const char *s) {
    char *p = gt_str_dup(s ? s : "");
    for (char *q = p; *q; q++) if (*q >= 'a' && *q <= 'z') *q -= 32;
    return p;
}
char *gt_str_lower(const char *s) {
    char *p = gt_str_dup(s ? s : "");
    for (char *q = p; *q; q++) if (*q >= 'A' && *q <= 'Z') *q += 32;
    return p;
}

/* trim：去掉首尾空白（空格/制表/换行/回车） */
char *gt_str_trim(const char *s) {
    if (s == NULL) return gt_str_dup("");
    const char *a = s;
    while (*a == ' ' || *a == '\t' || *a == '\n' || *a == '\r') a++;
    const char *b = s + strlen(s);
    while (b > a && (b[-1] == ' ' || b[-1] == '\t' || b[-1] == '\n' || b[-1] == '\r')) b--;
    long long len = (long long)(b - a);
    char *p = (char *)gt_alloc((size_t)len + 1);
    memcpy(p, a, (size_t)len);
    p[len] = 0;
    return p;
}

/* repeat(s, n) */
char *gt_str_repeat(const char *s, long long n) {
    if (s == NULL || n <= 0) return gt_str_dup("");
    size_t sl = strlen(s);
    char *p = (char *)gt_alloc(sl * (size_t)n + 1);
    char *q = p;
    for (long long i = 0; i < n; i++) { memcpy(q, s, sl); q += sl; }
    *q = 0;
    return p;
}

/* replace(s, from, to)：替换所有出现 */
char *gt_str_replace(const char *s, const char *from, const char *to) {
    if (s == NULL) return gt_str_dup("");
    if (from == NULL || from[0] == 0) return gt_str_dup(s);
    if (to == NULL) to = "";
    size_t fl = strlen(from), tl = strlen(to);
    size_t cap = strlen(s) + 1;
    char *out = (char *)gt_alloc(cap);
    size_t used = 0;
    const char *p = s;
    while (*p) {
        if (strncmp(p, from, fl) == 0) {
            if (used + tl + 1 > cap) { cap = (used + tl + 1) * 2; out = (char *)gt_realloc(out, cap); }
            memcpy(out + used, to, tl); used += tl; p += fl;
        } else {
            if (used + 2 > cap) { cap = cap * 2; out = (char *)gt_realloc(out, cap); }
            out[used++] = *p++;
        }
    }
    out[used] = 0;
    return out;
}

/* split(s, sep) → list<str>（元素是指针，存 i64 槽） */
GtList *gt_str_split(const char *s, const char *sep) {
    GtList *l = gt_list_new(0);
    if (s == NULL) return l;
    if (sep == NULL || sep[0] == 0) {
        /* 分隔符为空：整串作为单元素 */
        gt_list_push(l, (long long)gt_str_dup(s));
        return l;
    }
    size_t sl = strlen(sep);
    const char *p = s;
    const char *q;
    while ((q = strstr(p, sep)) != NULL) {
        long long len = (long long)(q - p);
        char *part = (char *)gt_alloc((size_t)len + 1);
        memcpy(part, p, (size_t)len); part[len] = 0;
        gt_list_push(l, (long long)part);
        p = q + sl;
    }
    gt_list_push(l, (long long)gt_str_dup(p));
    return l;
}

/* join(list, sep) → str */
char *gt_str_join(GtList *l, const char *sep) {
    if (sep == NULL) sep = "";
    size_t sepl = strlen(sep);
    size_t cap = 64, used = 0;
    char *out = (char *)gt_alloc(cap);
    out[0] = 0;
    if (l != NULL) {
        for (long long i = 0; i < l->len; i++) {
            const char *e = (const char *)l->data[i];
            if (e == NULL) e = "";
            size_t el = strlen(e);
            if (used + el + sepl + 1 > cap) {
                while (used + el + sepl + 1 > cap) cap *= 2;
                out = (char *)gt_realloc(out, cap);
            }
            if (i > 0) { memcpy(out + used, sep, sepl); used += sepl; }
            memcpy(out + used, e, el); used += el;
        }
    }
    out[used] = 0;
    return out;
}

/* sum(list)：遍历 GtList（元素 i64 或 f64 位模式） */
long long gt_sum_i(GtList *l) {
    if (l == NULL) return 0;
    long long s = 0;
    for (long long i = 0; i < l->len; i++) s += l->data[i];
    return s;
}
double gt_sum_f(GtList *l) {
    if (l == NULL) return 0.0;
    double s = 0.0;
    for (long long i = 0; i < l->len; i++) {
        double v;
        memcpy(&v, &l->data[i], sizeof(double));
        s += v;
    }
    return s;
}


/* ============================================================
 * 裸内存操作（mem_*）：显式指针，与解释器 jit.rs 的 rt_mem_* 一致。
 * 指针一律是字节地址（GTLang 侧为 i64）；越界/空指针由调用方负责。
 * ============================================================ */
void *gt_mem_alloc(long long n) {
    if (n <= 0) n = 1;
    return gt_alloc((size_t)n);
}
void gt_mem_free(void *p) { gt_free(p); }

void gt_mem_store_i64(void *p, long long off, long long v) {
    memcpy((char *)p + off, &v, sizeof(long long));
}
long long gt_mem_load_i64(void *p, long long off) {
    long long v = 0;
    memcpy(&v, (char *)p + off, sizeof(long long));
    return v;
}
void gt_mem_store_u8(void *p, long long off, long long v) {
    ((unsigned char *)p)[off] = (unsigned char)v;
}
long long gt_mem_load_u8(void *p, long long off) {
    return (long long)((unsigned char *)p)[off];
}
void gt_mem_copy(void *dst, void *src, long long n) {
    memcpy(dst, src, (size_t)n);
}
void gt_mem_set(void *p, long long byte, long long n) {
    memset(p, (int)byte, (size_t)n);
}

/* range(a, b)：生成 [a, b) 的整数列表。 */
GtList *gt_range(long long a, long long b) {
    GtList *l = gt_list_new(0);
    for (long long i = a; i < b; i++) gt_list_push(l, i);
    return l;
}


/* assert(cond, msg, line)：cond 为 0 时打印诊断并终止。 */
void gt_assert(long long cond, const char *msg, long long line) {
    if (cond) return;
    char buf[512];
    int n = GT_ZH
        ? snprintf(buf, sizeof buf, "\n[断言失败] 第 %lld 行：%s\n", line, msg ? msg : "断言条件为假")
        : snprintf(buf, sizeof buf, "\n[assertion failed] line %lld: %s\n", line, msg ? msg : "assertion failed");
    if (n > 0) gt_write(buf, n);
    exit(1);
}


/* pad_left(s, width, fill)：左补至 width。fill 为 0 时用空格。 */
char *gt_pad_left(const char *s, long long width, const char *fill) {
    long long n = (long long)strlen(s);
    if (n >= width) return gt_str_dup(s);
    const char *f = (fill && *fill) ? fill : " ";
    long long flen = (long long)strlen(f);
    char *out = (char *)gt_alloc((width + 1) * sizeof(char));
    long long i = 0;
    while (i + flen <= width - n) { memcpy(out + i, f, flen); i += flen; }
    while (i < width - n) out[i++] = ' ';
    memcpy(out + i, s, n + 1);
    return out;
}

char *gt_pad_right(const char *s, long long width, const char *fill) {
    long long n = (long long)strlen(s);
    if (n >= width) return gt_str_dup(s);
    const char *f = (fill && *fill) ? fill : " ";
    long long flen = (long long)strlen(f);
    char *out = (char *)gt_alloc((width + 1) * sizeof(char));
    memcpy(out, s, n);
    long long i = n;
    while (i + flen <= width) { memcpy(out + i, f, flen); i += flen; }
    while (i < width) out[i++] = ' ';
    out[width] = 0;
    return out;
}

char *gt_fmt_int(long long x, long long width) {
    char buf[32];
    snprintf(buf, sizeof buf, "%lld", x);
    return gt_pad_left(buf, width, " ");
}



/* ============================================================
 * StringBuilder：高性能字符串拼接（避免 str + str 的 O(n²)）。
 * 句柄是"下标 + 1"；缓冲用动态数组，末尾自动维护 NUL。
 * ============================================================ */
typedef struct { char *buf; long long len; long long cap; } GtSb;
#define GT_SB_MAX 256
static GtSb *GT_SBS[GT_SB_MAX];
static long long GT_SB_NEXT = 1;

static GtSb *gt_sb_get(long long h) {
    if (h <= 0 || h >= GT_SB_NEXT || h >= GT_SB_MAX) return NULL;
    return GT_SBS[h];
}

long long gt_sb_new(void) {
    if (GT_SB_NEXT >= GT_SB_MAX) return 0;
    GtSb *s = (GtSb *)gt_alloc(sizeof(GtSb));
    s->cap = 64; s->len = 0;
    s->buf = (char *)gt_alloc((size_t)s->cap);
    s->buf[0] = 0;
    GT_SBS[GT_SB_NEXT] = s;
    return GT_SB_NEXT++;
}

static void gt_sb_reserve(GtSb *s, long long extra) {
    if (s->len + extra + 1 <= s->cap) return;
    while (s->len + extra + 1 > s->cap) s->cap *= 2;
    s->buf = (char *)gt_realloc(s->buf, (size_t)s->cap);
}

void gt_sb_push_str(long long h, const char *p);   /* 前置声明 */

/* gt_sb_push_char：按字符追加（c 是 ASCII 码） */
void gt_sb_pop(long long h) {
    GtSb *s = gt_sb_get(h);
    if (s && s->len > 0) { s->len--; s->buf[s->len] = 0; }
}

void gt_sb_push_char(long long h, long long c) {
    char tmp[2]; tmp[0] = (char)(c & 0xFF); tmp[1] = 0;
    gt_sb_push_str(h, tmp);
}

void gt_sb_push_str(long long h, const char *p) {
    GtSb *s = gt_sb_get(h);
    if (!s || !p) return;
    long long n = (long long)strlen(p);
    gt_sb_reserve(s, n);
    memcpy(s->buf + s->len, p, (size_t)n);
    s->len += n; s->buf[s->len] = 0;
}

void gt_sb_push_i64(long long h, long long v) {
    char tmp[32];
    snprintf(tmp, sizeof tmp, "%lld", v);
    gt_sb_push_str(h, tmp);
}

void gt_sb_push_f64(long long h, double v) {
    char tmp[64];
    snprintf(tmp, sizeof tmp, "%g", v);
    gt_sb_push_str(h, tmp);
}

void gt_sb_push_bool(long long h, long long v) {
    gt_sb_push_str(h, v != 0 ? "true" : "false");
}

/* 收尾：把缓冲"移交"给调用方（返回 NUL 结尾字符串指针）。句柄失效。 */
char *gt_sb_finish(long long h) {
    GtSb *s = gt_sb_get(h);
    if (!s) return NULL;
    GT_SBS[h] = NULL;
    char *r = s->buf;
    gt_free(s);
    return r;
}

/* ============================================================
 * 并发：gt_thread_spawn(fn_ptr, args_ptr, nargs)
 *   新线程调用 fn(args_ptr[0..nargs])（参数为 i64）。
 * ============================================================ */
#ifdef _WIN32
#include <windows.h>
typedef struct { long long (*fn)(long long, long long, long long, long long); long long *args; long long n; } GtThreadArg;

/* ---------- 线程池：固定 N 个 worker + FIFO 任务队列 ----------
 * go 变成"入队"，避免每次 CreateThread 的开销（典型 10-100x）。
 * 队列满时回退为"直接起线程"，保证语义正确（不阻塞调用方）。 */
#define GT_POOL_MAX 256
static CRITICAL_SECTION gt_pool_lock;
static CONDITION_VARIABLE gt_pool_cv;
static int gt_pool_init_done = 0;
static GtThreadArg *gt_pool_q[GT_POOL_MAX];
static int gt_pool_head = 0, gt_pool_tail = 0, gt_pool_len = 0;
static int gt_pool_nworkers = 0;

static void gt_pool_init(void) {
    if (gt_pool_init_done) return;
    InitializeCriticalSection(&gt_pool_lock);
    InitializeConditionVariable(&gt_pool_cv);
    SYSTEM_INFO si; GetSystemInfo(&si);
    int n = (int)si.dwNumberOfProcessors;
    if (n < 4) n = 4;
    if (n > 64) n = 64;
    gt_pool_nworkers = n;
    gt_pool_init_done = 1;
}

static void gt_thread_run_one(GtThreadArg *ta) {
    long long a0 = ta->n > 0 ? ta->args[0] : 0;
    long long a1 = ta->n > 1 ? ta->args[1] : 0;
    long long a2 = ta->n > 2 ? ta->args[2] : 0;
    long long a3 = ta->n > 3 ? ta->args[3] : 0;
    ta->fn(a0, a1, a2, a3);
    gt_free(ta);
}

static DWORD WINAPI gt_pool_worker(LPVOID unused) {
    (void)unused;
    for (;;) {
        EnterCriticalSection(&gt_pool_lock);
        while (gt_pool_len == 0) {
            SleepConditionVariableCS(&gt_pool_cv, &gt_pool_lock, INFINITE);
        }
        GtThreadArg *ta = gt_pool_q[gt_pool_head];
        gt_pool_head = (gt_pool_head + 1) % GT_POOL_MAX;
        gt_pool_len--;
        LeaveCriticalSection(&gt_pool_lock);
        gt_thread_run_one(ta);
    }
    return 0;
}

void gt_thread_spawn(long long fn, long long *args, long long n) {
    GtThreadArg *ta = (GtThreadArg *)gt_alloc(sizeof(GtThreadArg));
    ta->fn = (long long (*)(long long, long long, long long, long long))fn;
    ta->args = args;
    ta->n = n;
    gt_pool_init();
    EnterCriticalSection(&gt_pool_lock);
    if (gt_pool_len < GT_POOL_MAX) {
        gt_pool_q[gt_pool_tail] = ta;
        gt_pool_tail = (gt_pool_tail + 1) % GT_POOL_MAX;
        gt_pool_len++;
        /* 队列从 0 变 1：若 worker 尚未全部起来，补一个（懒启动，最多 N 个） */
        static int started = 0;
        int need_start = (gt_pool_len == 1) || (started < gt_pool_nworkers && gt_pool_len > started);
        if (started < gt_pool_nworkers) {
            HANDLE h = CreateThread(NULL, 0, gt_pool_worker, NULL, 0, NULL);
            if (h) { CloseHandle(h); started++; }
        }
        WakeConditionVariable(&gt_pool_cv);
        LeaveCriticalSection(&gt_pool_lock);
        (void)need_start;
        return;
    }
    LeaveCriticalSection(&gt_pool_lock);
    /* 队列满：回退为直接起线程（保证不阻塞、不丢任务） */
    HANDLE h = CreateThread(NULL, 0, (LPTHREAD_START_ROUTINE)gt_thread_run_one, ta, 0, NULL);
    if (h) CloseHandle(h);
}
#else
#include <pthread.h>
typedef struct { long long (*fn)(long long, long long, long long, long long); long long *args; long long n; } GtThreadArg;
static void *gt_thread_main(void *p) {
    GtThreadArg *ta = (GtThreadArg *)p;
    long long a0 = ta->n > 0 ? ta->args[0] : 0;
    long long a1 = ta->n > 1 ? ta->args[1] : 0;
    long long a2 = ta->n > 2 ? ta->args[2] : 0;
    long long a3 = ta->n > 3 ? ta->args[3] : 0;
    ta->fn(a0, a1, a2, a3);
    gt_free(ta);
    return NULL;
}
void gt_thread_spawn(long long fn, long long *args, long long n) {
    GtThreadArg *ta = (GtThreadArg *)gt_alloc(sizeof(GtThreadArg));
    ta->fn = (long long (*)(long long, long long, long long, long long))fn;
    ta->args = args;
    ta->n = n;
    pthread_t t;
    pthread_create(&t, NULL, gt_thread_main, ta);
    pthread_detach(t);
}
#endif


void gt_sleep(long long ms) {
#ifdef _WIN32
    Sleep((DWORD)ms);
#else
    struct timespec ts;
    ts.tv_sec = ms / 1000;
    ts.tv_nsec = (ms % 1000) * 1000000;
    nanosleep(&ts, NULL);
#endif
}


/* ============================================================
 * 通道：转发到 GTO（GTLang Thread Orchestrator）。
 *   chan() 无界 → gto_chan_new(0)。GTO 以独立 gto.dll 分发，
 *   运行期 LoadLibrary + GetProcAddress（与标准库同款"零 .lib"）。
 *   go 仍为真线程（见上），GTO 只承担通道/同步原语。
 * ============================================================ */
typedef void *(*gto_chan_new_t)(long long);
typedef void (*gto_chan_send_t)(void *, long long);
typedef long long (*gto_chan_recv_t)(void *);
static gto_chan_new_t   g_gtoc_new  = 0;
static gto_chan_send_t  g_gtoc_send = 0;
static gto_chan_recv_t  g_gtoc_recv = 0;
static void *g_gto_mod = 0;

static void gt_gto_init(void) {
    if (g_gto_mod) return;
    /* 复用与 shim 相同的查找策略：GTC_STDLIB_DIR、exe 同目录、res\lib、逐级向上。 */
    char base[MAX_PATH]; DWORD n = GetModuleFileNameA(NULL, base, MAX_PATH);
    for (DWORD i = n; i > 0; --i) { if (base[i-1] == '\\' || base[i-1] == '/') { base[i] = 0; break; } }
    char envdir[MAX_PATH]; DWORD en = GetEnvironmentVariableA("GTC_STDLIB_DIR", envdir, MAX_PATH);
    if (en > 0 && en < MAX_PATH) {
        char p[MAX_PATH]; strcpy(p, envdir);
        if (p[strlen(p)-1] != '\\') strcat(p, "\\");
        strcat(p, "gto.dll");
        g_gto_mod = LoadLibraryA(p);
    }
    if (!g_gto_mod) {
        char root[MAX_PATH]; strcpy(root, base);
        for (int up = 0; up < 8 && !g_gto_mod; ++up) {
            char p[MAX_PATH]; strcpy(p, root); strcat(p, "gto.dll"); g_gto_mod = LoadLibraryA(p);
            if (!g_gto_mod) { strcpy(p, root); strcat(p, "res\\lib\\gto.dll"); g_gto_mod = LoadLibraryA(p); }
            if (!g_gto_mod) { int L = (int)strlen(root); if (L > 0) { root[L-1] = 0; char *sl = strrchr(root, '\\'); if (sl) sl[1] = 0; else root[0] = 0; } }
        }
    }
    if (!g_gto_mod) g_gto_mod = LoadLibraryA("gto.dll");
    if (g_gto_mod) {
        g_gtoc_new  = (gto_chan_new_t)  (void *)GetProcAddress(g_gto_mod, "gto_chan_new");
        g_gtoc_send = (gto_chan_send_t) (void *)GetProcAddress(g_gto_mod, "gto_chan_send");
        g_gtoc_recv = (gto_chan_recv_t) (void *)GetProcAddress(g_gto_mod, "gto_chan_recv");
    }
}

void *gt_chan_new(void) {
    gt_gto_init();
    if (!g_gtoc_new) return 0;
    return g_gtoc_new(0); /* 0 = 无界 */
}
void gt_chan_send(void *p, long long v) {
    gt_gto_init();
    if (g_gtoc_send) g_gtoc_send(p, v);
}
long long gt_chan_recv(void *p) {
    gt_gto_init();
    if (g_gtoc_recv) return g_gtoc_recv(p);
    return -1;
}


/* ============================================================
 * 引用计数（RC，非 STW）：
 *   - 所有堆对象（list/set/map/...）首字段为 rc
 *   - gt_rc_inc/dec 操作首 8 字节
 *   - dec 到 0：调用 free_fn 释放（容器释放内部缓冲）
 * ============================================================ */
void gt_rc_inc(void *p) {
    if (p) { (*(long long *)p)++; }
}
long long gt_rc_dec(void *p, void (*free_fn)(void *)) {
    if (!p) return 0;
    long long *rc = (long long *)p;
    if (--(*rc) == 0) {
        if (free_fn) free_fn(p);
        else gt_free(p);
        return 0;
    }
    return *rc;
}
/* 按对象类型分发 free（gc_dec 到 0 时用）。 */
static void gt_dispatch_free(void *p) {
    /* 用 gc 头的 kind 字段分派；字符串等无 kind 的由 gc_dec 默认路径处理 */
    extern long long gc_kind_of(void *p);
    long long k = gc_kind_of(p);
    if (k == 1) { gt_list_free(p); return; }
    if (k == 2) { gt_set_free(p); return; }
    if (k == 3) { gt_map_free(p); return; }
    /* 其他（字符串等）：gc_dec 已释放对象本身，这里不再处理 */
}

void gt_list_free(void *p) {
    GtList *l = (GtList *)p;
    if (l->elem_ptr && l->data) {
        for (long long i = 0; i < l->len; i++) {
            if (l->data[i]) gc_dec((void *)(size_t)l->data[i], gt_dispatch_free);
        }
    }
    if (l->data) gt_free(l->data);
    gt_free(l);
}

void gt_set_free(void *p) {
    GtSet *s = (GtSet *)p;
    if (s->elem_ptr && s->data) {
        for (long long i = 0; i < s->len; i++) {
            if (s->data[i]) gc_dec((void *)(size_t)s->data[i], gt_dispatch_free);
        }
    }
    if (s->data) gt_free(s->data);
    if (s->ht) gt_free(s->ht);
    gt_free(s);
}

void gt_map_free(void *p) {
    GtMap *m = (GtMap *)p;
    if (m->elem_ptr) {
        for (long long i = 0; i < m->len; i++) {
            if (m->keys[i]) gc_dec((void *)(size_t)m->keys[i], gt_dispatch_free);
            if (m->vals[i]) gc_dec((void *)(size_t)m->vals[i], gt_dispatch_free);
        }
    }
    if (m->keys) gt_free(m->keys);
    if (m->vals) gt_free(m->vals);
    if (m->ht) gt_free(m->ht);
    gt_free(m);
}

/* ============================================================
 * 标准输入：read_line / read_int
 *   - gt_read_line() 读一行（去换行），返回 NUL 结尾的堆字符串
 *   - gt_read_int()  跳过空白读一个整数，EOF 返回 0
 * ============================================================ */
char *gt_read_line(void) {
    char *buf = (char *)gt_alloc(4096);
    if (!buf) return NULL;
    if (!fgets(buf, 4096, stdin)) { buf[0] = '\0'; return buf; }
    size_t n = strlen(buf);
    while (n > 0 && (buf[n-1] == '\n' || buf[n-1] == '\r')) { buf[--n] = '\0'; }
    return buf;
}
long long gt_read_int(void) {
    long long v;
    if (scanf("%lld", &v) != 1) return 0;
    return v;
}

/* 取字符串第 i 个"字符"（UTF-8 码点），返回新分配的 NUL 结尾子串。
   i 超出返回空串。 */
char *gt_str_char_at(const char *s, long long i) {
    if (!s || i < 0) { char *e = (char *)gt_alloc(1); if (e) e[0] = 0; return e; }
    size_t len = strlen(s);
    size_t pos = 0;
    long long k = 0;
    while (pos < len) {
        unsigned char c = (unsigned char)s[pos];
        size_t clen = 1;
        if      ((c & 0x80) == 0x00) clen = 1;
        else if ((c & 0xE0) == 0xC0) clen = 2;
        else if ((c & 0xF0) == 0xE0) clen = 3;
        else if ((c & 0xF8) == 0xF0) clen = 4;
        if (k == i) {
            char *out = (char *)gt_alloc(clen + 1);
            if (!out) return NULL;
            memcpy(out, s + pos, clen);
            out[clen] = '\0';
            return out;
        }
        pos += clen;
        k++;
    }
    { char *e = (char *)gt_alloc(1); if (e) e[0] = 0; return e; }
}
long long gt_str_char_len(const char *s) {
    if (!s) return 0;
    size_t len = strlen(s);
    long long n = 0;
    size_t pos = 0;
    while (pos < len) {
        unsigned char c = (unsigned char)s[pos];
        size_t clen = 1;
        if      ((c & 0x80) == 0x00) clen = 1;
        else if ((c & 0xE0) == 0xC0) clen = 2;
        else if ((c & 0xF0) == 0xE0) clen = 3;
        else if ((c & 0xF8) == 0xF0) clen = 4;
        pos += clen;
        n++;
    }
    return n;
}

/* 按索引取集合元素 / 映射键（线性扫描，供 for 遍历） */
long long gt_set_at(GtSet *s, long long i) {
    if (!s || i < 0 || i >= s->len) return 0;
    return s->data[i];
}
long long gt_map_key_at(GtMap *m, long long i) {
    if (!m || i < 0 || i >= m->len) return 0;
    return m->keys[i];
}
long long gt_map_val_at(GtMap *m, long long i) {
    if (!m || i < 0 || i >= m->len) return 0;
    return m->vals[i];
}

/* 字符串拼接：返回新分配的 NUL 结尾字符串 */

/* ============================================================
 * 字符串构建器（str_builder）：避免循环里 s = s + x 的 O(n^2) 复制与泄漏
 * 用法：b = str_builder(); sb_append(b, x); ... s = sb_finish(b)
 * ============================================================ */
typedef struct { char *buf; size_t len; size_t cap; } GtStrBuilder;

long long str_builder(void) {
    GtStrBuilder *b = (GtStrBuilder *)gt_alloc(sizeof(GtStrBuilder));
    b->cap = 64; b->len = 0;
    b->buf = (char *)gt_alloc(b->cap);
    b->buf[0] = 0;
    return (long long)b;
}

static void sb_reserve(GtStrBuilder *b, size_t extra) {
    if (b->len + extra + 1 > b->cap) {
        while (b->len + extra + 1 > b->cap) b->cap *= 2;
        b->buf = (char *)gt_realloc(b->buf, b->cap);
    }
}

void sb_append(long long h, const char *s) {
    GtStrBuilder *b = (GtStrBuilder *)h;
    if (!b || !s) return;
    size_t n = strlen(s);
    sb_reserve(b, n);
    memcpy(b->buf + b->len, s, n);
    b->len += n;
    b->buf[b->len] = 0;
}

void sb_append_int(long long h, long long v) {
    char tmp[32];
    snprintf(tmp, sizeof(tmp), "%lld", v);
    sb_append(h, tmp);
}

const char *sb_finish(long long h) {
    GtStrBuilder *b = (GtStrBuilder *)h;
    if (!b) return "";
    char *r = b->buf;
    b->buf = NULL;
    gt_free(b);
    return r;
}
char *gt_str_concat(const char *a, const char *b) {
    if (!a) a = "";
    if (!b) b = "";
    size_t la = strlen(a), lb = strlen(b);
    char *out = (char *)gt_alloc(la + lb + 1);
    if (!out) return NULL;
    memcpy(out, a, la);
    memcpy(out + la, b, lb);
    out[la + lb] = '\0';
    return out;
}
/* sb_push_char(b, c)：按字符追加（c 是 ASCII 码） */
void sb_push_char(long long h, long long c) {
    char tmp[2]; tmp[0] = (char)(c & 0xFF); tmp[1] = 0;
    sb_append(h, tmp);
}

