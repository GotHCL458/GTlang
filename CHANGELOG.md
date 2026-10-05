# Changelog

All notable changes to GTLang. Bilingual (EN / 中文).

## [0.0.1d] - 2026-10-04

### Added
- **`x or 默认值`** —— Option 默认值运算符：`Some(v)` 取 `v`，`None` 取默认值（展开为 `match`）。`v := o or 0`。
- **字符串 Unicode 转义** —— 字符串/字符字面量支持 `\uXXXX` 与 `\u{...}`。
- **Named functions as first-class values** — pass a top-level `fn` name where a function value is expected (direct arg / let / `if` / `match` / `return` / container). Hoist rewrites it to an anonymous closure.
- **Any expression as callee** — `fs[0](5)`, `(f)(10)` lower to indirect calls.
- **Closures capturing containers/strings** — a closure body using `len(xs)` now gets the captured variable's type from the call-site captures.
- **Closures capturing outer params** — `return |x| g(f(x))` captures `g`/`f`; lifted closure bodies also get closure-call rewriting.
- **AST traversal completeness tests** — `each_expr` and `convert_closure_calls_expr` are asserted to cover every `ExprKind` variant.
- **Friendly error for `loop {`** without a count (E303).
- **`x or default`** — Option default-value operator: `Some(v)` yields `v`, `None` yields the fallback (rewritten to a `match`). `v := o or 0`.
- **String Unicode escapes** — `\uXXXX` and `\u{...}` in string/char literals.
- **Match ident-binding pattern** — `match v { n if n > 0 => ... }` binds the subject.
- **Nested destructuring** — `Some(Some(v))`, `Ok(Some(v))`, `E::A(Some(v))` in `match`.
- **Automatic deref for `&T`/`&mut T`** — field access/assignment and method calls (`a.x`, `a.方法()`) and borrow params.
- **`dyn Trait` auto-boxing** — passing a `struct`/`enum` to a `dyn Trait` parameter boxes automatically.
- **`x or default` also works for `Result`** (not just `Option`).
- **`Option[T]` bracket type syntax** now parsed correctly.

### Fixed
- **JIT/AOT newline mismatch on Windows** — CRT stdout is now binary in both runtimes; the consistency test no longer normalizes CRLF (so this class of bug is caught).
- **Calling a closure returned by a function**; **higher-order closure params** (`g(f(x))`); **closure called inside `if`/`match`/etc.**
- **JIT `gen_call_value`** takes the callee's closure signature (was `e.ty`).
- **`mono` substitutes the inferred `ret_ty`** (fixes generic multi-instantiation type mixing).
- **Plain assignment no longer changes a variable's type when the RHS is Unknown.**
- **Deep nesting / long chains** report a syntax error instead of overflowing the stack; `i64::MIN` parses.
- **`gtlib/random` RNG seed** perturbed per thread (concurrent `go` threads no longer share a sequence).
- **Pipe `|>` to a non-callable RHS** no longer silently drops the LHS.
- **`if` on the same line as the previous statement** is no longer misparsed as a ternary (elif chains work).
- **match guards** were swallowed by the ternary branch.
- **`"${x}"` (single interpolation)** no longer degrades to the inner expression `x`.
- **Non-enum match exhaustiveness** always requires `_` (empty match no longer passes).
- **Composite-type match patterns** are rejected in both backends (was invalid IR / silent fallthrough).
- **Nested enum payload destructuring** no longer crashes JIT / mis-binds inner tags.
- **`own`** no longer reports a false use-after-move when a match binding shadows an outer name.
- **Method calls on borrows** (`a.方法()` where `a: &T`) in sema/mono/LLVM.
- **`ariadne` spans** align to UTF-8 char boundaries (truncated multi-byte source no longer panics).
- **Inline** substitutes the receiver of `recv.method()` (else abandons inlining).
- **`dyn` vtable calls** return `str`/`bool` with the correct LLVM type.
- **`opt::collect_calls`** / **`mono::auto_box_args`** cover `MethodOn`/`Borrow`/`DynBox`/`Slice`/`ListComp`/`EnumLit`/`TupleLit`.
- **Cross-module type prefixing** recurses into `Dyn`/`Enum`/`Result`/`Option`/`Tuple`/`Ref`/`Closure`.
- **`Result` with `x or y`** no longer binds the Err payload for a `Some(v)` arm.

### Changed
- Split `sema.rs` (58→21 KB, + `sema_infer.rs`) and `codegen/mod.rs` (51→18 KB, + `codegen/stmt.rs`); all `.rs` < 50 KB.
- `tests/` tracked in the repo again.

### OS / Bare-metal（新增）
- **`gtc --bare --target x86_64|x86_32|x86_16`** —— 裸机目标（无 CRT/运行时）。
- **`gtc --bare --boot`** —— 一键生成可启动镜像（引导库 + 内核 + 运行时）。
- **`gtc --asm16`** —— 内置 x86-16 汇编器（与 nasm 逐字节一致；`res/bin/nasm.exe` 随发行包）。
- **`gtc --asm16gen`** —— AST → x86-16 机器码后端（`codegen_asm16`：函数/算术/if/while/for/match/break/continue/字符串/数组）。
- **`gtlib: boot`** —— 完整裸机引导库（30 函数 × 10 组：serial/screen/keyboard/memory/time/system/disk/port/interrupt/info）。
- **三套引导实现**：`os/boot/stage1.asm`+`stage2.asm`（MBR→32 位保护模式→64 位长模式）、`rt_bare.c`（32/64 位运行时）、`boot16.asm`（16 位，`gtc --asm16` 自组装）。
- **中断/PIC/PIT/键盘/ATA 磁盘** 均已在 QEMU 验证。

