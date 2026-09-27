<div align="center">

<img src="GTlangLOGO.png" alt="GTLang" width="160">

# GTLang

**GTLang 是一门静态类型、编译型、表达式导向的编程语言，采用双后端（LLVM + Cranelift），原生支持中文标识符 —— 且默认（带安全检查）在紧循环上追平 C。**

[![Tests](https://img.shields.io/badge/tests-613%20passed-brightgreen)]()
[![Backends](https://img.shields.io/badge/backends-LLVM%20%2B%20Cranelift-blue)]()
[![Warnings](https://img.shields.io/badge/warnings-0-brightgreen)]()
[![License](https://img.shields.io/badge/license-MIT-lightgrey)]()

中文 | [English](README.md)

<img src="docs/benchmark.svg" alt="GTLang 基准：loop_sum(2e8) —— GTLang 5.9 ms，C 5.5 ms，Python 6788 ms" width="760">

> ⚠️ **早期阶段项目。** API、语法与标准库可能随时变动，BUG 与缺失功能属正常现象。
>
> 🪟 **目前仅支持 Windows。** 当前构建与运行时面向 Windows x64，尚不支持 Linux/macOS。

</div>

---

## 为什么选 GTLang？

- **默认就快。** 流敏感范围分析在可证明安全时省略溢出检查，带检查的算术仍能追平 C（`loop_sum(2×10⁸)`：**5.9 ms**，与 `clang -O2` 相差 7%）。
- **同一 AST，两个后端。** 同一份源码既可编译为原生可执行文件（LLVM），也可内存执行（Cranelift JIT），**输出逐字节一致**。
- **安全而不将就。** 静态类型、所有权/借用检查（流敏感 NLL）、边界/溢出/除零检查、穷尽 `match`。
- **原生双语。** 中文标识符（`计数器`、`累加`、`点`）是一等公民，诊断信息本地化。
- **生产力不缺。** 泛型、trait、`dyn Trait`、带载荷枚举、闭包、宏、`@derive`、真线程、通道、C 交互。

---

## 目录
25: 
26: - [GTLang 是什么？](#gtlang-是什么)
27: - [亮点](#亮点)
28: - [快速上手](#快速上手)
29: - [代码一览](#代码一览)
30: - [语言特性](#语言特性)
31: - [双后端](#双后端)
32: - [性能](#性能)
33: - [CLI 速查](#cli-速查)
34: - [目录结构](#目录结构)
35: - [架构](#架构)
36: - [测试](#测试)
37: - [文档](#文档)
38: - [常见问题](#常见问题)
39: 
40: ---
41: 
42: ## GTLang 是什么？
43: 
44: GTLang 是一门**静态类型**、**编译型**、**表达式导向**的编程语言，融合了：
45: 
46: - **Rust 的内存安全** —— 所有权、借用、流敏感 NLL、边界/溢出/除零检查
47: - **C 级性能** —— 经 LLVM 生成原生机器码；紧循环可与 C 持平
48: - **Python 式简洁** —— 表达式导向、类型标注可选、闭包、列表推导
49: - **原生中文标识符** —— `计数器`、`累加`、`点` 都是合法名字
50: - **双后端** —— 同一份 AST 既可编译为原生可执行文件（LLVM），也可内存执行（Cranelift JIT），**输出逐字节一致**（测试保证）
51: 
52: ---
53: 
54: ## 亮点
55: 
56: | | 特性 |
57: |---|---|
58: | 🧠 | **静态类型** + 双向推断 |
59: | 🛡️ | **内存安全** —— 所有权、借用、NLL、边界/溢出检查 |
60: | ⚡ | **双后端** —— LLVM（发布）+ Cranelift（JIT），语义一致 |
61: | 🌏 | **中文标识符** —— 无需音译 |
62: | 🧬 | **泛型**（单态化）、trait、**dyn Trait**（vtable 动态分发） |
63: | 🎯 | **带载荷枚举** + 穷尽 `match`（范围/守卫/OR） |
64: | 🚀 | **并发** —— `go`（真线程）+ `chan`（无界通道） |
65: | 🔌 | **C 交互** —— 内联 C、`extern "C"`、`import c "头.h"` |
66: | 🧩 | **宏** —— 声明式 `macro` + `@derive(Eq, Clone, Debug, ...)` |
67: | 💬 | **智能诊断** —— 稳定错误码、中英双语、"是否想用 X？" |
68: | 📦 | **模块** —— `import math`（内置）、`import a.b`（用户）、`import "x.gt"` |
69: | 🛠️ | **工具链** —— `gtc`（编译器/解释器）、`gtfmt`（格式化器） |
70: 
71: ---
72: 
73: ## 快速上手
74: 
75: ### 构建
76: 
77: > **预编译包**（无需 Rust/LLVM）：下载 [`dist/gtlang-res-win-x64.zip`](dist/gtlang-res-win-x64.zip)（10 MB），
78: 解压后直接运行 `gtc.exe`。包内已含 `gtc`、`gtfmt`、标准库、运行时与 TCC。
79: **不含** `clang`/`lld-link`（约 176 MB）—— GTLang 会从系统 `PATH`（或 `GTC_CLANG`）定位它们。
80: 
81: 或从源码构建：
82: 
83: ```bat
84: REM 需要 PATH 上有 Rust（>= 1.75）与 LLVM/clang（>= 15）
85: REM 可用 GTC_CLANG 指定 clang.exe 完整路径以覆盖自动探测
86: REM TCC 可选（用于内联 C 块），见下方 GTC_TCC 说明
87: build.bat
88: ```
89: 
90: `build.bat` 会先检查工具链版本，然后构建 `gtc`、`gtfmt`、标准库，并组装可分发的 `res/` 目录，**无需任何参数**。
91: 
92: **可选：TCC（用于内联 C 块）。** GTLang 通过 [TCC](https://bellard.org/tcc/) 执行内联 `C { ... }` 块。`build.bat` 按以下顺序查找：
93: 
94: 1. `GTC_TCC` 环境变量（指向含 `libtcc.dll` 的目录）
95: 2. `.\tcc`（本仓库自带）
96: 3. `.\toolchain\tcc`
97: 4. `PATH` 上的 `tcc`
98: 
99: 若均未找到，构建仍会成功 —— 仅内联 C 不可用。
100: 
101: 产物：
102: - `target\release\gtc.exe` —— 编译器 & 解释器
103: - `target\release\gtfmt.exe` —— 格式化器
104: - `res\lib\*.dll` —— 标准库
105: 
106: ### 运行第一个程序
107: 
108: 创建 `hello.gt`：
109: 
110: ```gt
111: fn main() {
112:     put("你好，世界！")
113: }
114: ```
115: 
116: 然后：
117: 
118: ```bat
119: REM 直接运行（Cranelift JIT）
120: target\release\gtc.exe --run hello.gt
121: 
122: REM 编译为独立可执行文件（LLVM + clang）
123: target\release\gtc.exe hello.gt -o hello.exe -O 2
124: hello.exe
125: ```
126: 
127: ---
128: 
129: ## 代码一览
130: 
131: ```gt
132: // 结构体、枚举、trait、泛型、模式匹配、闭包、并发
133: @derive(Debug, Clone, Eq)
134: struct 点 { x: int, y: int }
135: 
136: enum 形状 {
137:     Circle(f64)
138:     Rect(f64, f64)
139:     Unit
140: }
141: 
142: trait 面积 {
143:     fn area(self) -> f64
144: }
145: 
146: impl 面积 for 形状 {
147:     fn area(self) -> f64 {
148:         match self {
149:             形状::Circle(r) => { return 3.14159 * r * r }
150:             形状::Rect(w, h) => { return w * h }
151:             形状::Unit => { return 0.0 }
152:         }
153:     }
154: }
155: 
156: // 泛型函数（单态化）
157: fn 映射[T](xs: list, f) -> list {
158:     r := list()
159:     for x in xs { push(r, f(x)) }
160:     return r
161: }
162: 
163: fn main() {
164:     p := 点 { x: 3, y: 4 }
165:     put(p.to_str())                    // 点 { x: 3, y: 4 }
166: 
167:     // 列表推导
168:     平方 := [x * x for x in 0..6]
169:     put(len(平方))                      // 6
170: 
171:     // 计数循环
172:     loop 3 { put("hi") }
173: 
174:     // 穷尽匹配
175:     s := 形状::Circle(2.0)
176:     put(s.area())                       // 12.56636
177: 
178:     // 真线程 + 通道
179:     ch := chan()
180:     go 生产者(ch)
181:     put(chan_recv(ch))                  // 42
182: }
183: 
184: fn 生产者(ch) {
185:     chan_send(ch, 42)
186: }
187: ```
188: 
189: ---
190: 
191: ## 语言特性
192: 
193: ### 变量
194: 
195: ```gt
196: a = 1          // 裸赋值：自动声明 + 推导
197: x := 2         // 推导声明（类型可变）
198: let mut y = 3  // 推导，可变
199: let n: int = 4 // 显式固定类型
200: const K = 5    // 常量
201: ```
202: 
203: ### 控制流
204: 
205: ```gt
206: if c { ... } elif c2 { ... } else { ... }
207: loop 3 { ... }              // 计数循环
208: while c { ... }
209: do { ... } while c
210: for i in 0..n { ... }
211: for v in 容器 { ... }
212: for ... else { ... }        // 无 break 时执行 else
213: outer: for ... { break outer }
214: 
215: match v {
216:     1 => { ... }
217:     1..10 => { ... }        // 范围
218:     1 | 2 | 3 => { ... }    // OR
219:     n if n > 0 => { ... }   // 守卫
220:     _ => { ... }            // 通配
221: }
222: ```
223: 
224: ### 函数
225: 
226: ```gt
227: fn 加(a: int, b: int) -> int { a + b }   // 尾表达式即返回值
228: 
229: fn f(a, b) { ... }                        // 类型可推断
230: fn g(x: int = 10) { ... }                 // 默认参数
231: g(x: 5)                                   // 命名参数
232: 
233: fn 恒等[T](x: T) -> T { x }              // 泛型
234: fn max[T](a: T, b: T) -> T where T: Ord { ... }  // where 约束
235: 
236: |x| x * 2                                 // 闭包
237: |x: int| -> int { x * x }                 // 标注闭包
238: ```
239: 
240: ### 结构体 / 枚举 / trait
241: 
242: ```gt
243: struct 点 { x: int, y: int }
244: 
245: enum 形状 { Circle(f64) Rect(f64, f64) Unit }
246: 
247: trait 面积 { fn area(self) -> f64 }
248: impl 面积 for 圆 { fn area(self) -> f64 { ... } }
249: 
250: fn print_area(s: dyn 面积) { put(s.area()) }  // 动态分发
251: 
252: @derive(Eq, PartialEq, Clone, Debug, Display, Default, Hash, Ord)
253: struct 点 { x: int }
254: ```
255: 
256: **可用的 `@derive`：**
257: 
258: | 派生 | 生成 |
259: |---|---|
260: | `Eq` | `eq`、`ne` |
261: | `PartialEq` | `eq` |
262: | `Clone` | `clone` |
263: | `Debug` | `to_str` → `"类型名 { f: v }"` |
264: | `Display` | `to_str` → `"v1, v2"` |
265: | `Default` | `default` |
266: | `Hash` | `hash` |
267: | `Ord` | `cmp`、`lt`、`le`、`gt`、`ge`（+ `<`、`<=`、`>`、`>=`） |
268: 
269: ### 运算符重载
270: 
271: ```gt
272: impl 向量 {
273:     fn add(self, o: 向量) -> 向量 { ... }
274:     fn lt(self, o: 向量) -> bool { ... }
275: }
276: a + b    // → 向量__add(a, b)
277: a < b    // → 向量__lt(a, b)
278: ```
279: 
280: ### 并发
281: 
282: ```gt
283: go 工作者(42)          // 启动真线程
284: ch := chan()           // 无界通道
285: chan_send(ch, v)
286: v := chan_recv(ch)     // 阻塞接收
287: sleep(500)             // 毫秒
288: ```
289: 
290: ### C 交互
291: 
292: ```gt
293: C {
294:     static long long 平方(long long x) { return x * x; }
295: }
296: put(平方(5))           // 自动解析签名
297: 
298: extern "C" { fn puts(s: str) -> int }
299: 
300: import c "math.h" as m
301: put(m.sqrt(2.0))
302: ```
303: 
304: ### 错误处理
305: 
306: ```gt
307: fn f(n: int) -> Result[int, str] {
308:     if n < 0 { return Err("负数") }
309:     return Ok(n * 2)
310: }
311: 
312: v := f(21)?                                     // ? 传播
313: match f(-1) { Ok(v) => { ... } Err(e) => { ... } }
314: 
315: o := Some(1)
316: v := o or 0                                     // 默认值
317: 
318: try { throw "异常" } expt e { put(e) } fily { ... }
319: ```
320: 
321: ### 数据扩展
322: 
323: ```gt
324: t := (1, 2)                // 元组
325: let (a, b) = t             // 解构
326: s[0..3]                    // 切片
327: l[-1]                      // 负索引
328: a, b = b, a                // 交换
329: [x * 2 for x in l if x > 0]  // 列表推导
330: ```
331: 
332: ---
333: 
334: ## 双后端
335: 
336: GTLang 提供**两个后端**，消费同一份 AST：
337: 
338: | | `--c`（LLVM） | `--run`（Cranelift） |
339: |---|---|---|
340: | **产物** | 独立原生可执行文件 | 内存中 |
341: | **编译速度** | 慢（clang） | 快（JIT） |
342: | **运行速度** | 快（优化充分） | 基线 |
343: | **适用** | 发布 / 分发 | 开发 / 脚本 |
344: | **一致性** | — | **输出逐字节一致** |
345: 
346: 一致性由 `tests/consistency.rs` 强制：每个测试用同一份源码跑两个后端，断言 stdout 相同。
347: 
348: ---
349: 
350: ## 性能
351: 
352: 基准：`loop_sum(2e8)` —— 累加 `0..200_000_000`，**开启溢出检查**。
353: 
354: | 实现 | fib(35) | loop_sum(2e8) |
355: |---|---:|---:|
356: | C（clang -O2） | ~26 ms | ~5.5 ms |
357: | Rust（rustc -O） | ~28 ms | ~8 ms |
358: | **GTLang（-O2，有检查）** | ~40 ms | **5.9 ms** |
359: | Node.js 24（V8 JIT） | ~144 ms | ~195 ms |
360: | Lua 5.4 | ~445 ms | ~673 ms |
361: | Python 3.12 | ~1300 ms | ~6788 ms |
362: 
363: **GTLang 默认（安全）模式在紧循环上追平 C** —— 流敏感范围分析（`src/range.rs`）证明安全即省略溢出检查，**不牺牲安全性**。
364: 
365: 详见 [doc/bench.md](doc/bench.md)。
366: 
367: ---
368: 
369: ## CLI 速查
370: 
371: ```text
372: gtc --c        <文件.gt> [...] [-o 输出] [-O 0..3]   编译为可执行文件
373: gtc --run      <文件.gt> [...]                        解释执行（Cranelift JIT）
374: gtc --check    <文件.gt> [...]                        仅检查（词法/语法/类型）
375: gtc --lint     <文件.gt> [...]                        静态检查（未用函数/变量/参数、不可达代码等）
376: gtc --lint --strict <文件.gt>                         警告视为错误
377: gtc --lint --json   <文件.gt>                         JSON 输出（供 CI）
378: gtc --emit-llvm <文件.gt> [...]                       仅生成 LLVM IR
379: gtc --test     <文件.gt>                              运行 test_ 前缀的测试
380: gtc --watch/-w <文件.gt> ...                          监控变更自动重跑
381: gtc --version / --verbose                             版本 / 详细日志
382: gtc ... zh                                            中文诊断
383: 
384: gtfmt [--check] <文件.gt>                             格式化（先经完整检查）
385: ```
386: 
387: ---
388: 
389: ## 目录结构
390: 
391: ```text
392: src/
393:   lib.rs            模块声明 + 对外 API
394:   main.rs           CLI（参数解析 / 诊断渲染）
395:   bin/gtfmt.rs      格式化器
396:   lint.rs           静态检查（供 gtc --lint）
397:   ast.rs            统一 AST
398:   lexer.rs          词法（中文标识符、字符串插值、原始串）
399:   parser.rs         递归下降语法
400:   parser_recover.rs 语法错误恢复（panic-mode）
401:   sema.rs           语义 / 类型检查
402:   type.rs           类型系统（单一事实来源）
403:   codegen/          LLVM 后端（mod / expr / call）
404:   jit/              Cranelift 后端（mod / stmt / expr / call / rt / symbol）
405:   own.rs            所有权 / 借用检查（流敏感 NLL）
406:   cblock.rs         内联 C 提取 + 桥接 + C 头解析
407:   tcc.rs            libtcc 动态绑定
408:   driver.rs         工具链定位 + clang 调用
409:   module.rs         import 模块系统 + import c
410:   hoist.rs          嵌套函数 / 闭包提升 + impl/trait 展平
411:   mono.rs           泛型单态化 + 方法降级 + where 约束校验
412:   opt.rs            优化 pass + 宏展开 + 列表推导展开
413:   range.rs          整数范围分析（省略冗余溢出检查）
414:   unit.rs           前端产物 Unit
415:   diag.rs           诊断类型 + 源码位置
416:   encoding.rs       源文件解码
417:   tmp.rs            临时目录
418:   lang.rs           全局语言开关 + 双语消息
419:   runtime/gt_rt.c   内置 C 运行时（并发、通道、容器）
420:   stdlib/           标准库源码（math.rs / string.rs）
421: res/lib/            标准库产物（math.dll / string.dll + .lib）
422: examples/  tests/  bench/
423: toolchain/          （可选）vendored Rust + LLVM + TCC
424: ```
425: 
426: ---
427: 
428: ## 架构
429: 
430: ```text
431: 源码文本
432:    │  cblock::extract（挖空内联 C）
433:    ▼
434: lexer → parser → AST
435:    │  module::Linker（import 解析，含 import c）
436:    ▼
437: hoist（嵌套函数/闭包提升，impl/trait 展平）
438:    │  opt.resolve_named + opt.apply_defaults
439:    │  opt.expand_list_comp + opt.expand_macros
440:    │  expand_derives（@derive）
441:    ▼
442: sema(1)（填调用点类型）
443:    │  mono（方法降级 + 泛型单态化 + 运算符重载）
444:    │  opt.inline_and_fold + sema(2) 重推断
445:    ▼
446: sema(2)（完整类型检查）
447:    │  own（所有权/借用，NLL）
448:    │  opt.dead_code
449:    ▼
450: Unit ──┬── jit::run（Cranelift）
451:        └── codegen（LLVM IR → clang）
452: ```
453: 
454: ---
455: 
456: ## 测试
457: 
458: ```bat
459: cargo test --release
460: ```
461: 
462: **613 个测试**：
463: - **17 个单元测试**（`--lib`）—— 类型系统、cblock、tmp
464: - **96 个双后端一致性测试**（`tests/consistency.rs`）—— 同一源码、两个后端、stdout 相同
465: - **500 个前端批量测试**（`tests/bulk.rs`）—— parse + type-check 覆盖
466: 
467: ---
468: 
469: ## 文档
470: 
471: | 文档 | 内容 |
472: |---|---|
473: | [doc/LANGUAGE.md](doc/LANGUAGE.md) | 语言手册（17 章） |
474: | [doc/PERFORMANCE.md](doc/PERFORMANCE.md) | 性能 |
475: | [doc/COMPARISON.md](doc/COMPARISON.md) | GTLang vs Python（摘要） |
476: | [doc/GTLang_vs_Python.md](doc/GTLang_vs_Python.md) | GTLang vs Python（详细） |
477: | [doc/bench.md](doc/bench.md) | 基准 |
478: | [doc/syntax_status.md](doc/syntax_status.md) | 语法 / 特性状态 |
479: 
480: 所有文档均有**中文**（`doc/`）与**英文**（`doc/en/`）两个版本。
481: 
482: ---
483: 
484: ## 常见问题
485: 
486: **Q：为什么支持中文标识符？**
487: A：GTLang 把 Unicode 标识符视为一等公民。`计数器`、`累加` 与 `counter`、`sum` 同样合法，便于中文开发者与领域命名（数学、几何），无需音译。
488: 
489: **Q：为什么有两个后端？**
490: A：JIT（Cranelift）提供开发时的即时反馈；LLVM 后端产出优化的原生二进制用于发布。两者消费同一 AST 且输出一致 —— 测试保证。
491: 
492: **Q：性能为何接近 C？**
493: A：溢出检查是唯一成本。流敏感范围分析在可证明安全时省略检查（如 `for i in 0..N { s += i }`），让 LLVM 向量化循环。详见 `doc/bench.md`。
494: 
495: **Q：有垃圾回收吗？**
496: A：暂无。容器是引用语义，存活至进程退出。RC 原语（`gt_rc_inc` / `gt_rc_dec`）已就绪，插桩待续。
497: 
498: **Q：有哪些未实现？**
499: A：内联汇编（已移除）、`async/await`（与 `go` 重复）、完整 GC 插桩、交叉编译、过程宏。详见 [doc/syntax_status.md](doc/syntax_status.md)。
500: 
501: ---
502: 
503: ## 许可证
504: 
505: MIT（如存在 [LICENSE](LICENSE)）。
506: 

(End of file - total 506 lines)
</content>