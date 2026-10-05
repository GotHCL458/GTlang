# GTO —— GTLang Thread Orchestrator

纯 C 高性能并发框架（M:N 协程调度 + 同步原语 + 通道）。

## 特性

| 模块 | API | 说明 |
|---|---|---|
| **调度器** | `gto_init(n)` / `gto_spawn(fn, arg)` / `gto_yield()` / `gto_wait_all()` / `gto_shutdown()` | M:N 协程（Windows Fiber / POSIX ucontext）|
| **通道** | `gto_chan_new(cap)` / `send` / `recv` / `try_send` / `try_recv` / `close` / `len` | 有界/无界，多生产者多消费者 |
| **互斥锁** | `gto_mutex_new` / `lock` / `try_lock` / `unlock` | |
| **读写锁** | `gto_rwlock_new` / `rdlock` / `wrlock` / `unlock` | 写优先，避免写饥饿 |
| **原子** | `gto_atomic_add/load/store/cas/swap` | int64 |
| **WaitGroup** | `gto_wg_new` / `add` / `done` / `wait` | |
| **select** | `gto_select_recv(chans, n, out, timeout_ms)` | 多通道等待 |
| **线程池** | `gto_pool_new(n)` / `submit` / `wait` / `free` | CPU 密集任务 |

## 用法

```c
#include "GTO/gto.h"

static GtoChan *ch;
static void producer(void *_) { for (int i = 0; i < 10; i++) gto_chan_send(ch, i); gto_chan_close(ch); }
static void consumer(void *_) { int64_t v, sum = 0; while ((v = gto_chan_recv(ch)) >= 0) sum += v; printf("%ld\n", sum); }

int main(void) {
    gto_init(4);          // 4 个 worker 线程
    ch = gto_chan_new(0); // 无界通道
    gto_spawn(producer, NULL);
    gto_spawn(consumer, NULL);
    gto_wait_all();
    gto_shutdown();
}
```

## 平台

- **Windows**：Fiber + CRITICAL_SECTION + CONDITION_VARIABLE + Interlocked
- **POSIX**：ucontext + pthread + __sync_*

## 编译

```bat
clang -I src -o app.exe app.c src/GTO/gto.c
```

## 设计

- **零依赖**：纯 C11 + OS 原语
- **M:N 调度**：N 个 OS 线程跑 M 个协程（round-robin ready 队列）
- **无锁读路径**：通道 try_recv 用短临界区；select 用轮询 + 1ms sleep
- **写优先读写锁**：避免写饥饿

