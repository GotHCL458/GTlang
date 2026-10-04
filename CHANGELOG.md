# Changelog

All notable changes to GTLang. Bilingual (EN / 中文).

## [0.0.1d] - 2026-10-04

### Added
- **Named functions as first-class values** — pass a top-level `fn` name where a function value is expected (direct arg / let / `if` / `match` / `return` / container). Hoist rewrites it to an anonymous closure.
- **Any expression as callee** — `fs[0](5)`, `(f)(10)` lower to indirect calls.
- **Closures capturing containers/strings** — a closure body using `len(xs)` now gets the captured variable's type from the call-site captures.
- **Closures capturing outer params** — `return |x| g(f(x))` captures `g`/`f`; lifted closure bodies also get closure-call rewriting.
- **AST traversal completeness tests** — `each_expr` and `convert_closure_calls_expr` are asserted to cover every `ExprKind` variant.
- **Friendly error for `loop {`** without a count (E303).

### Fixed
- **JIT/AOT newline mismatch on Windows** — CRT stdout is now binary in both runtimes; the consistency test no longer normalizes CRLF (so this class of bug is caught).
- **Calling a closure returned by a function**; **higher-order closure params** (`g(f(x))`); **closure called inside `if`/`match`/etc.**
- **JIT `gen_call_value`** takes the callee's closure signature (was `e.ty`).
- **`mono` substitutes the inferred `ret_ty`** (fixes generic multi-instantiation type mixing).
- **Plain assignment no longer changes a variable's type when the RHS is Unknown.**
- **Deep nesting / long chains** report a syntax error instead of overflowing the stack; `i64::MIN` parses.
- **`gtlib/random` RNG seed** perturbed per thread (concurrent `go` threads no longer share a sequence).

### Changed
- Split `sema.rs` (58→21 KB, + `sema_infer.rs`) and `codegen/mod.rs` (51→18 KB, + `codegen/stmt.rs`); all `.rs` < 50 KB.
- `tests/` tracked in the repo again.

### Tests
- **626 tests**: 25 unit + 101 dual-backend consistency + 500 frontend bulk. 0 warnings.

## [0.0.1c] - 2026-09-28

### Added
- **-v flag** — print version (gtc -v / gtc --version).
- **Stdlib os** — getcwd, getenv, setenv, path_exists, is_file, is_dir, getsize, listdir, basename, dirname, path_join, abspath, mkdir, rmdir, os_remove, system.
- **Stdlib json** — json_dumps, json_loads, json_dump, json_load.
- **Stdlib toml** — toml_loads, toml_load.
- **Iterable str / set / map** in for v in ...
- Build scripts auto-discover stdlib modules.

### Changed
- build.bat / build_res.bat rewritten (system toolchain, auto-discovery, portable res/).
- README hero: 3-line pitch + benchmark chart.
- Docs: stdlib module reference added (§14b).

### Fixed
- Parser: for x in 0..CONST { ... } no longer mis-parses the bound as a struct literal.
- Docs: restored code-block indentation.
- LLVM: correct GEP for inlined list-element load.

### Performance
- Inlined list[i] in for v in list (-18% on 1M-element loop).
- Hoisted list_len out of the loop.

---

## [0.0.1d] - 2026-10-04（中文）

### 新增
- **命名函数作一等值** —— 顶层 `fn` 名可直接当函数值用（直接传参 / 存变量 / `if` / `match` / `return` / 容器）；hoist 会把它改写成匿名闭包。
- **任意表达式作 callee** —— `fs[0](5)`、`(f)(10)` 降级为间接调用。
- **闭包捕获容器/字符串** —— 闭包体里 `len(xs)` 现在能从调用点的捕获值拿到类型。
- **闭包捕获外层参数** —— `return |x| g(f(x))` 会捕获 `g`/`f`；提升出的闭包体也会做闭包调用改写。
- **AST 遍历完备性测试** —— 断言 `each_expr` 与 `convert_closure_calls_expr` 覆盖每个 `ExprKind` 变体。
- **`loop {` 友好错误**（缺计数，E303）。

### 修复
- **Windows 下 JIT/AOT 换行不一致** —— 两个运行时的 CRT stdout 都设为二进制；一致性测试不再归一化 CRLF（该类 bug 会被抓到）。
- **调用"函数返回的闭包"**；**高阶闭包参数**（`g(f(x))`）；**闭包在 `if`/`match` 等里调用**。
- **JIT `gen_call_value`** 取被调闭包的签名（原来用 `e.ty`，函数工厂场景错误）。
- **`mono` 替换推断出的 `ret_ty`**（修泛型多实例化的类型混用）。
- **纯赋值在 RHS 类型未知时不再改变量类型**。
- **深嵌套 / 长链**改为报语法错而非爆栈；`i64::MIN` 可解析。
- **`gtlib/random` 的 RNG 种子**按线程扰动（并发 `go` 不再共享序列）。

### 变更
- 拆 `sema.rs`（58→21 KB，+ `sema_infer.rs`）与 `codegen/mod.rs`（51→18 KB，+ `codegen/stmt.rs`）；全部 `.rs` < 50 KB。
- `tests/` 重新纳入仓库。

### 测试
- **626 个测试**：25 单元 + 101 双后端一致性 + 500 前端批量。0 warning。

---

## [0.0.1c] - 2026-09-28（中文）

### 新增
- **-v 参数** —— 打印版本（gtc -v / gtc --version）。
- **标准库 os** —— 见上（16 个函数）。
- **标准库 json** —— json_dumps / json_loads / json_dump / json_load。
- **标准库 toml** —— toml_loads / toml_load。
- **for v in str / set / map** 可迭代。
- build 脚本自动发现 stdlib 模块。

### 变更
- **build.bat / build_res.bat 重写**（系统工具链 + 自动发现 + 便携 res）。
- README 首屏：3 行亮点 + 基准图。
- 文档：新增标准库模块参考（§14b）。

### 修复
- 语法：for x in 0..常量 { ... } 不再把上界误解析为结构体字面量。
- 文档代码块缩进恢复。
- LLVM：内联 list 取址的 GEP 修正。

### 性能
- **for v in list 内联 list[i]**（100 万循环 -18%）。
- **list_len 循环外提**。

---

## [0.0.1b] - 2026-09-27

### Added
- read_line() / read_int() builtins (dual backend).
- CLI games: game_2048.gt (interactive), game_of_life.gt.

### Fixed
- Parser: for x in 0..CONST { ... }.

## [0.0.1a] - 2026-09-26

First public preview: dual backend, generics, traits, dyn Trait, enums, match, closures, macros, @derive, go/chan, C interop, smart diagnostics.
