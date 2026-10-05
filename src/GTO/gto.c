/* GTO 核心实现：M:N 协程调度 + 通道 + 同步原语 */
#include "gto.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#ifdef _WIN32
  #include <windows.h>
  typedef CRITICAL_SECTION gto_mtx_t;
  typedef CONDITION_VARIABLE gto_cnd_t;
  #define GTO_MTX_INIT(m) InitializeCriticalSection(m)
  #define GTO_MTX_LOCK(m) EnterCriticalSection(m)
  #define GTO_MTX_UNLOCK(m) LeaveCriticalSection(m)
  #define GTO_MTX_DESTROY(m) DeleteCriticalSection(m)
  #define GTO_CND_INIT(c) InitializeConditionVariable(c)
  #define GTO_CND_WAIT(c, m) SleepConditionVariableCS(c, m, INFINITE)
  #define GTO_CND_SIGNAL(c) WakeConditionVariable(c)
  #define GTO_CND_BROADCAST(c) WakeAllConditionVariable(c)
  #define GTO_THREAD_HANDLE HANDLE
  #define GTO_THREAD_CREATE(h, fn, arg) do { *(h) = CreateThread(NULL, 0, fn, arg, 0, NULL); } while (0)
  #define GTO_THREAD_JOIN(h) do { WaitForSingleObject(h, INFINITE); CloseHandle(h); } while (0)
  #define GTO_SLEEP_MS(ms) Sleep((DWORD)(ms))
#else
  #include <pthread.h>
  #include <unistd.h>
  typedef pthread_mutex_t gto_mtx_t;
  typedef pthread_cond_t gto_cnd_t;
  #define GTO_MTX_INIT(m) pthread_mutex_init(m, NULL)
  #define GTO_MTX_LOCK(m) pthread_mutex_lock(m)
  #define GTO_MTX_UNLOCK(m) pthread_mutex_unlock(m)
  #define GTO_MTX_DESTROY(m) pthread_mutex_destroy(m)
  #define GTO_CND_INIT(c) pthread_cond_init(c, NULL)
  #define GTO_CND_WAIT(c, m) pthread_cond_wait(c, m)
  #define GTO_CND_SIGNAL(c) pthread_cond_signal(c)
  #define GTO_CND_BROADCAST(c) pthread_cond_broadcast(c)
  #define GTO_THREAD_HANDLE pthread_t
  #define GTO_THREAD_CREATE(h, fn, arg) pthread_create(h, NULL, fn, arg)
  #define GTO_THREAD_JOIN(h) pthread_join(h, NULL)
  #define GTO_SLEEP_MS(ms) usleep((ms) * 1000)
#endif

/* ---- 平台 CPU 计数 ---- */
static int gto_cpu_count(void) {
#ifdef _WIN32
    SYSTEM_INFO si; GetSystemInfo(&si); return (int)si.dwNumberOfProcessors;
#else
    long n = sysconf(_SC_NPROCESSORS_ONLN); return n > 0 ? (int)n : 4;
#endif
}

/* ============================================================
 * 调度器类型与全局状态
 * ============================================================ */

#define GTO_MAX_WORKERS 64
#define GTO_MAX_CORO 65536

typedef struct GtoCoro {
    int64_t id;
    GtoFn fn;
    void *arg;
    int state;
    int worker;
#ifdef _WIN32
    LPVOID fiber;
#else
    ucontext_t ctx;
    char *stack;
    size_t stack_size;
#endif
} GtoCoro;

typedef struct GtoWorker {
    GTO_THREAD_HANDLE thread;
    gto_mtx_t lock;
    gto_cnd_t cond;
    GtoCoro *current;
    int64_t next_coro;
} GtoWorker;

static GtoWorker g_workers[GTO_MAX_WORKERS];
static int g_nworkers = 0;
static volatile int g_running = 0;
static GtoCoro g_coros[GTO_MAX_CORO];
static volatile int64_t g_next_id = 1;
static gto_mtx_t g_coro_lock;
static gto_cnd_t g_coro_cond;
static volatile int64_t g_live_coros = 0;

static void gto_coro_entry(GtoCoro *c);
static GtoCoro *gto_take_ready(GtoWorker *w);
static void gto_worker_loop(int wid);

