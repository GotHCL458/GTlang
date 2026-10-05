/* ============================================================
 * GTO —— GTLang Thread Orchestrator
 * 纯 C 高性能并发框架（M:N 协程调度 + 同步原语 + 通道）
 *
 * 设计目标：
 *   - 极高性能：用户态 M:N 协程（ucontext/fibers），无系统调用切换
 *   - 强大功能：chan/mutex/rwlock/atomic/waitgroup/select/pool
 *   - 零依赖：纯 C11，仅依赖 OS 线程/同步原语
 * ============================================================ */
#ifndef GTO_H
#define GTO_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ---- 版本 ---- */
#define GTO_VERSION "1.0.0"
const char *gto_version(void);

/* ============================================================
 * 1. 调度器（M:N 协程）
 * ============================================================ */

/* 初始化调度器：n_workers 个 OS 线程（0 = CPU 核数） */
int gto_init(int n_workers);
void gto_shutdown(void);

/* 启动一个协程（无参函数） */
typedef void (*GtoFn)(void *arg);
int64_t gto_spawn(GtoFn fn, void *arg);

/* 让出当前协程 */
void gto_yield(void);

/* 等待所有协程结束 */
void gto_wait_all(void);

/* 当前协程 id（0 = 主） */
int64_t gto_self(void);

/* ============================================================
 * 2. 通道（有界/无界，多生产者多消费者）
 * ============================================================ */

typedef struct GtoChan GtoChan;

/* 创建通道：cap <= 0 表示无界 */
GtoChan *gto_chan_new(int64_t cap);
void gto_chan_free(GtoChan *c);

/* 发送/接收（int64 负载；阻塞） */
void gto_chan_send(GtoChan *c, int64_t v);
int64_t gto_chan_recv(GtoChan *c);

/* 非阻塞：成功返回 0，失败返回 -1 */
int gto_chan_try_send(GtoChan *c, int64_t v);
int gto_chan_try_recv(GtoChan *c, int64_t *out);

/* 关闭通道（recv 返回 -1） */
void gto_chan_close(GtoChan *c);

/* 长度 */
int64_t gto_chan_len(GtoChan *c);

/* ============================================================
 * 3. 互斥锁 / 读写锁
 * ============================================================ */

typedef struct GtoMutex GtoMutex;
typedef struct GtoRwLock GtoRwLock;

GtoMutex *gto_mutex_new(void);
void gto_mutex_free(GtoMutex *m);
void gto_mutex_lock(GtoMutex *m);
int gto_mutex_try_lock(GtoMutex *m);
void gto_mutex_unlock(GtoMutex *m);

GtoRwLock *gto_rwlock_new(void);
void gto_rwlock_free(GtoRwLock *r);
void gto_rwlock_rdlock(GtoRwLock *r);
void gto_rwlock_wrlock(GtoRwLock *r);
void gto_rwlock_unlock(GtoRwLock *r);

/* ============================================================
 * 4. 原子操作
 * ============================================================ */

int64_t gto_atomic_add(volatile int64_t *p, int64_t v);
int64_t gto_atomic_load(volatile int64_t *p);
void gto_atomic_store(volatile int64_t *p, int64_t v);
int64_t gto_atomic_cas(volatile int64_t *p, int64_t expect, int64_t v);
int64_t gto_atomic_swap(volatile int64_t *p, int64_t v);

/* ============================================================
 * 5. WaitGroup
 * ============================================================ */

typedef struct GtoWaitGroup GtoWaitGroup;
GtoWaitGroup *gto_wg_new(void);
void gto_wg_free(GtoWaitGroup *w);
void gto_wg_add(GtoWaitGroup *w, int64_t n);
void gto_wg_done(GtoWaitGroup *w);
void gto_wg_wait(GtoWaitGroup *w);

/* ============================================================
 * 6. select（多通道等待）
 * ============================================================ */

/* 等待任一通道可读，返回通道下标（-1 超时）；timeout_ms < 0 表示永久阻塞 */
int gto_select_recv(GtoChan **chans, int n, int64_t *out, int64_t timeout_ms);

/* ============================================================
 * 7. 线程池（CPU 密集型）
 * ============================================================ */

typedef struct GtoPool GtoPool;
typedef void (*GtoTask)(void *arg);

GtoPool *gto_pool_new(int n_threads);
void gto_pool_free(GtoPool *p);
void gto_pool_submit(GtoPool *p, GtoTask fn, void *arg);
void gto_pool_wait(GtoPool *p);

#ifdef __cplusplus
}
#endif

#endif /* GTO_H */