### Tests
- **825 tests**: 143 unit + 160 dual-backend consistency + 522 frontend bulk. 0 warnings.

## [0.0.1c] - 2026-09-28

### Added
- **`x or 默认值`** —— Option 默认值运算符：`Some(v)` 取 `v`，`None` 取默认值（展开为 `match`）。`v := o or 0`。
- **字符串 Unicode 转义** —— 字符串/字符字面量支持 `\uXXXX` 与 `\u{...}`。
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
- **match 裸标识符绑定** —— `match v { n if n > 0 => ... }` 绑定主体值。
- **嵌套解构** —— `match` 里支持 `Some(Some(v))`、`Ok(Some(v))`、`E::A(Some(v))`。
- **`&T`/`&mut T` 自动解引用** —— 字段访问/赋值、方法调用（`a.x`、`a.方法()`）与借用参数。
- **`dyn Trait` 自动装箱** —— `struct`/`enum` 传给 `dyn Trait` 形参时自动装箱。
- **`x or 默认值` 也支持 `Result`**（不再仅限 `Option`）。
- **`Option[T]` 方括号类型语法**正确解析。

### 修复
- **Windows 下 JIT/AOT 换行不一致** —— 两个运行时的 CRT stdout 都设为二进制；一致性测试不再归一化 CRLF（该类 bug 会被抓到）。
- **调用"函数返回的闭包"**；**高阶闭包参数**（`g(f(x))`）；**闭包在 `if`/`match` 等里调用**。
- **JIT `gen_call_value`** 取被调闭包的签名（原来用 `e.ty`，函数工厂场景错误）。
- **`mono` 替换推断出的 `ret_ty`**（修泛型多实例化的类型混用）。
- **纯赋值在 RHS 类型未知时不再改变量类型**。
- **深嵌套 / 长链**改为报语法错而非爆栈；`i64::MIN` 可解析。
- **`gtlib/random` 的 RNG 种子**按线程扰动（并发 `go` 不再共享序列）。
- **`|>` 右侧非可调用**不再静默丢弃 LHS。
- **`if` 与前一语句同行**不再被误判为三元（elif 链可用）。
- **match 守卫**曾被三元分支吞掉。
- **`"${x}"`（单个插值）**不再退化为内部表达式 `x`。
- **非枚举 match 的穷尽性**始终要求 `_`（空 match 不再通过）。
- **复合类型 match 模式**双端明确拒绝（原来生成非法 IR / 静默走 `_`）。
- **enum 载荷嵌套解构**不再令 JIT 崩溃 / 内层 tag 误判。
- **`own`** 在 match 绑定遮蔽外层同名变量时不再误报 use-after-move。
- **借用上的方法调用**（`a.方法()`，`a: &T`）在 sema/mono/LLVM 全链路修复。
- **`ariadne` 的 Span** 对齐 UTF-8 字符边界（截断多字节源码不再 panic）。
- **内联**替换 `recv.method()` 的接收者（否则放弃内联）。
- **`dyn` vtable 调用**返回 `str`/`bool` 时使用正确的 LLVM 类型。
- **`opt::collect_calls`** / **`mono::auto_box_args`** 补齐 `MethodOn`/`Borrow`/`DynBox`/`Slice`/`ListComp`/`EnumLit`/`TupleLit`。
- **跨模块类型前缀**递归 `Dyn`/`Enum`/`Result`/`Option`/`Tuple`/`Ref`/`Closure`。
- **`Result` 用 `x or y`** 时，`Some(v)` 分支不再绑定 Err 载荷。

### 变更
- 拆 `sema.rs`（58→21 KB，+ `sema_infer.rs`）与 `codegen/mod.rs`（51→18 KB，+ `codegen/stmt.rs`）；全部 `.rs` < 50 KB。
- `tests/` 重新纳入仓库。

### OS / 裸机（新增）
- **`gtc --bare --target x86_64|x86_32|x86_16`** —— 裸机目标（无 CRT/运行时）。
- **`gtc --bare --boot`** —— 一键生成可启动镜像。
- **`gtc --asm16`** —— 内置 x86-16 汇编器（与 nasm 逐字节一致）。
- **`gtc --asm16gen`** —— AST → x86-16 机器码后端。
- **`gtlib: boot`** —— 完整裸机引导库（30 函数 × 10 组）。
- **三套引导实现**：MBR→保护模式→长模式（x86_64）、16 位自组装。
- **中断/PIC/PIT/键盘/ATA 磁盘** 均已在 QEMU 验证。

### 测试
- **825 个测试**：143 单元 + 160 双后端一致性 + 522 前端批量。0 warning。

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
- **`x or 默认值`** —— Option 默认值运算符：`Some(v)` 取 `v`，`None` 取默认值（展开为 `match`）。`v := o or 0`。
- **字符串 Unicode 转义** —— 字符串/字符字面量支持 `\uXXXX` 与 `\u{...}`。
- read_line() / read_int() builtins (dual backend).
- CLI games: game_2048.gt (interactive), game_of_life.gt.

### Fixed
- Parser: for x in 0..CONST { ... }.

## [0.0.1a] - 2026-09-26

First public preview: dual backend, generics, traits, dyn Trait, enums, match, closures, macros, @derive, go/chan, C interop, smart diagnostics.
