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
| GTLang 默认（有溢出检查） | ~43 ms |
| GTLang `--no-overflow-check` | ~33 ms |
| C（clang -O2） | ~26 ms |
| GTLang（bignum 快路径） | ~43 ms |

fib 是**递归**（`n-1`/`n-2`），范围分析无法证明不溢出，故每次调用有 2 次检查；这是**安全**的代价。用 `--no-overflow-check` 可关闭（溢出回绕）。

## 2. 结果（ms）

| 语言 / 实现 | fib(35) | loop_sum(2e8) | 备注 |
|---|---|---|---|
| C（clang -O2） | ~22 | ~5 | 循环向量化 |
| Rust（rustc -O） | ~24 | ~5 | wrapping_add |
| nasm（手写汇编） | ~36 | ~50 | 无优化、朴素 |
| **GTLang -O4（双层 int）** | **~113** | **~818** | tagged int + 溢出→bignum |
| GTLang -O2 | ~137 | ~909 | |
| GTLang -O0 | ~309 | ~1456 | |
| Node.js 24 (V8 JIT) | ~144 | ~195 | JIT |
| Lua 5.4 | ~445 | ~673 | 解释 |
| Python 3.12 | ~1300 | ~6788 | 解释 |

> **注（双层 int 的代价）**：GTLang 的 `int` 是 8 字节 tagged 值——小整数内联、溢出自动升级 bignum（永不终止）。这带来安全性（无溢出回绕、无大数丢失），但**紧循环里的每次算术都要做 tagged 解码/编码**，因此比 C/Rust 的裸 i64 慢。`loop_sum` 的 `s = s + i` 每轮 2e8 次 `gt_int_add` 调用，是主要开销来源。
>
> **优化器有效**：`-O0 → -O4` 把 loop_sum 从 1456ms 降到 818ms、fib 从 309ms 降到 113ms（常量折叠/内联/死代码）。
>
> **后续工作**：「安全变量裸化」——当范围分析证明循环内变量不溢出时，改用裸 i64 槽（去掉 tagged 开销），目标追平 C。

## 3. 分析

GTLang 的范围分析（src/range.rs）对整数运算做流敏感分析：当能静态证明某次
Add/Sub/Mul 不溢出时（如 for i in 0..N 加单调累积 s = s + i），省略溢出检查，
但保持语义安全（危险溢出时整数自动升级 bignum，不再终止）。

- 范围分析目前只用于**省略边界检查**；整数算术仍走 tagged `gt_int_*`（保证溢出→bignum 语义）。
- `loop_sum` 当前 ~818ms（-O4），远慢于 C——因为每轮 `s = s + i` 都调 `gt_int_add`。后续「安全变量裸化」会把此类循环特化为裸 i64，目标追平 C。

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
