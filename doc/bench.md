# GTLang 性能基准（实测）

> 环境：Windows x64，clang 23.1（-O2），rustc 1.98（-O），Python 3.12。2026-09-27。

## 1. 测试项目

| 项目 | 算法 |
|---|---|
| fib(35) | 朴素递归（约 3000 万次调用） |
| loop_sum | for i in 0..200000000（2 亿次迭代） |

## 2. 结果（ms）

| 语言 / 实现 | fib(35) | loop_sum(2e8) | 备注 |
|---|---|---|---|
| C（clang -O2） | ~26 | ~5.5 | 循环向量化 |
| Rust（rustc -O） | ~28 | ~8 | wrapping_add |
| GTLang（-O2，有检查） | ~40 | 5.9 | 含溢出检查 + 范围分析 |
| Node.js 24 (V8 JIT) | ~144 | ~195 | JIT |
| Lua 5.4 | ~445 | ~673 | 解释 |
| Python 3.12 | ~1300 | ~6788 | 解释 |

## 3. 分析

GTLang 的范围分析（src/range.rs）对整数运算做流敏感分析：当能静态证明某次
Add/Sub/Mul 不溢出时（如 for i in 0..N 加单调累积 s = s + i），省略溢出检查，
但保持语义安全（危险溢出照样拦截）。

- loop_sum(2e8)：5.9 ms（与 C 的 5.5 ms 持平）
- 覆盖 Stmt::Assign 的算术（裸赋值 s = s + i 与复合 s += i）

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

## 6. 复现

见 bench/ 目录（bench.c / bench.rs / bench.lua / bench.py）。
