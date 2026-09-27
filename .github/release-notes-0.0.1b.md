<path>.github/release-notes-0.0.1b.md</path>
<type>file</type>
<content>
1: # GTLang v0.0.1b — Early Preview
2: 
3: > ⚠️ **Early-stage project.** APIs, syntax, and the standard library may change
4: > without notice. Bugs and missing features are expected.
5: >
6: > 🪟 **Windows x64 only (for now).** Linux/macOS support is not yet available.
7: 
8: **GTLang** is a statically-typed, compiled, expression-oriented language with
9: first-class Chinese identifiers, dual backends (LLVM + Cranelift), and memory
10: safety.
11: 
12: ---
13: 
14: ## 📦 What's in this release
15: 
16: \`gtlang-0.0.1b-win-x64.zip\` (10.4 MB) — a portable, no-install bundle:
17: 
18: | Path | Contents |
19: |---|---|
20: | \`gtc.exe\` | Compiler & interpreter |
21: | \`gtfmt.exe\` | Formatter |
22: | \`lib/\` | Standard library (\`math.dll\` / \`string.dll\` + import libs) |
23: | \`runtime/\` | Built-in C runtime (\`gt_rt.c\`) |
24: | \`tcc/\` | TCC (\`libtcc.dll\` + headers) for inline C blocks |
25: 
26: **Not bundled**: \`clang\` / \`lld-link\` (≈176 MB). GTLang locates them on your
27: system \`PATH\` (or via the \`GTC_CLANG\` env var). Install LLVM from
28: <https://releases.llvm.org> (≥ 15).
29: 
30: ---
31: 
32: ## 🚀 Quick start
33: 
34: \`\`\`bat
35: REM 1. Extract the zip anywhere, e.g. D:\gtlang
36: REM 2. Make sure LLVM (clang) is on PATH, then:
37: D:\gtlang\gtc.exe --run hello.gt
38: D:\gtlang\gtc.exe hello.gt -o hello.exe -O 2
39: \`\`\`
40: 
41: \`hello.gt\`:
42: 
43: \`\`\`gt
44: fn main() {
45:     put("你好，世界！")
46: }
47: \`\`\`
48: 
49: ---
50: 
51: ## 🆕 What's new in 0.0.1b

- **Interactive I/O** — new builtins `read_line()` and `read_int()` (aliases: `input`, `readline`, `readint`), implemented on **both** backends.
- **CLI games** — `examples/game_2048.gt` is now fully interactive (1/2/3/4 to move, 0 to quit); `examples/game_of_life.gt` renders Conway's Game of Life.
- **Parser fix** — `for x in 0..CONST { ... }` no longer mis-parses the upper bound as a struct literal.
- Assorted README/doc improvements.

## ✨ Highlights
52: 
53: - **Dual backend** — LLVM (native exe) + Cranelift (JIT), byte-for-byte identical output
54: - **Static types** with bidirectional inference; bounds/overflow/divide-by-zero checks
55: - **Ownership & borrow checking** (flow-sensitive NLL)
56: - **Generics** (monomorphization), traits, **dyn Trait** (vtable dispatch)
57: - **Enums with payloads** + exhaustive \`match\` (ranges, guards, OR)
58: - **Closures**, higher-order functions, default & named args
59: - **Macros** — declarative \`macro\` + \`@derive(Eq, Clone, Debug, Default, Hash, Ord, PartialEq, Display)\`
60: - **Concurrency** — \`go\` (real threads) + \`chan\` (unbounded channels)
61: - **C interop** — inline C blocks, \`extern "C"\`, \`import c "header.h"\`
62: - **Smart diagnostics** — stable codes, bilingual, "did you mean X?"
63: - **Performance** — tight loops match C via range-analysis-elided overflow checks
64: 
65: ---
66: 
67: ## 🧪 Tests
68: 
69: **613 tests** (17 unit + 96 dual-backend consistency + 500 frontend bulk) — all green, 0 warnings.
70: 
71: ---
72: 
73: ## 📚 Documentation
74: 
75: - [Language Reference](doc/LANGUAGE.md)
76: - [Performance](doc/PERFORMANCE.md)
77: - [vs Python](doc/COMPARISON.md)
78: - [Benchmarks](doc/bench.md)
79: - [Syntax status](doc/syntax_status.md)
80: 
81: All docs are bilingual (Chinese + English).
82: 
83: ---
84: 
85: # GTLang v0.0.1b — 早期预览
86: 
87: > ⚠️ **早期阶段项目。** API、语法与标准库可能随时变动，BUG 与缺失功能属正常现象。
88: >
89: > 🪟 **目前仅支持 Windows x64。** 尚不支持 Linux/macOS。
90: 
91: **GTLang** 是一门静态类型、编译型、表达式导向的编程语言，原生支持中文标识符，
92: 双后端（LLVM + Cranelift），内存安全。
93: 
94: ---
95: 
96: ## 📦 本次发布内容
97: 
98: \`gtlang-0.0.1b-win-x64.zip\`（10.4 MB）—— 免安装便携包：
99: 
100: | 路径 | 内容 |
101: |---|---|
102: | \`gtc.exe\` | 编译器 & 解释器 |
103: | \`gtfmt.exe\` | 格式化器 |
104: | \`lib/\` | 标准库（\`math.dll\` / \`string.dll\` + 导入库） |
105: | \`runtime/\` | 内置 C 运行时（\`gt_rt.c\`） |
106: | \`tcc/\` | TCC（\`libtcc.dll\` + 头文件），用于内联 C 块 |
107: 
108: **未捆绑**：\`clang\` / \`lld-link\`（约 176 MB）。GTLang 会从系统 \`PATH\`
109: （或 \`GTC_CLANG\` 环境变量）定位它们。请安装 LLVM（≥ 15）：<https://releases.llvm.org>。
110: 
111: ---
112: 
113: ## 🚀 快速上手
114: 
115: \`\`\`bat
116: REM 1. 解压到任意目录，如 D:\gtlang
117: REM 2. 确保 LLVM (clang) 在 PATH 上，然后：
118: D:\gtlang\gtc.exe --run hello.gt
119: D:\gtlang\gtc.exe hello.gt -o hello.exe -O 2
120: \`\`\`
121: 
122: \`hello.gt\`：
123: 
124: \`\`\`gt
125: fn main() {
126:     put("你好，世界！")
127: }
128: \`\`\`
129: 
130: ---
131: 
132: ## 🆕 0.0.1b 新增

- **交互式 I/O** —— 新增内置 `read_line()` 与 `read_int()`（别名：`input`、`readline`、`readint`），双后端均实现。
- **CLI 游戏** —— `examples/game_2048.gt` 现为完整交互（1/2/3/4 移动，0 退出）；`examples/game_of_life.gt` 渲染康威生命游戏。
- **语法修复** —— `for x in 0..常量 { ... }` 不再把上界误解析为结构体字面量。
- README / 文档多处改进。

## ✨ 亮点
133: 
134: - **双后端** —— LLVM（原生 exe）+ Cranelift（JIT），输出逐字节一致
135: - **静态类型** + 双向推断；边界/溢出/除零检查
136: - **所有权与借用检查**（流敏感 NLL）
137: - **泛型**（单态化）、trait、**dyn Trait**（vtable 动态分发）
138: - **带载荷枚举** + 穷尽 \`match\`（范围/守卫/OR）
139: - **闭包**、高阶函数、默认/命名参数
140: - **宏** —— 声明式 \`macro\` + \`@derive(Eq, Clone, Debug, Default, Hash, Ord, PartialEq, Display)\`
141: - **并发** —— \`go\`（真线程）+ \`chan\`（无界通道）
142: - **C 交互** —— 内联 C、\`extern "C"\`、\`import c "头.h"\`
143: - **智能诊断** —— 稳定错误码、中英双语、"是否想用 X？"
144: - **性能** —— 紧循环追平 C（范围分析省略冗余溢出检查）
145: 
146: ---
147: 
148: ## 🧪 测试
149: 
150: **613 个测试**（17 单元 + 96 双后端一致性 + 500 前端批量）—— 全绿，0 warning。
151: 
152: ---
153: 
154: ## 📚 文档
155: 
156: - [语言手册](doc/LANGUAGE.md)
157: - [性能](doc/PERFORMANCE.md)
158: - [对比 Python](doc/COMPARISON.md)
159: - [基准](doc/bench.md)
160: - [语法状态](doc/syntax_status.md)
161: 
162: 所有文档均有**中文 + 英文**两个版本。
163: 
164: 

(End of file - total 164 lines)
</content>