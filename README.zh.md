# GTLang

**静态类型**、**编译型**、**表达式导向**的编程语言，**原生支持中文标识符**，**双后端**（LLVM + Cranelift），**内存安全**。

[English](README.md)

---

## 特性

- **双后端**：同一 AST → ① LLVM（编译为 exe）② Cranelift JIT（内存执行），逐字节一致
- **静态类型** + 推断；溢出/边界/除零检查
- **所有权与借用检查**（流敏感 NLL）
- **泛型**（单态化）、trait、dyn Trait（vtable 动态分发）
- **枚举**（带载荷）+ 穷尽 match（范围/守卫/OR）
- **闭包**、高阶函数、默认/命名参数
- **宏**：声明式 macro + @derive（Eq/PartialEq/Clone/Debug/Display/Default/Hash/Ord）
- **并发**：go（真线程）+ chan（无界通道）
- **C 交互**：内联 C、extern "C"、import c "头.h"
- **错误处理**：Result/Option + ?、try/expt/fily
- **智能诊断**：稳定错误码、ariadne 渲染、中英双语、"是否想用 X？"建议
- **模块**：import math（内置）、import a.b（用户）、import "x.gt"（文件）

---

## 快速上手

    REM 构建（vendored 工具链，含标准库）
    build.bat

    REM 运行 / 编译 / 检查 / lint
    target\release\gtc.exe --run examples\hello.gt
    target\release\gtc.exe examples\hello.gt -o hello.exe -O 2
    target\release\gtc.exe --check examples\hello.gt
    target\release\gtc.exe --lint --strict examples\hello.gt

---

## 性能

loop_sum(2e8)（2 亿次迭代，含溢出检查 + 范围分析）：

| 实现 | fib(35) | loop_sum(2e8) |
|---|---|---|
| C（clang -O2） | ~26 ms | ~5.5 ms |
| GTLang（-O2，有检查） | ~40 ms | 5.9 ms |
| Python 3.12 | ~1300 ms | ~6788 ms |

GTLang 默认（安全）模式在紧循环上追平 C —— 流敏感范围分析证明安全即省略溢出检查。

---

## 文档

| 文档 | 内容 |
|---|---|
| doc/LANGUAGE.md | 语言手册（17 章） |
| doc/PERFORMANCE.md | 性能 |
| doc/COMPARISON.md | GTLang vs Python |
| doc/bench.md | 基准 |
| doc/syntax_status.md | 语法/特性状态 |

---

## 测试

    cargo test --release

613 测试（17 单元 + 96 双后端一致性 + 500 前端批量）。
