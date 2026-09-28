# Changelog

All notable changes to GTLang. Bilingual (EN / 中文).

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
