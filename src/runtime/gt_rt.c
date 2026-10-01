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

/* 统一分配器：用 Windows 进程堆（HeapAlloc），而非 CRT 的 malloc。
 * 好处：进程堆是「整个进程共享」的，不同 CRT（libcmt / msvcrt）都能
 * 用 HeapFree 释放对方分配的内存 —— 跨 CRT 释放不再崩溃。
 * （GTLang 与标准库 DLL 处于不同 CRT，serve_fn 等回调需要这种可跨 CRT 释放的内存。） */
#define gt_alloc(n)   HeapAlloc(GetProcessHeap(), 0, (n))
#define gt_free(p)    HeapFree(GetProcessHeap(), 0, (p))

/* 自动内存管理（引用计数 + 预留环检测）。--no-gc 时退化为裸分配。
 * 直接包含实现（gc.c 内含 gc.h），使 gt_rt.c 自成编译单元；
 * driver.rs / build.bat 只需把 gc.c、gc.h 与 gt_rt.c 放在同一目录即可。 */
#include "gc.c"

/* 运行时消息语言：默认英文；编译器在用 `zh` 模式编译时会调用 gt_rt_set_zh() 切到中文。 */
static int GT_ZH = 0;
void gt_rt_set_zh(void) { GT_ZH = 1; }