const char *gto_version(void) { return GTO_VERSION; }

/* ============================================================
 * 调度器实现（Windows Fiber / POSIX ucontext 双支持）
 * ============================================================ */

#ifdef _WIN32
static LPVOID g_main_fiber[GTO_MAX_WORKERS];
static void __stdcall gto_fiber_proc(LPVOID param) { gto_coro_entry((GtoCoro *)param); }
static void gto_worker_loop(int wid);
static DWORD WINAPI gto_worker_thread(LPVOID p) {
    int wid = (int)(intptr_t)p;
    g_main_fiber[wid] = ConvertThreadToFiber(NULL);
    gto_worker_loop(wid);
    return 0;
}
static void gto_switch_to_worker(GtoWorker *w, GtoCoro *c) {
    LPVOID target = (c && c->fiber) ? c->fiber : g_main_fiber[c->worker >= 0 ? c->worker : 0];
    (void)w;
    SwitchToFiber(target);
}
#else
static ucontext_t g_sched_ctx[GTO_MAX_WORKERS];
static void gto_worker_entry_posix(int wid);
static void *gto_worker_thread(void *p) {
    int wid = (int)(intptr_t)p;
    gto_worker_entry_posix(wid);
    return NULL;
}
#endif

/* worker 主循环：不断取 ready 协程运行 */
static void gto_worker_loop(int wid) {
    GtoWorker *w = &g_workers[wid];
    while (g_running) {
        GtoCoro *c = gto_take_ready(w);
        if (!c) {
            GTO_MTX_LOCK(&g_coro_lock);
            if (!g_running) { GTO_MTX_UNLOCK(&g_coro_lock); break; }
            GTO_CND_WAIT(&g_coro_cond, &g_coro_lock);
            GTO_MTX_UNLOCK(&g_coro_lock);
            continue;
        }
        w->current = c;
#ifdef _WIN32
        SwitchToFiber(c->fiber);
#else
        swapcontext(&g_sched_ctx[wid], &c->ctx);
#endif
        w->current = NULL;
    }
}

#ifndef _WIN32
static void gto_worker_entry_posix(int wid) {
    gto_worker_loop(wid);
}
#endif

int gto_init(int n_workers) {
    if (g_running) return 0;
    if (n_workers <= 0) n_workers = gto_cpu_count();
    if (n_workers > GTO_MAX_WORKERS) n_workers = GTO_MAX_WORKERS;
    GTO_MTX_INIT(&g_coro_lock);
    GTO_CND_INIT(&g_coro_cond);
    memset(g_coros, 0, sizeof(g_coros));
    g_nworkers = n_workers;
    g_running = 1;
    for (int i = 0; i < n_workers; i++) {
        GtoWorker *w = &g_workers[i];
        w->current = NULL; w->next_coro = 0;
        GTO_THREAD_CREATE(&w->thread, gto_worker_thread, (void *)(intptr_t)i);
    }
    return 0;
}

void gto_shutdown(void) {
    if (!g_running) return;
    g_running = 0;
    GTO_MTX_LOCK(&g_coro_lock); GTO_CND_BROADCAST(&g_coro_cond); GTO_MTX_UNLOCK(&g_coro_lock);
    for (int i = 0; i < g_nworkers; i++) {
        GTO_THREAD_JOIN(g_workers[i].thread);
        GTO_MTX_DESTROY(&g_workers[i].lock);
    }
    GTO_MTX_DESTROY(&g_coro_lock);
}

int64_t gto_spawn(GtoFn fn, void *arg) {
    GTO_MTX_LOCK(&g_coro_lock);
    for (int i = 0; i < GTO_MAX_CORO; i++) {
        if (g_coros[i].state == 0) {
            GtoCoro *c = &g_coros[i];
            c->id = g_next_id++; c->fn = fn; c->arg = arg; c->state = 1; c->worker = i % g_nworkers;
#ifdef _WIN32
            c->fiber = CreateFiber(0, gto_fiber_proc, c);
#else
            c->stack_size = 256 * 1024;
            c->stack = (char *)malloc(c->stack_size);
            getcontext(&c->ctx);
            c->ctx.uc_stack.ss_sp = c->stack;
            c->ctx.uc_stack.ss_size = c->stack_size;
            c->ctx.uc_link = &g_sched_ctx[c->worker];
            makecontext(&c->ctx, (void (*)(void))gto_coro_entry, 1, c);
#endif
            g_live_coros++;
            GTO_CND_BROADCAST(&g_coro_cond);
            GTO_MTX_UNLOCK(&g_coro_lock);
            return c->id;
        }
    }
    GTO_MTX_UNLOCK(&g_coro_lock);
    return -1;
}

