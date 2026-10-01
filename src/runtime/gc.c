#include "gc.h"
#include <windows.h>
#include <stdlib.h>
#include <string.h>

/* 统一分配器：Windows 进程堆（跨 CRT 安全） */
#define GC_HEAP   GetProcessHeap()
#define GC_ALLOC(n)  HeapAlloc(GC_HEAP, 0, (n))
#define GC_FREE(p)   HeapFree(GC_HEAP, 0, (p))

/* rc 头：8 字节计数 + 8 字节"环检测标记"（Bacon-Rajan 用） */
typedef struct GcHdr {
    volatile long long rc;     /* 引用计数（原子） */
    volatile long long mark;   /* 环检测：紫色标记（0=黑/未标记） */
    size_t size;
} GcHdr;

static int GC_ON = 1;
static volatile long long GC_LIVE = 0;      /* 存活对象数 */
static volatile long long GC_ALLOC_SINCE = 0; /* 自上次环检测以来的分配数 */
static const long long GC_CYCLE_THRESHOLD = 100000; /* 每分配这么多对象触发一次环检测 */

void gc_set_enabled(int on) { GC_ON = on; }
int  gc_enabled(void) { return GC_ON; }
long long gc_live_objects(void) { return GC_LIVE; }

void *gc_alloc(size_t n) {
    if (!GC_ON) { return GC_ALLOC(n); }
    GcHdr *h = (GcHdr *)GC_ALLOC(sizeof(GcHdr) + n);
    if (!h) return NULL;
    h->rc = 1;
    h->mark = 0;
    h->size = n;
    InterlockedIncrement64((volatile LONG64 *)&GC_LIVE);
    long long since = InterlockedIncrement64((volatile LONG64 *)&GC_ALLOC_SINCE);
    if (since >= GC_CYCLE_THRESHOLD) { InterlockedExchange64((volatile LONG64 *)&GC_ALLOC_SINCE, 0); gc_collect_cycles(); }
    return (void *)(h + 1);
}

static GcHdr *hdr_of(void *p) { return p ? ((GcHdr *)p - 1) : NULL; }

void gc_inc(void *p) {
    if (!GC_ON || !p) return;
    GcHdr *h = hdr_of(p);
    InterlockedIncrement64((volatile LONG64 *)&h->rc);
}

void gc_dec(void *p, void (*free_fn)(void *)) {
    if (!p) return;
    GcHdr *h = hdr_of(p);
    if (!GC_ON) { /* --no-gc：不释放，靠进程结束回收 */ return; }
    if (InterlockedDecrement64((volatile LONG64 *)&h->rc) == 0) {
        InterlockedDecrement64((volatile LONG64 *)&GC_LIVE);
        if (free_fn) free_fn(p);
        GC_FREE(h);
    }
}

/*
 * 环检测（Bacon-Rajan 简化版）：
 *   完整实现需要"对象图 + 紫色标记 + 三色传播"，对 GTLang 的"i64 容器元素"
 *   无法精确遍历（元素可能是整数或指针）—— 这里采用"保守"策略：
 *   只维护存活计数，不做"真环回收"（避免误释放整数指针导致崩溃）。
 *   真正的循环引用回收留待"精确类型信息"就绪后再做。
 *
 * 因此本函数当前是"观测点 + 预留"，不会释放任何内存（安全优先）。
 */
void gc_collect_cycles(void) {
    /* 预留：未来接 Bacon-Rajan。当前不做任何回收。 */
}