/* 进程初始化：设控制台输出为 UTF-8（让 C 的 printf 也能正确显示中文）。 */
void gt_rt_init(void) {
    static int done = 0;
    if (done) return;
    done = 1;
    SetConsoleOutputCP(65001 /* CP_UTF8 */);
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

    /* 重定向 / 转换失败：原样写 UTF-8 字节 */
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

typedef struct { long long rc; long long *data; long long len; long long cap; } GtList;
typedef struct {
    long long rc;
    long long *data;   /* 插入顺序数组（for 遍历按此顺序） */
    long long len;
    long long cap;
    long long *ht;     /* 开放寻址哈希桶：存 data 下标，-1 表示空；hcap 为 2 的幂 */
    long long hcap;
} GtSet;
typedef struct {
    long long *keys;   /* 插入顺序数组（keys/vals/for 遍历按此顺序） */
    long long *vals;
    long long len;
    long long cap;
    long long *ht;     /* 开放寻址哈希桶：存 keys 下标，-1 表示空；大小 hcap 为 2 的幂 */
    long long hcap;
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
static void gt_map_rehash(GtMap *m, long long newcap) {
    gt_free(m->ht);
    m->hcap = newcap;
    m->ht = (long long *)gt_alloc(sizeof(long long) * (size_t)newcap);
    for (long long i = 0; i < newcap; i++) m->ht[i] = -1;
    for (long long i = 0; i < m->len; i++) {
        unsigned long long h = gt_hash64((unsigned long long)m->keys[i]) & (unsigned long long)(newcap - 1);
        while (m->ht[h] != -1) h = (h + 1) & (unsigned long long)(newcap - 1);
        m->ht[h] = i;
    }
}
static long long gt_map_find(GtMap *m, long long k) {
    if (m->hcap == 0) return -1;
    unsigned long long mask = (unsigned long long)(m->hcap - 1);
    unsigned long long h = gt_hash64((unsigned long long)k) & mask;
    while (m->ht[h] != -1) {
        long long idx = m->ht[h];
        if (m->keys[idx] == k) return idx;
        h = (h + 1) & mask;
    }
    return -1;
}

/* ---------- list ---------- */
GtList *gt_list_new(void) {
    GtList *l = (GtList *)gc_alloc(sizeof(GtList));
    l->rc = 1;
    l->cap = 4;
    l->len = 0;
    l->data = (long long *)gt_alloc(sizeof(long long) * (size_t)l->cap);
    return l;
}

static void gt_list_grow(GtList *l) {
    if (l->len >= l->cap) {
        l->cap *= 2;
        l->data = (long long *)realloc(l->data, sizeof(long long) * (size_t)l->cap);
    }
}

void gt_list_push(GtList *l, long long v) {
    gt_list_grow(l);
    l->data[l->len++] = v;
}

long long gt_list_pop(GtList *l) {
    if (l->len == 0) return 0;
    return l->data[--l->len];
}

long long gt_list_at(GtList *l, long long i) {
    if (i < 0 || i >= l->len) return 0;
    return l->data[i];
}

void gt_list_set(GtList *l, long long i, long long v) {
    if (i >= 0 && i < l->len) l->data[i] = v;
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

GtSet *gt_set_new(void) {
    GtSet *s = (GtSet *)gt_alloc(sizeof(GtSet));
    s->cap = 4;
    s->len = 0;
    s->data = (long long *)gt_alloc(sizeof(long long) * (size_t)s->cap);
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
        s->data = (long long *)realloc(s->data, sizeof(long long) * (size_t)s->cap);
    }
    long long idx = s->len;
    s->data[idx] = v;
    s->len++;
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
GtMap *gt_map_new(void) {
    GtMap *m = (GtMap *)gt_alloc(sizeof(GtMap));
    m->cap = 4;
    m->len = 0;
    m->keys = (long long *)gt_alloc(sizeof(long long) * (size_t)m->cap);
    m->vals = (long long *)gt_alloc(sizeof(long long) * (size_t)m->cap);
    m->hcap = 8;
    m->ht = (long long *)gt_alloc(sizeof(long long) * (size_t)m->hcap);
    for (long long i = 0; i < m->hcap; i++) m->ht[i] = -1;
    return m;
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
        m->keys = (long long *)realloc(m->keys, sizeof(long long) * (size_t)m->cap);
        m->vals = (long long *)realloc(m->vals, sizeof(long long) * (size_t)m->cap);
    }
    long long idx = m->len;
    m->keys[idx] = k;
    m->vals[idx] = v;
    m->len++;
    unsigned long long mask = (unsigned long long)(m->hcap - 1);
    unsigned long long h = gt_hash64((unsigned long long)k) & mask;
    while (m->ht[h] != -1) h = (h + 1) & mask;
    m->ht[h] = idx;
}

long long gt_map_get(GtMap *m, long long k) {
    long long i = gt_map_find(m, k);
    return i >= 0 ? m->vals[i] : 0;
}

long long gt_map_has(GtMap *m, long long k) {
    return gt_map_find(m, k) >= 0 ? 1 : 0;
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
    GtList *l = gt_list_new();
    for (long long i = 0; i < m->len; i++) gt_list_push(l, m->keys[i]);
    return l;
}

GtList *gt_map_values(GtMap *m) {
    GtList *l = gt_list_new();
    for (long long i = 0; i < m->len; i++) gt_list_push(l, m->vals[i]);
    return l;
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
            if (used + tl + 1 > cap) { cap = (used + tl + 1) * 2; out = (char *)realloc(out, cap); }
            memcpy(out + used, to, tl); used += tl; p += fl;
        } else {
            if (used + 2 > cap) { cap = cap * 2; out = (char *)realloc(out, cap); }
            out[used++] = *p++;
        }
    }
    out[used] = 0;
    return out;
}

