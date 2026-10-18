# GTLang 性能基准（实测）

> [English](en/bench.md)

> 环境：Windows x64，clang 23.1（-O2），rustc 1.98（-O），Python 3.12。2026-09-27。

## 1. 测试项目

| 项目 | 算法 |
|---|---|
| fib(35) | 朴素递归（约 3000 万次调用） |
| loop_sum | for i in 0..200000000（2 亿次迭代） |

**fib(35) 实测（2026-09）**：

| 版本 | 时间 |
|---|---|
| GTLang -O4（裸 i64） | ~26 ms |
| C（clang -O2） | ~22 ms |

fib 是**递归**（`n-1`/`n-2`），裸 i64 无溢出检查，直接 `add`（与 C 一致）。

## 2. 结果（ms）

| 语言 / 实现 | fib(35) | loop_sum(2e8) | 备注 |
|---|---|---|---|
| C（clang -O2） | ~22 | ~5 | 循环向量化 |
| Rust（rustc -O） | ~24 | ~5 | wrapping_add |
| nasm（手写汇编） | ~36 | ~50 | 无优化、朴素 |
| **GTLang -O4（裸 i64）** | **~26** | **~4.2** | 裸 i64，溢出回绕（C 语义） |
| GTLang -O2 | ~30 | ~5 | |
| GTLang -O0 | ~40 | ~40 | |
| Node.js 24 (V8 JIT) | ~144 | ~195 | JIT |
| Lua 5.4 | ~445 | ~673 | 解释 |
| Python 3.12 | ~1300 | ~6788 | 解释 |

> **裸 i64（C 语义）**：GTLang 的 `int` 是裸 64 位整数，溢出**回绕**（与 C 一致）。紧循环里就是 LLVM 的原生 `add/sub/mul`，无任何运行时开销，因此 `loop_sum` 追平 C（~4.2ms）、`fib` 接近 C（~26ms）。高精度需求用独立的 `decimal` 类型。
>
> **优化器有效**：`-O0 → -O4` 把 loop_sum 从 ~40ms 降到 ~4.2ms、fib 从 ~40ms 降到 ~26ms（常量折叠/内联/死代码）。

## 3. 分析

GTLang 的范围分析（src/range.rs）对整数运算做流敏感分析：当能静态证明某次
Add/Sub/Mul 不溢出时（如 for i in 0..N 加单调累积 s = s + i），省略溢出检查，
但保持语义安全（裸 i64，溢出回绕，与 C 一致）。

- 整数算术是裸 i64（`add/sub/mul`），无运行时开销；溢出回绕（C 语义）。
- `loop_sum` 当前 ~4.2ms（-O4），与 C（~5ms）持平——裸 i64 算术无运行时开销。

## 4. 关闭溢出检查

gtc bench/xxx.gt -o x.exe -O2                     # 默认（有检查 + 范围分析）
gtc bench/xxx.gt -o x.exe -O2 --no-overflow-check # 关闭检查

| 版本 | fib(35) | loop_sum(2e8) |
|---|---|---|
| GTLang 默认（范围分析） | ~40 ms | 5.9 ms |
| GTLang --no-overflow-check | ~25 ms | ~6 ms |
| C（clang -O2） | ~26 ms | ~5.5 ms |

## 5. 结论

- 默认（有安全检查）：loop 追平 C；fib 因溢出检查略慢（1.5x）
- 范围分析是核心：证明安全即省略检查，不牺牲安全性
- --no-overflow-check 可进一步提速（代价：溢出静默回绕）

## 6. 并发与编译速度

**`go` 任务派发**：使用**固定 worker 线程池**（而非"每任务起线程"）。
2000 个 `go` 任务的总派发开销约 **7 ms**（此前"每任务 spawn"要数百 ms）。

**编译速度**（`gtc --check`，Windows x64）：近线性。
单文件 + 数百个顶层函数通常在 **数十 ms** 量级；5000 个顶层函数约 80 ms。

| 规模 | `--check` 耗时 |
|---|---|
| 300 函数 | ~11 ms |
| 2000 函数 | ~43 ms |
| 5000 函数 | ~82 ms |

**集合/映射**：开放寻址哈希，均摊 O(1)（此前是线性扫描）。

---

## 7. 复现

见 bench/ 目录（bench.c / bench.rs / bench.lua / bench.py）。