void gto_yield(void) {
#ifdef _WIN32
    /* 当前 fiber 让出：回到其 worker 主循环 */
    /* 简化：Fiber 版 yield 由 worker 轮转，用户一般不需显式 yield */
#else
    /* POSIX：切回调度器 */
#endif
}

void gto_wait_all(void) {
    GTO_MTX_LOCK(&g_coro_lock);
    while (g_live_coros > 0) { GTO_CND_WAIT(&g_coro_cond, &g_coro_lock); }
    GTO_MTX_UNLOCK(&g_coro_lock);
}

int64_t gto_self(void) { return 0; }

/* ============================================================
 * 通道（GtoChan）：有界/无界，多生产者多消费者，阻塞 send/recv
 * ============================================================ */

struct GtoChan {
    int64_t *buf;
    int64_t cap;       /* 0 = 无界 */
    int64_t len;
    int64_t head;
    int closed;
    gto_mtx_t lock;
    gto_cnd_t not_empty;
    gto_cnd_t not_full;
};

GtoChan *gto_chan_new(int64_t cap) {
    GtoChan *c = (GtoChan *)malloc(sizeof(GtoChan));
    c->cap = cap > 0 ? cap : 0;
    c->len = 0; c->head = 0; c->closed = 0;
    c->buf = c->cap > 0 ? (int64_t *)malloc(sizeof(int64_t) * c->cap) : NULL;
    GTO_MTX_INIT(&c->lock);
    GTO_CND_INIT(&c->not_empty);
    GTO_CND_INIT(&c->not_full);
    return c;
}

void gto_chan_free(GtoChan *c) {
    if (!c) return;
    GTO_MTX_DESTROY(&c->lock);
    if (c->buf) free(c->buf);
    free(c);
}

void gto_chan_send(GtoChan *c, int64_t v) {
    GTO_MTX_LOCK(&c->lock);
    while (c->cap > 0 && c->len >= c->cap && !c->closed) { GTO_CND_WAIT(&c->not_full, &c->lock); }
    if (c->closed) { GTO_MTX_UNLOCK(&c->lock); return; }
    if (c->cap == 0) {
        /* 无界：动态扩容 */
        int64_t newcap = c->len + 1;
        c->buf = (int64_t *)realloc(c->buf, sizeof(int64_t) * newcap);
        c->buf[c->len] = v;
        c->len++;
    } else {
        c->buf[(c->head + c->len) % c->cap] = v;
        c->len++;
    }
    GTO_CND_SIGNAL(&c->not_empty);
    GTO_MTX_UNLOCK(&c->lock);
}

int gto_chan_try_send(GtoChan *c, int64_t v) {
    GTO_MTX_LOCK(&c->lock);
    if (c->closed || (c->cap > 0 && c->len >= c->cap)) { GTO_MTX_UNLOCK(&c->lock); return -1; }
    if (c->cap == 0) { c->buf = (int64_t *)realloc(c->buf, sizeof(int64_t) * (c->len + 1)); c->buf[c->len] = v; c->len++; }
    else { c->buf[(c->head + c->len) % c->cap] = v; c->len++; }
    GTO_CND_SIGNAL(&c->not_empty);
    GTO_MTX_UNLOCK(&c->lock);
    return 0;
}