/* split(s, sep) → list<str>（元素是指针，存 i64 槽） */
GtList *gt_str_split(const char *s, const char *sep) {
    GtList *l = gt_list_new();
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
                out = (char *)realloc(out, cap);
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
    GtList *l = gt_list_new();
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
 * 并发：gt_thread_spawn(fn_ptr, args_ptr, nargs)
 *   新线程调用 fn(args_ptr[0..nargs])（参数为 i64）。
 * ============================================================ */
#ifdef _WIN32
#include <windows.h>
typedef struct { long long (*fn)(long long, long long, long long, long long); long long *args; long long n; } GtThreadArg;
static DWORD WINAPI gt_thread_main(LPVOID p) {
    GtThreadArg *ta = (GtThreadArg *)p;
    long long a0 = ta->n > 0 ? ta->args[0] : 0;
    long long a1 = ta->n > 1 ? ta->args[1] : 0;
    long long a2 = ta->n > 2 ? ta->args[2] : 0;
    long long a3 = ta->n > 3 ? ta->args[3] : 0;
    ta->fn(a0, a1, a2, a3);
    gt_free(ta);
    return 0;
}
void gt_thread_spawn(long long fn, long long *args, long long n) {
    GtThreadArg *ta = (GtThreadArg *)gt_alloc(sizeof(GtThreadArg));
    ta->fn = (long long (*)(long long, long long, long long, long long))fn;
    ta->args = args;
    ta->n = n;
    HANDLE h = CreateThread(NULL, 0, gt_thread_main, ta, 0, NULL);
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
 * 通道：无界队列 + 互斥锁 + 条件变量
 * ============================================================ */
#ifdef _WIN32
typedef struct { CRITICAL_SECTION mu; CONDITION_VARIABLE cv; long long *buf; long long len, cap, head; } GtChan;
void *gt_chan_new(void) {
    GtChan *c = (GtChan *)gt_alloc(sizeof(GtChan));
    InitializeCriticalSection(&c->mu);
    InitializeConditionVariable(&c->cv);
    c->cap = 16; c->len = 0; c->head = 0;
    c->buf = (long long *)gt_alloc(sizeof(long long) * c->cap);
    return c;
}
void gt_chan_send(void *p, long long v) {
    GtChan *c = (GtChan *)p;
    EnterCriticalSection(&c->mu);
    if (c->len == c->cap) {
        c->cap *= 2;
        long long *nb = (long long *)gt_alloc(sizeof(long long) * c->cap);
        for (long long i = 0; i < c->len; i++) nb[i] = c->buf[(c->head + i) % (c->cap / 2)];
        gt_free(c->buf); c->buf = nb; c->head = 0;
    }
    c->buf[(c->head + c->len) % c->cap] = v;
    c->len++;
    WakeConditionVariable(&c->cv);
    LeaveCriticalSection(&c->mu);
}
long long gt_chan_recv(void *p) {
    GtChan *c = (GtChan *)p;
    EnterCriticalSection(&c->mu);
    while (c->len == 0) SleepConditionVariableCS(&c->cv, &c->mu, INFINITE);
    long long v = c->buf[c->head];
    c->head = (c->head + 1) % c->cap;
    c->len--;
    LeaveCriticalSection(&c->mu);
    return v;
}
#else
#include <pthread.h>
typedef struct { pthread_mutex_t mu; pthread_cond_t cv; long long *buf; long long len, cap, head; } GtChan;
void *gt_chan_new(void) {
    GtChan *c = (GtChan *)gt_alloc(sizeof(GtChan));
    pthread_mutex_init(&c->mu, NULL);
    pthread_cond_init(&c->cv, NULL);
    c->cap = 16; c->len = 0; c->head = 0;
    c->buf = (long long *)gt_alloc(sizeof(long long) * c->cap);
    return c;
}
void gt_chan_send(void *p, long long v) {
    GtChan *c = (GtChan *)p;
    pthread_mutex_lock(&c->mu);
    if (c->len == c->cap) {
        c->cap *= 2;
        long long *nb = (long long *)gt_alloc(sizeof(long long) * c->cap);
        for (long long i = 0; i < c->len; i++) nb[i] = c->buf[(c->head + i) % (c->cap / 2)];
        gt_free(c->buf); c->buf = nb; c->head = 0;
    }
    c->buf[(c->head + c->len) % c->cap] = v;
    c->len++;
    pthread_cond_signal(&c->cv);
    pthread_mutex_unlock(&c->mu);
}
long long gt_chan_recv(void *p) {
    GtChan *c = (GtChan *)p;
    pthread_mutex_lock(&c->mu);
    while (c->len == 0) pthread_cond_wait(&c->cv, &c->mu);
    long long v = c->buf[c->head];
    c->head = (c->head + 1) % c->cap;
    c->len--;
    pthread_mutex_unlock(&c->mu);
    return v;
}
#endif


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
void gt_list_free(void *p) {
    GtList *l = (GtList *)p;
    if (l->data) gt_free(l->data);
    gt_free(l);
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

/* 字符串拼接：返回新分配的 NUL 结尾字符串 */
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
