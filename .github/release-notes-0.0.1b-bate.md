# GTLang 0.0.1b — Early Preview (bate)

> ⚠️ **Early-stage project.** APIs, syntax, and the standard library may change without notice.
>
> 🪟 **Windows x64 only (for now).**

## 🆕 What's new in 0.0.1b

- **Interactive I/O** — new builtins `read_line()` and `read_int()` (aliases: `input`, `readline`, `readint`), implemented on **both** backends.
- **CLI games** — `examples/game_2048.gt` is now fully interactive (1/2/3/4 to move, 0 to quit); `examples/game_of_life.gt` renders Conway's Game of Life.
- **Parser fix** — `for x in 0..CONST { ... }` no longer mis-parses the upper bound as a struct literal.
- Assorted README/doc improvements.

## 📦 Bundle

`gtlang-0.0.1b-bate-win-x64.zip` (~10.8 MB) — portable, no-install:

- `gtc.exe` / `gtfmt.exe` — compiler & interpreter / formatter
- `lib/` — standard library (`math.dll` / `string.dll` + import libs)
- `runtime/` — built-in C runtime (`gt_rt.c`)
- `tcc/` — TCC (inline C blocks)
- `examples/` — sample programs (incl. the games)

**Not bundled**: `clang` / `lld-link` (≈176 MB). Install LLVM (≥15) and put it on `PATH` (or set `GTC_CLANG`).

## 🚀 Quick start

```bat
D:\gtlang\gtc.exe --run hello.gt
D:\gtlang\gtc.exe --run examples\game_2048.gt
```

---

# GTLang 0.0.1b —— 早期预览（bate）

> ⚠️ **早期阶段项目。** API、语法与标准库可能随时变动。
>
> 🪟 **目前仅支持 Windows x64。**

## 🆕 0.0.1b 新增

- **交互式 I/O** —— 新增内置 `read_line()` 与 `read_int()`（别名：`input`、`readline`、`readint`），**双后端**均实现。
- **CLI 游戏** —— `examples/game_2048.gt` 现为完整交互（1/2/3/4 移动，0 退出）；`examples/game_of_life.gt` 渲染康威生命游戏。
- **语法修复** —— `for x in 0..常量 { ... }` 不再把上界误解析为结构体字面量。
- README / 文档多处改进。

## 📦 包内容

`gtlang-0.0.1b-bate-win-x64.zip`（约 10.8 MB）—— 免安装便携包：

- `gtc.exe` / `gtfmt.exe` —— 编译器 & 解释器 / 格式化器
- `lib/` —— 标准库（`math.dll` / `string.dll` + 导入库）
- `runtime/` —— 内置 C 运行时（`gt_rt.c`）
- `tcc/` —— TCC（内联 C 块）
- `examples/` —— 示例程序（含游戏）

**未捆绑**：`clang` / `lld-link`（约 176 MB）。请安装 LLVM（≥15）并加入 `PATH`（或设 `GTC_CLANG`）。

## 🚀 快速上手

```bat
D:\gtlang\gtc.exe --run hello.gt
D:\gtlang\gtc.exe --run examples\game_2048.gt
```