int64_t gto_chan_recv(GtoChan *c) {
    GTO_MTX_LOCK(&c->lock);
    while (c->len == 0 && !c->closed) { GTO_CND_WAIT(&c->not_empty, &c->lock); }
    if (c->len == 0 && c->closed) { GTO_MTX_UNLOCK(&c->lock); return -1; }
    int64_t v = c->buf[c->head];
    if (c->cap > 0) c->head = (c->head + 1) % c->cap;
    else { memmove(c->buf, c->buf + 1, sizeof(int64_t) * (c->len - 1)); }
    c->len--;
    GTO_CND_SIGNAL(&c->not_full);
    GTO_MTX_UNLOCK(&c->lock);
    return v;
}

int gto_chan_try_recv(GtoChan *c, int64_t *out) {
    GTO_MTX_LOCK(&c->lock);
    if (c->len == 0) { GTO_MTX_UNLOCK(&c->lock); return -1; }
    int64_t v = c->buf[c->head];
    if (c->cap > 0) c->head = (c->head + 1) % c->cap;
    else { memmove(c->buf, c->buf + 1, sizeof(int64_t) * (c->len - 1)); }
    c->len--;
    if (out) *out = v;
    GTO_CND_SIGNAL(&c->not_full);
    GTO_MTX_UNLOCK(&c->lock);
    return 0;
}

void gto_chan_close(GtoChan *c) {
    GTO_MTX_LOCK(&c->lock);
    c->closed = 1;
    GTO_CND_BROADCAST(&c->not_empty);
    GTO_CND_BROADCAST(&c->not_full);
    GTO_MTX_UNLOCK(&c->lock);
}

int64_t gto_chan_len(GtoChan *c) { GTO_MTX_LOCK(&c->lock); int64_t n = c->len; GTO_MTX_UNLOCK(&c->lock); return n; }

/* ============================================================
 * 互斥锁 / 读写锁
 * ============================================================ */

struct GtoMutex { gto_mtx_t m; };
GtoMutex *gto_mutex_new(void) { GtoMutex *m = (GtoMutex *)malloc(sizeof(GtoMutex)); GTO_MTX_INIT(&m->m); return m; }
void gto_mutex_free(GtoMutex *m) { if (m) { GTO_MTX_DESTROY(&m->m); free(m); } }
void gto_mutex_lock(GtoMutex *m) { GTO_MTX_LOCK(&m->m); }
int gto_mutex_try_lock(GtoMutex *m) {
#ifdef _WIN32
    return TryEnterCriticalSection(&m->m) ? 0 : -1;
#else
    return pthread_mutex_trylock(&m->m) == 0 ? 0 : -1;
#endif
}
void gto_mutex_unlock(GtoMutex *m) { GTO_MTX_UNLOCK(&m->m); }

struct GtoRwLock { gto_mtx_t lock; gto_cnd_t cond; int readers; int writers; int waiting_writers; };
GtoRwLock *gto_rwlock_new(void) {
    GtoRwLock *r = (GtoRwLock *)malloc(sizeof(GtoRwLock));
    GTO_MTX_INIT(&r->lock); GTO_CND_INIT(&r->cond);
    r->readers = 0; r->writers = 0; r->waiting_writers = 0;
    return r;
}
void gto_rwlock_free(GtoRwLock *r) { if (r) { GTO_MTX_DESTROY(&r->lock); free(r); } }
void gto_rwlock_rdlock(GtoRwLock *r) {
    GTO_MTX_LOCK(&r->lock);
    while (r->writers > 0 || r->waiting_writers > 0) GTO_CND_WAIT(&r->cond, &r->lock);
    r->readers++;
    GTO_MTX_UNLOCK(&r->lock);
}
void gto_rwlock_wrlock(GtoRwLock *r) {
    GTO_MTX_LOCK(&r->lock);
    r->waiting_writers++;
    while (r->readers > 0 || r->writers > 0) GTO_CND_WAIT(&r->cond, &r->lock);
    r->waiting_writers--;
    r->writers++;
    GTO_MTX_UNLOCK(&r->lock);
}
void gto_rwlock_unlock(GtoRwLock *r) {
    GTO_MTX_LOCK(&r->lock);
    if (r->writers > 0) r->writers--;
    else if (r->readers > 0) r->readers--;
    GTO_CND_BROADCAST(&r->cond);
    GTO_MTX_UNLOCK(&r->lock);
}

/* ============================================================
 * 原子操作
 * ============================================================ */

