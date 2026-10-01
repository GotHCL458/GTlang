#ifndef GT_GC_H
#define GT_GC_H

/* GTLang 内存管理（混合模式）：
 *   - 引用计数（RC，原子）：主要机制，无 STW
 *   - 环检测：Bacon-Rajan 风格（周期性，回收循环引用）
 *   - 保守容器扫描：i64 元素可能是整数或指针，宁可漏回收不可误释放
 *
 * 设计约束：
 *   - 所有堆对象首字段为 8 字节 rc
 *   - 分配统一走 HeapAlloc（进程堆，跨 CRT 安全）
 *   - --no-gc 时全部退化为"裸分配"（不计数、不回收）
 */
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* 是否启用 GC（编译器用 --no-gc 时置 0） */
void gc_set_enabled(int on);
int  gc_enabled(void);

/* 分配：返回可用内存（首 8 字节为 rc，调用方拿到的指针 = 头 + 8） */
void *gc_alloc(size_t n);

/* 引用计数增减（原子） */
void gc_inc(void *p);
void gc_dec(void *p, void (*free_fn)(void *));

/* 周期性环检测（阈值自动触发；也可手动调用） */
void gc_collect_cycles(void);

/* 统计（可选） */
long long gc_live_objects(void);

#ifdef __cplusplus
}
#endif
#endif /* GT_GC_H */