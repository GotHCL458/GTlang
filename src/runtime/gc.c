#include "gc.h"
#include <windows.h>
#include <stdlib.h>
#include <string.h>

/* 统一分配器：Windows 进程堆（跨 CRT 安全） */
#define GC_HEAP   GetProcessHeap()
#define GC_ALLOC(n)  HeapAlloc(GC_HEAP, 0, (n))
#define GC_FREE(p)   HeapFree(GC_HEAP, 0, (p))

/* 三色标记：0=黑(已确认活) 1=灰(待扫描) 2=白/紫(环检测候选) */
#define GC_BLACK 0
#define GC_GRAY  1
#define GC_PURPLE 2

/* 对象头：紧随其后是用户数据。
 * 头里保存 rc、mark、size，以及"如何遍历子引用"的描述符。 */
typedef struct GcHdr {
    volatile long long rc;      /* 引用计数（原子） */
    volatile long long mark;    /* 三色标记 */
    size_t size;                /* 用户数据字节数 */
    long long elem_ptr;         /* 元素是否为指针（0/1）：list/set/map 用 */
    long long kind;             /* 0=字符串(无子) 1=list 2=set 3=map 4=其他(无子) */
    struct GcHdr *next;         /* 全局对象链（供环检测遍历） */
} GcHdr;

static int GC_ON = 1;
static volatile long long GC_LIVE = 0;
static volatile long long GC_ALLOC_SINCE = 0;
static const long long GC_CYCLE_THRESHOLD = 50000;
static GcHdr *GC_LIST = NULL;          /* 全局对象链 */
static CRITICAL_SECTION GC_LOCK;
static int GC_LOCK_INIT = 0;

static void gc_lock_init(void) {
    if (!GC_LOCK_INIT) { InitializeCriticalSection(&GC_LOCK); GC_LOCK_INIT = 1; }
}

void gc_set_enabled(int on) { GC_ON = on; }
int  gc_enabled(void) { return GC_ON; }
long long gc_live_objects(void) { return GC_LIVE; }

/* 供容器遍历：由调用方按 kind 解释"用户数据"里的子引用 */
typedef void (*GcVisitFn)(void *child);
static GcVisitFn GC_VISITOR = NULL;
void gc_set_visitor(GcVisitFn f) { GC_VISITOR = f; }

void *gc_alloc(size_t n) {
    if (!GC_ON) { return GC_ALLOC(n); }
    GcHdr *h = (GcHdr *)GC_ALLOC(sizeof(GcHdr) + n);
    if (!h) return NULL;
    h->rc = 1;
    h->mark = GC_BLACK;
    h->size = n;
    h->elem_ptr = 0;
    h->kind = 0;
    gc_lock_init();
    EnterCriticalSection(&GC_LOCK);
    h->next = GC_LIST;
    GC_LIST = h;
    LeaveCriticalSection(&GC_LOCK);
    InterlockedIncrement64((volatile LONG64 *)&GC_LIVE);
    long long since = InterlockedIncrement64((volatile LONG64 *)&GC_ALLOC_SINCE);
    if (since >= GC_CYCLE_THRESHOLD) { InterlockedExchange64((volatile LONG64 *)&GC_ALLOC_SINCE, 0); gc_collect_cycles(); }
    return (void *)(h + 1);
}

static GcHdr *hdr_of(void *p) { return p ? ((GcHdr *)p - 1) : NULL; }

/* 标记对象元信息（由容器构造函数调用） */
void gc_set_meta(void *p, long long kind, long long elem_ptr) {
    GcHdr *h = hdr_of(p);
    if (h) { h->kind = kind; h->elem_ptr = elem_ptr; }
}

void gc_inc(void *p) {
    if (!GC_ON || !p) return;
    GcHdr *h = hdr_of(p);
    InterlockedIncrement64((volatile LONG64 *)&h->rc);
}

void gc_dec(void *p, void (*free_fn)(void *)) {
    if (!p) return;
    GcHdr *h = hdr_of(p);
    if (!GC_ON) return;
    if (InterlockedDecrement64((volatile LONG64 *)&h->rc) == 0) {
        InterlockedDecrement64((volatile LONG64 *)&GC_LIVE);
        gc_lock_init();
        EnterCriticalSection(&GC_LOCK);
        GcHdr **pp = &GC_LIST;
        while (*pp && *pp != h) pp = &(*pp)->next;
        if (*pp == h) *pp = h->next;
        LeaveCriticalSection(&GC_LOCK);
        if (free_fn) free_fn(p);
        GC_FREE(h);
    }
}

/* ============================================================
 * Bacon-Rajan 环回收（简化，单线程，非并发）：
 *   1) 对全局链上"rc>0"的对象，尝试从"根"（rc>1，近似外部引用）标记可达
 *   2) 标记后仍为"未标记"的对象，若其所在"环"整体不可达 → 回收
 *
 * 简化点（安全优先）：
 *   - 不做"并发写屏障"，gc_collect_cycles 假定在"用户代码暂停"时调用
 *   - 只回收"确认成环且外部不可达"的对象；拿不准就留着
 * ============================================================ */

