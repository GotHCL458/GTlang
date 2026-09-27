# GTLang v0.0.1a — Early Preview

> ⚠️ **Early-stage project.** APIs, syntax, and the standard library may change
> without notice. Bugs and missing features are expected.
>
> 🪟 **Windows x64 only (for now).** Linux/macOS support is not yet available.

**GTLang** is a statically-typed, compiled, expression-oriented language with
first-class Chinese identifiers, dual backends (LLVM + Cranelift), and memory
safety.

---

## 📦 What's in this release

\`gtlang-0.0.1a-win-x64.zip\` (10.4 MB) — a portable, no-install bundle:

| Path | Contents |
|---|---|
| \`gtc.exe\` | Compiler & interpreter |
| \`gtfmt.exe\` | Formatter |
| \`lib/\` | Standard library (\`math.dll\` / \`string.dll\` + import libs) |
| \`runtime/\` | Built-in C runtime (\`gt_rt.c\`) |
| \`tcc/\` | TCC (\`libtcc.dll\` + headers) for inline C blocks |

**Not bundled**: \`clang\` / \`lld-link\` (≈176 MB). GTLang locates them on your
system \`PATH\` (or via the \`GTC_CLANG\` env var). Install LLVM from
<https://releases.llvm.org> (≥ 15).

---

## 🚀 Quick start

\`\`\`bat
REM 1. Extract the zip anywhere, e.g. D:\gtlang
REM 2. Make sure LLVM (clang) is on PATH, then:
D:\gtlang\gtc.exe --run hello.gt
D:\gtlang\gtc.exe hello.gt -o hello.exe -O 2
\`\`\`

\`hello.gt\`:

\`\`\`gt
fn main() {
    put("你好，世界！")
}
\`\`\`

---

## ✨ Highlights

- **Dual backend** — LLVM (native exe) + Cranelift (JIT), byte-for-byte identical output
- **Static types** with bidirectional inference; bounds/overflow/divide-by-zero checks
- **Ownership & borrow checking** (flow-sensitive NLL)
- **Generics** (monomorphization), traits, **dyn Trait** (vtable dispatch)
- **Enums with payloads** + exhaustive \`match\` (ranges, guards, OR)
- **Closures**, higher-order functions, default & named args
- **Macros** — declarative \`macro\` + \`@derive(Eq, Clone, Debug, Default, Hash, Ord, PartialEq, Display)\`
- **Concurrency** — \`go\` (real threads) + \`chan\` (unbounded channels)
- **C interop** — inline C blocks, \`extern "C"\`, \`import c "header.h"\`
- **Smart diagnostics** — stable codes, bilingual, "did you mean X?"
- **Performance** — tight loops match C via range-analysis-elided overflow checks

---

## 🧪 Tests

**613 tests** (17 unit + 96 dual-backend consistency + 500 frontend bulk) — all green, 0 warnings.

---

## 📚 Documentation

- [Language Reference](doc/LANGUAGE.md)
- [Performance](doc/PERFORMANCE.md)
- [vs Python](doc/COMPARISON.md)
- [Benchmarks](doc/bench.md)
- [Syntax status](doc/syntax_status.md)

All docs are bilingual (Chinese + English).

---

# GTLang v0.0.1a — 早期预览

> ⚠️ **早期阶段项目。** API、语法与标准库可能随时变动，BUG 与缺失功能属正常现象。
>
> 🪟 **目前仅支持 Windows x64。** 尚不支持 Linux/macOS。

**GTLang** 是一门静态类型、编译型、表达式导向的编程语言，原生支持中文标识符，
双后端（LLVM + Cranelift），内存安全。

---

## 📦 本次发布内容

\`gtlang-0.0.1a-win-x64.zip\`（10.4 MB）—— 免安装便携包：

| 路径 | 内容 |
|---|---|
| \`gtc.exe\` | 编译器 & 解释器 |
| \`gtfmt.exe\` | 格式化器 |
| \`lib/\` | 标准库（\`math.dll\` / \`string.dll\` + 导入库） |
| \`runtime/\` | 内置 C 运行时（\`gt_rt.c\`） |
| \`tcc/\` | TCC（\`libtcc.dll\` + 头文件），用于内联 C 块 |

**未捆绑**：\`clang\` / \`lld-link\`（约 176 MB）。GTLang 会从系统 \`PATH\`
（或 \`GTC_CLANG\` 环境变量）定位它们。请安装 LLVM（≥ 15）：<https://releases.llvm.org>。

---

## 🚀 快速上手

\`\`\`bat
REM 1. 解压到任意目录，如 D:\gtlang
REM 2. 确保 LLVM (clang) 在 PATH 上，然后：
D:\gtlang\gtc.exe --run hello.gt
D:\gtlang\gtc.exe hello.gt -o hello.exe -O 2
\`\`\`

\`hello.gt\`：

\`\`\`gt
fn main() {
    put("你好，世界！")
}
\`\`\`

---

## ✨ 亮点

- **双后端** —— LLVM（原生 exe）+ Cranelift（JIT），输出逐字节一致
- **静态类型** + 双向推断；边界/溢出/除零检查
- **所有权与借用检查**（流敏感 NLL）
- **泛型**（单态化）、trait、**dyn Trait**（vtable 动态分发）
- **带载荷枚举** + 穷尽 \`match\`（范围/守卫/OR）
- **闭包**、高阶函数、默认/命名参数
- **宏** —— 声明式 \`macro\` + \`@derive(Eq, Clone, Debug, Default, Hash, Ord, PartialEq, Display)\`
- **并发** —— \`go\`（真线程）+ \`chan\`（无界通道）
- **C 交互** —— 内联 C、\`extern "C"\`、\`import c "头.h"\`
- **智能诊断** —— 稳定错误码、中英双语、"是否想用 X？"
- **性能** —— 紧循环追平 C（范围分析省略冗余溢出检查）

---

## 🧪 测试

**613 个测试**（17 单元 + 96 双后端一致性 + 500 前端批量）—— 全绿，0 warning。

---

## 📚 文档

- [语言手册](doc/LANGUAGE.md)
- [性能](doc/PERFORMANCE.md)
- [对比 Python](doc/COMPARISON.md)
- [基准](doc/bench.md)
- [语法状态](doc/syntax_status.md)

所有文档均有**中文 + 英文**两个版本。