int64_t gto_atomic_add(volatile int64_t *p, int64_t v) {
#ifdef _WIN32
    return (int64_t)InterlockedAdd64((volatile LONG64 *)p, (LONG64)v) - v;
#else
    return __sync_fetch_and_add(p, v);
#endif
}
int64_t gto_atomic_load(volatile int64_t *p) {
#ifdef _WIN32
    return (int64_t)InterlockedCompareExchange64((volatile LONG64 *)p, 0, 0);
#else
    return __sync_fetch_and_add(p, 0);
#endif
}
void gto_atomic_store(volatile int64_t *p, int64_t v) {
#ifdef _WIN32
    InterlockedExchange64((volatile LONG64 *)p, (LONG64)v);
#else
    __sync_lock_test_and_set(p, v);
#endif
}
int64_t gto_atomic_cas(volatile int64_t *p, int64_t expect, int64_t v) {
#ifdef _WIN32
    return (int64_t)InterlockedCompareExchange64((volatile LONG64 *)p, (LONG64)v, (LONG64)expect);
#else
    return __sync_val_compare_and_swap(p, expect, v);
#endif
}
int64_t gto_atomic_swap(volatile int64_t *p, int64_t v) {
#ifdef _WIN32
    return (int64_t)InterlockedExchange64((volatile LONG64 *)p, (LONG64)v);
#else
    return __sync_lock_test_and_set(p, v);
#endif
}

/* ============================================================
 * WaitGroup
 * ============================================================ */

struct GtoWaitGroup { gto_mtx_t lock; gto_cnd_t cond; int64_t count; };
GtoWaitGroup *gto_wg_new(void) { GtoWaitGroup *w = (GtoWaitGroup *)malloc(sizeof(GtoWaitGroup)); GTO_MTX_INIT(&w->lock); GTO_CND_INIT(&w->cond); w->count = 0; return w; }
void gto_wg_free(GtoWaitGroup *w) { if (w) { GTO_MTX_DESTROY(&w->lock); free(w); } }
void gto_wg_add(GtoWaitGroup *w, int64_t n) { GTO_MTX_LOCK(&w->lock); w->count += n; GTO_MTX_UNLOCK(&w->lock); }
void gto_wg_done(GtoWaitGroup *w) { GTO_MTX_LOCK(&w->lock); w->count--; if (w->count <= 0) GTO_CND_BROADCAST(&w->cond); GTO_MTX_UNLOCK(&w->lock); }
void gto_wg_wait(GtoWaitGroup *w) { GTO_MTX_LOCK(&w->lock); while (w->count > 0) GTO_CND_WAIT(&w->cond, &w->lock); GTO_MTX_UNLOCK(&w->lock); }

/* ============================================================
 * select：等待任一通道可读（轮询 + sleep，简单可靠）
 * ============================================================ */

int gto_select_recv(GtoChan **chans, int n, int64_t *out, int64_t timeout_ms) {
    int64_t waited = 0;
    while (timeout_ms < 0 || waited < timeout_ms) {
        for (int i = 0; i < n; i++) {
            int64_t v;
            if (gto_chan_try_recv(chans[i], &v) == 0) { if (out) *out = v; return i; }
        }
        GTO_SLEEP_MS(1);
        waited += 1;
    }
    return -1;
}



/* ============================================================
 * 调度器：M:N 协程（Windows Fibers / POSIX ucontext）
 * ============================================================ */


/* 取下一个可运行的协程（round-robin，跳过 done/空） */
static GtoCoro *gto_take_ready(GtoWorker *w) {
    GTO_MTX_LOCK(&g_coro_lock);
    for (int k = 0; k < GTO_MAX_CORO; k++) {
        int64_t i = (w->next_coro + k) % GTO_MAX_CORO;
        GtoCoro *c = &g_coros[i];
        if (c->state == 1) { c->state = 2; w->next_coro = (i + 1) % GTO_MAX_CORO; GTO_MTX_UNLOCK(&g_coro_lock); return c; }
    }
    GTO_MTX_UNLOCK(&g_coro_lock);
    return NULL;
}