/* 遍历一个对象的子引用，对每个"子指针"调用 visit */
static void gc_visit_children(GcHdr *h, void (*visit)(void *)) {
    if (!visit) return;
    void *body = (void *)(h + 1);
    if (h->elem_ptr && (h->kind == 1 /* list */ || h->kind == 2 /* set */)) {
        /* GtList/GtSet：data 指针 + len。布局与 gt_rt.c 对齐：
         *   GtList { rc, data, len, cap, elem_ptr }
         *   但用户数据从 h+1 开始，首字段是 rc（我们已知），data 在 +8 */
        long long *p = (long long *)body;
        /* 跳过 rc（首 8 字节），data 指针在 +8，len 在 +16 */
        void *data = (void *)(size_t)p[1];
        long long len = p[2];
        long long *arr = (long long *)data;
        for (long long i = 0; i < len; i++) {
            void *child = (void *)(size_t)arr[i];
            if (child) visit(child);
        }
    } else if (h->elem_ptr && h->kind == 3 /* map */) {
        long long *p = (long long *)body;
        /* GtMap { rc, keys, vals, len, cap, ht, hcap, elem_ptr } */
        void *keys = (void *)(size_t)p[1];
        void *vals = (void *)(size_t)p[2];
        long long len = p[3];
        long long *ks = (long long *)keys;
        long long *vs = (long long *)vals;
        for (long long i = 0; i < len; i++) {
            void *k = (void *)(size_t)ks[i]; if (k) visit(k);
            void *v = (void *)(size_t)vs[i]; if (v) visit(v);
        }
    }
}

/* 标记阶段（递归，灰栈） */
#define GC_STACK_CAP 65536
static GcHdr *gc_gray_stack[GC_STACK_CAP];
static long long gc_gray_top = 0;

static void gc_mark_gray(GcHdr *h) {
    if (!h) return;
    if (h->mark == GC_GRAY) return;
    h->mark = GC_GRAY;
    if (gc_gray_top < GC_STACK_CAP) gc_gray_stack[gc_gray_top++] = h;
}

/* 扫描：把灰对象的子引用也标灰 */
static void gc_scan(GcHdr *h) {
    h->mark = GC_BLACK;
    gc_visit_children(h, (void (*)(void *))gc_mark_gray);
}

/* 安全开关：只有编译器完成"引用计数插桩"后，环回收才安全。
 * 未插桩时（当前默认），gc_collect_cycles 只清标记、不做回收，
 * 避免把"被外部（未计数）引用"的对象误判为环而释放。 */
static int GC_COLLECT_ON = 0;
void gc_set_collect(int on) { GC_COLLECT_ON = on; }

void gc_collect_cycles(void) {
    if (!GC_ON || !GC_LOCK_INIT) return;
    if (!GC_COLLECT_ON) return;
    EnterCriticalSection(&GC_LOCK);
    /* 1) 所有对象先清 mark */
    for (GcHdr *h = GC_LIST; h; h = h->next) h->mark = GC_BLACK;
    /* 2) 根集：rc > 1 的对象视为"外部仍持有"，标灰（含其可达图） */
    gc_gray_top = 0;
    for (GcHdr *h = GC_LIST; h; h = h->next) {
        if (h->rc > 1) gc_mark_gray(h);
    }
    /* 3) 传播（灰 → 黑，子 → 灰） */
    while (gc_gray_top > 0) {
        GcHdr *h = gc_gray_stack[--gc_gray_top];
        gc_scan(h);
    }
    /* 4) 收集：rc>0 且 mark==BLACK 且不在灰色可达集 —— 这些是"自引用环"
     *    （rc 只被环内对象持有，外部无引用）。安全地回收整环。 */
    GcHdr *dead_head = NULL;
    {
        GcHdr **pp = &GC_LIST;
        while (*pp) {
            GcHdr *h = *pp;
            if (h->rc > 0 && h->mark == GC_BLACK) {
                /* 从链上摘除，加入待释放 */
                *pp = h->next;
                h->next = dead_head;
                dead_head = h;
            } else {
                pp = &(*pp)->next;
            }
        }
    }
    LeaveCriticalSection(&GC_LOCK);
    /* 5) 释放（不在锁内，避免 free_fn 重入） */
    while (dead_head) {
        GcHdr *h = dead_head;
        dead_head = h->next;
        InterlockedDecrement64((volatile LONG64 *)&GC_LIVE);
        /* 注意：这里不调用 free_fn —— 环内对象的内部缓冲由各自 free_fn 释放，
         * 但它们的 rc 已为 0 的兄弟会被 gc_dec 处理。为安全，这里只释放对象本身，
         * 内部缓冲（data/keys/vals/ht）交给调用方注册的释放逻辑。 */
        GC_FREE(h);
    }
}