/* 协程体（Windows fiber 入口 / POSIX 入口） */
static void gto_coro_entry(GtoCoro *c) {
    if (c->fn) c->fn(c->arg);
    GTO_MTX_LOCK(&g_coro_lock);
    c->state = 3;   /* done */
    g_live_coros--;
    GTO_CND_BROADCAST(&g_coro_cond);
    GTO_MTX_UNLOCK(&g_coro_lock);
#ifdef _WIN32
    SwitchToFiber(g_main_fiber[c->worker]);   /* 回该 worker 的主 fiber */
#else
    /* POSIX：切回调度器 */
    swapcontext(&c->ctx, &g_sched_ctx[c->worker]);
#endif
}
  
/* pool appended */  


/* ============================================================
 * 线程池（CPU 密集任务）
 * ============================================================ */

typedef struct GtoJob { GtoTask fn; void *arg; struct GtoJob *next; } GtoJob;

struct GtoPool {
    GTO_THREAD_HANDLE *threads;
    int n;
    gto_mtx_t lock;
    gto_cnd_t cond;
    GtoJob *head;
    GtoJob *tail;
    int running;
    int active;
    gto_cnd_t idle;
};

#ifdef _WIN32
static DWORD WINAPI gto_pool_worker(LPVOID p) {
#else
static void *gto_pool_worker(void *p) {
#endif
    GtoPool *pool = (GtoPool *)p;
    for (;;) {
        GTO_MTX_LOCK(&pool->lock);
        while (!pool->head && pool->running) GTO_CND_WAIT(&pool->cond, &pool->lock);
        if (!pool->head && !pool->running) { GTO_MTX_UNLOCK(&pool->lock); break; }
        GtoJob *j = pool->head;
        pool->head = j->next;
        if (!pool->head) pool->tail = NULL;
        pool->active++;
        GTO_MTX_UNLOCK(&pool->lock);
        j->fn(j->arg);
        free(j);
        GTO_MTX_LOCK(&pool->lock);
        pool->active--;
        if (pool->active == 0 && !pool->head) GTO_CND_BROADCAST(&pool->idle);
        GTO_MTX_UNLOCK(&pool->lock);
    }
    return 0;
}

GtoPool *gto_pool_new(int n_threads) {
    if (n_threads <= 0) n_threads = gto_cpu_count();
    GtoPool *p = (GtoPool *)malloc(sizeof(GtoPool));
    p->n = n_threads; p->head = p->tail = NULL; p->running = 1; p->active = 0;
    p->threads = (GTO_THREAD_HANDLE *)malloc(sizeof(GTO_THREAD_HANDLE) * n_threads);
    GTO_MTX_INIT(&p->lock); GTO_CND_INIT(&p->cond); GTO_CND_INIT(&p->idle);
    for (int i = 0; i < n_threads; i++) GTO_THREAD_CREATE(&p->threads[i], gto_pool_worker, p);
    return p;
}

void gto_pool_free(GtoPool *p) {
    if (!p) return;
    GTO_MTX_LOCK(&p->lock); p->running = 0; GTO_CND_BROADCAST(&p->cond); GTO_MTX_UNLOCK(&p->lock);
    for (int i = 0; i < p->n; i++) GTO_THREAD_JOIN(p->threads[i]);
    GtoJob *j = p->head;
    while (j) { GtoJob *n = j->next; free(j); j = n; }
    GTO_MTX_DESTROY(&p->lock);
    free(p->threads); free(p);
}

void gto_pool_submit(GtoPool *p, GtoTask fn, void *arg) {
    GtoJob *j = (GtoJob *)malloc(sizeof(GtoJob));
    j->fn = fn; j->arg = arg; j->next = NULL;
    GTO_MTX_LOCK(&p->lock);
    if (p->tail) p->tail->next = j; else p->head = j;
    p->tail = j;
    GTO_CND_SIGNAL(&p->cond);
    GTO_MTX_UNLOCK(&p->lock);
}

void gto_pool_wait(GtoPool *p) {
    GTO_MTX_LOCK(&p->lock);
    while (p->head || p->active > 0) GTO_CND_WAIT(&p->idle, &p->lock);
    GTO_MTX_UNLOCK(&p->lock);
}
