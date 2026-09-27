<div align="center">

<img src="GTlangLOGO.png" alt="GTLang" width="160">

# GTLang

**GTLang is a statically-typed, compiled, expression-oriented language that runs on a dual backend (LLVM + Cranelift) and speaks Chinese identifiers natively — and its safe defaults match C on tight loops.**

[![Tests](https://img.shields.io/badge/tests-613%20passed-brightgreen)]()
[![Backends](https://img.shields.io/badge/backends-LLVM%20%2B%20Cranelift-blue)]()
[![Warnings](https://img.shields.io/badge/warnings-0-brightgreen)]()
[![License](https://img.shields.io/badge/license-MIT-lightgrey)]()

[中文](README.zh.md) | English

<img src="docs/benchmark.svg" alt="GTLang benchmark: loop_sum(2e8) — GTLang 5.9 ms vs C 5.5 ms, Python 6788 ms" width="760">

> ⚠️ **Early-stage project.** APIs, syntax, and the standard library may change without notice. Bugs and missing features are expected.
>
> 🪟 **Windows only (for now).** The current build and runtime target Windows x64. Linux/macOS support is not yet available.

</div>

---

## Why GTLang?

- **Fast by default.** Flow-sensitive range analysis elides provably-safe overflow checks, so checked arithmetic still reaches C-level speed (`loop_sum(2×10⁸)`: **5.9 ms**, within 7% of `clang -O2`).
- **One AST, two backends.** The same source compiles to a native executable (LLVM) or runs in memory (Cranelift JIT) with **byte-for-byte identical output**.
- **Safe, not sloppy.** Static types, ownership/borrow checking (flow-sensitive NLL), bounds/overflow/divide-by-zero checks, exhaustive `match`.
- **Bilingual from the ground up.** Chinese identifiers (`计数器`, `累加`, `点`) are first-class, and diagnostics are localized.
- **Genuinely productive.** Generics, traits, `dyn Trait`, enums with payloads, closures, macros, `@derive`, real threads, channels, and C interop.

---

## Table of Contents
25: 
26: - [What is GTLang?](#what-is-gtlang)
27: - [Highlights](#highlights)
28: - [Quick Start](#quick-start)
29: - [A Taste of GTLang](#a-taste-of-gtlang)
30: - [Language Features](#language-features)
31: - [Dual Backend](#dual-backend)
32: - [Performance](#performance)
33: - [CLI Reference](#cli-reference)
34: - [Project Layout](#project-layout)
35: - [Architecture](#architecture)
36: - [Testing](#testing)
37: - [Documentation](#documentation)
38: - [FAQ](#faq)
39: 
40: ---
41: 
42: ## What is GTLang?
43: 
44: GTLang is a **statically-typed**, **compiled**, **expression-oriented** programming language that combines:
45: 
46: - **Rust's memory safety** — ownership, borrowing, flow-sensitive NLL, bounds/overflow/divide-by-zero checks
47: - **C-level performance** — native machine code via LLVM; tight loops match C
48: - **Python-like brevity** — expression-oriented, optional type annotations, closures, comprehensions
49: - **First-class Chinese identifiers** — `计数器`, `累加`, `点` are all valid names
50: - **Dual backends** — the same AST compiles to a native executable (LLVM) **or** runs in-memory (Cranelift JIT), with **byte-for-byte identical output** enforced by tests
51: 
52: ---
53: 
54: ## Highlights
55: 
56: | | Feature |
57: |---|---|
58: | 🧠 | **Static typing** with bidirectional inference |
59: | 🛡️ | **Memory safety** — ownership, borrowing, NLL, bounds/overflow checks |
60: | ⚡ | **Dual backend** — LLVM (release) + Cranelift (JIT), identical semantics |
61: | 🌏 | **Chinese identifiers** — no transliteration needed |
62: | 🧬 | **Generics** (monomorphization), traits, **dyn Trait** (vtable dispatch) |
63: | 🎯 | **Enums with payloads** + exhaustive `match` (ranges, guards, OR-patterns) |
64: | 🚀 | **Concurrency** — `go` (real threads) + `chan` (unbounded channels) |
65: | 🔌 | **C interop** — inline C blocks, `extern "C"`, `import c "header.h"` |
66: | 🧩 | **Macros** — declarative `macro` + `@derive(Eq, Clone, Debug, ...)` |
67: | 💬 | **Smart diagnostics** — stable codes, bilingual, "did you mean X?" |
68: | 📦 | **Modules** — `import math` (builtin), `import a.b` (user), `import "x.gt"` |
69: | 🛠️ | **Toolchain** — `gtc` (compiler/interpreter), `gtfmt` (formatter) |
70: 
71: ---
72: 
73: ## Quick Start
74: 
75: ### Build
76: 
77: > **Prebuilt bundle** (no Rust/LLVM needed): download [`dist/gtlang-res-win-x64.zip`](dist/gtlang-res-win-x64.zip) (10 MB),
78: extract it anywhere, and run `gtc.exe`. It bundles `gtc`, `gtfmt`, the standard
79: library, the runtime, and TCC. It does **not** bundle `clang`/`lld-link`
80: (≈176 MB) — GTLang locates them on your system `PATH` (or via `GTC_CLANG`).
81: 
82: Or build from source:
83: 
84: ```bat
85: REM Requires Rust (>= 1.75) and LLVM/clang (>= 15) on PATH.
86: REM Set GTC_CLANG to the full path of clang.exe to override auto-detection.
87: REM TCC is optional (for inline C blocks); see GTC_TCC below.
88: build.bat
89: ```
90: 
91: `build.bat` checks the toolchain versions, then builds `gtc`, `gtfmt`, the
92: standard library, and assembles a portable `res/` directory.
93: It takes **no arguments**.
94: 
95: **Optional: TCC for inline C blocks.** GTLang executes inline `C { ... }` blocks
96: via [TCC](https://bellard.org/tcc/). `build.bat` looks for it in this order:
97: 
98: 1. `GTC_TCC` environment variable (a directory containing `libtcc.dll`)
99: 2. `.\tcc` (vendored in this repo)
100: 3. `.\toolchain\tcc`
101: 4. `tcc` on `PATH`
102: 
103: If none is found, the build still succeeds — only inline C becomes unavailable.
104: 
105: This produces:
106: - `target\release\gtc.exe` — compiler & interpreter
107: - `target\release\gtfmt.exe` — formatter
108: - `res\lib\*.dll` — standard library
109: 
110: ### Run your first program
111: 
112: Create `hello.gt`:
113: 
114: ```gt
115: fn main() {
116:     put("你好，世界！")
117: }
118: ```
119: 
120: Then:
121: 
122: ```bat
123: REM Run directly (Cranelift JIT)
124: target\release\gtc.exe --run hello.gt
125: 
126: REM Compile to a standalone executable (LLVM + clang)
127: target\release\gtc.exe hello.gt -o hello.exe -O 2
128: hello.exe
129: ```
130: 
131: ---
132: 
133: ## A Taste of GTLang
134: 
135: ```gt
136: // Structs, enums, traits, generics, pattern matching, closures, concurrency
137: @derive(Debug, Clone, Eq)
138: struct 点 { x: int, y: int }
139: 
140: enum 形状 {
141:     Circle(f64)
142:     Rect(f64, f64)
143:     Unit
144: }
145: 
146: trait 面积 {
147:     fn area(self) -> f64
148: }
149: 
150: impl 面积 for 形状 {
151:     fn area(self) -> f64 {
152:         match self {
153:             形状::Circle(r) => { return 3.14159 * r * r }
154:             形状::Rect(w, h) => { return w * h }
155:             形状::Unit => { return 0.0 }
156:         }
157:     }
158: }
159: 
160: // Generic function (monomorphized)
161: fn 映射[T](xs: list, f) -> list {
162:     r := list()
163:     for x in xs { push(r, f(x)) }
164:     return r
165: }
166: 
167: fn main() {
168:     p := 点 { x: 3, y: 4 }
169:     put(p.to_str())                    // 点 { x: 3, y: 4 }
170: 
171:     // List comprehension
172:     平方 := [x * x for x in 0..6]
173:     put(len(平方))                      // 6
174: 
175:     // Count loop
176:     loop 3 { put("hi") }
177: 
178:     // Exhaustive match
179:     s := 形状::Circle(2.0)
180:     put(s.area())                       // 12.56636
181: 
182:     // Real threads + channels
183:     ch := chan()
184:     go 生产者(ch)
185:     put(chan_recv(ch))                  // 42
186: }
187: 
188: fn 生产者(ch) {
189:     chan_send(ch, 42)
190: }
191: ```
192: 
193: ---
194: 
195: ## Language Features
196: 
197: ### Variables
198: 
199: ```gt
200: a = 1          // bare assignment: auto-declare + infer
201: x := 2         // inferred declaration (type may change)
202: let mut y = 3  // inferred, mutable
203: let n: int = 4 // explicit fixed type
204: const K = 5    // constant
205: ```
206: 
207: ### Control Flow
208: 
209: ```gt
210: if c { ... } elif c2 { ... } else { ... }
211: loop 3 { ... }              // count loop
212: while c { ... }
213: do { ... } while c
214: for i in 0..n { ... }
215: for v in container { ... }
216: for ... else { ... }        // else runs if no break
217: outer: for ... { break outer }
218: 
219: match v {
220:     1 => { ... }
221:     1..10 => { ... }        // range
222:     1 | 2 | 3 => { ... }    // OR
223:     n if n > 0 => { ... }   // guard
224:     _ => { ... }            // wildcard
225: }
226: ```
227: 
228: ### Functions
229: 
230: ```gt
231: fn add(a: int, b: int) -> int { a + b }   // last expression is returned
232: 
233: fn f(a, b) { ... }                        // types may be inferred
234: fn g(x: int = 10) { ... }                 // default arguments
235: g(x: 5)                                   // named arguments
236: 
237: fn id[T](x: T) -> T { x }                // generic
238: fn max[T](a: T, b: T) -> T where T: Ord { ... }  // where clause
239: 
240: |x| x * 2                                 // closure
241: |x: int| -> int { x * x }                 // annotated closure
242: ```
243: 
244: ### Structs / Enums / Traits
245: 
246: ```gt
247: struct 点 { x: int, y: int }
248: 
249: enum 形状 { Circle(f64) Rect(f64, f64) Unit }
250: 
251: trait 面积 { fn area(self) -> f64 }
252: impl 面积 for 圆 { fn area(self) -> f64 { ... } }
253: 
254: fn print_area(s: dyn 面积) { put(s.area()) }  // dynamic dispatch
255: 
256: @derive(Eq, PartialEq, Clone, Debug, Display, Default, Hash, Ord)
257: struct 点 { x: int }
258: ```
259: 
260: **Available `@derive`s:**
261: 
262: | Derive | Generates |
263: |---|---|
264: | `Eq` | `eq`, `ne` |
265: | `PartialEq` | `eq` |
266: | `Clone` | `clone` |
267: | `Debug` | `to_str` → `"Name { f: v }"` |
268: | `Display` | `to_str` → `"v1, v2"` |
269: | `Default` | `default` |
270: | `Hash` | `hash` |
271: | `Ord` | `cmp`, `lt`, `le`, `gt`, `ge` (+ `<`, `<=`, `>`, `>=`) |
272: 
273: ### Operator Overloading
274: 
275: ```gt
276: impl 向量 {
277:     fn add(self, o: 向量) -> 向量 { ... }
278:     fn lt(self, o: 向量) -> bool { ... }
279: }
280: a + b    // → 向量__add(a, b)
281: a < b    // → 向量__lt(a, b)
282: ```
283: 
284: ### Concurrency
285: 
286: ```gt
287: go 工作者(42)          // spawn real thread
288: ch := chan()           // unbounded channel
289: chan_send(ch, v)
290: v := chan_recv(ch)     // blocking receive
291: sleep(500)             // milliseconds
292: ```
293: 
294: ### C Interop
295: 
296: ```gt
297: C {
298:     static long long 平方(long long x) { return x * x; }
299: }
300: put(平方(5))           // auto-resolves signature
301: 
302: extern "C" { fn puts(s: str) -> int }
303: 
304: import c "math.h" as m
305: put(m.sqrt(2.0))
306: ```
307: 
308: ### Error Handling
309: 
310: ```gt
311: fn f(n: int) -> Result[int, str] {
312:     if n < 0 { return Err("negative") }
313:     return Ok(n * 2)
314: }
315: 
316: v := f(21)?                                     // ? propagates
317: match f(-1) { Ok(v) => { ... } Err(e) => { ... } }
318: 
319: o := Some(1)
320: v := o or 0                                     // default
321: 
322: try { throw "boom" } expt e { put(e) } fily { ... }
323: ```
324: 
325: ### Data Extensions
326: 
327: ```gt
328: t := (1, 2)                // tuple
329: let (a, b) = t             // destructuring
330: s[0..3]                    // slicing
331: l[-1]                      // negative index
332: a, b = b, a                // swap
333: [x * 2 for x in l if x > 0]  // list comprehension
334: ```
335: 
336: ---
337: 
338: ## Dual Backend
339: 
340: GTLang ships with **two backends** that consume the same AST:
341: 
342: | | `--c` (LLVM) | `--run` (Cranelift) |
343: |---|---|---|
344: | **Output** | Standalone native executable | In-memory |
345: | **Speed (compile)** | Slower (clang) | Fast (JIT) |
346: | **Speed (run)** | Faster (optimized) | Baseline |
347: | **Use case** | Release / distribution | Development / scripting |
348: | **Consistency** | — | **Byte-for-byte identical output** |
349: 
350: This consistency is enforced by the `tests/consistency.rs` test suite: every test runs the same source through both backends and asserts identical stdout.
351: 
352: ---
353: 
354: ## Performance
355: 
356: Benchmark: `loop_sum(2e8)` — sum `0..200_000_000` with overflow checks enabled.
357: 
358: | Implementation | fib(35) | loop_sum(2e8) |
359: |---|---:|---:|
360: | C (clang -O2) | ~26 ms | ~5.5 ms |
361: | Rust (rustc -O) | ~28 ms | ~8 ms |
362: | **GTLang (-O2, checked)** | ~40 ms | **5.9 ms** |
363: | Node.js 24 (V8 JIT) | ~144 ms | ~195 ms |
364: | Lua 5.4 | ~445 ms | ~673 ms |
365: | Python 3.12 | ~1300 ms | ~6788 ms |
366: 
367: **GTLang's default (safe) mode matches C on tight loops** thanks to flow-sensitive range analysis (`src/range.rs`) that elides provably-safe overflow checks — without sacrificing safety.
368: 
369: See [doc/bench.md](doc/bench.md) for methodology.
370: 
371: ---
372: 
373: ## CLI Reference
374: 
375: ```text
376: gtc --c        <file.gt> [...] [-o out.exe] [-O 0..3]   compile to executable
377: gtc --run      <file.gt> [...]                          run with Cranelift JIT
378: gtc --check    <file.gt> [...]                          lex/parse/type-check only
379: gtc --lint     <file.gt> [...]                          static checks (unused fns/vars, unreachable, ...)
380: gtc --lint --strict <file.gt>                           warnings are errors
381: gtc --lint --json   <file.gt>                           JSON output (for CI)
382: gtc --emit-llvm <file.gt> [...]                         emit LLVM IR
383: gtc --test     <file.gt>                                run test_ prefixed functions
384: gtc --watch/-w <file.gt> ...                            watch and rebuild
385: gtc --version / --verbose                               version / verbose
386: gtc ... zh                                              Chinese diagnostics
387: 
388: gtfmt [--check] <file.gt>                               format (after full check)
389: ```
390: 
391: ---
392: 
393: ## Project Layout
394: 
395: ```text
396: src/
397:   lib.rs            module declarations + public API
398:   main.rs           CLI (arg parsing / diagnostic rendering)
399:   bin/gtfmt.rs      formatter
400:   lint.rs           static checks (for gtc --lint)
401:   ast.rs            unified AST
402:   lexer.rs          lexer (Chinese identifiers, string interpolation, raw strings)
403:   parser.rs         recursive-descent parser
404:   parser_recover.rs syntax error recovery (panic-mode)
405:   sema.rs           semantic / type checking
406:   type.rs           type system (single source of truth)
407:   codegen/          LLVM backend (mod / expr / call)
408:   jit/              Cranelift backend (mod / stmt / expr / call / rt / symbol)
409:   own.rs            ownership / borrow checking (flow-sensitive NLL)
410:   cblock.rs         inline C extraction + bridging + C header parsing
411:   tcc.rs            libtcc dynamic binding
412:   driver.rs         toolchain location + clang invocation
413:   module.rs         import system + import c
414:   hoist.rs          nested fn / closure lifting + impl/trait flattening
415:   mono.rs           generic monomorphization + method lowering + where checks
416:   opt.rs            optimization passes + macro expansion + list-comp desugaring
417:   range.rs          integer range analysis (elides redundant overflow checks)
418:   unit.rs           frontend output Unit
419:   diag.rs           diagnostic types + source spans
420:   encoding.rs       source decoding
421:   tmp.rs            temp directories
422:   lang.rs           global language switch + bilingual messages
423:   runtime/gt_rt.c   built-in C runtime (concurrency, channels, containers)
424:   stdlib/           standard library source (math.rs / string.rs)
425: res/lib/            stdlib artifacts (math.dll / string.dll + .lib)
426: examples/  tests/  bench/
427: toolchain/          (optional) vendored Rust + LLVM + TCC
428: ```
429: 
430: ---
431: 
432: ## Architecture
433: 
434: ```text
435: source text
436:    │  cblock::extract  (extract inline C)
437:    ▼
438: lexer → parser → AST
439:    │  module::Linker  (resolve imports, incl. import c)
440:    ▼
441: hoist  (lift nested fns/closures, flatten impl/trait)
442:    │  opt.resolve_named + opt.apply_defaults
443:    │  opt.expand_list_comp + opt.expand_macros
444:    │  expand_derives (@derive)
445:    ▼
446: sema(1)  (fill call-site types)
447:    │  mono  (method lowering + generic monomorphization + operator overloading)
448:    │  opt.inline_and_fold  + sema(2) re-infer
449:    ▼
450: sema(2)  (full type check)
451:    │  own   (ownership / borrow, NLL)
452:    │  opt.dead_code
453:    ▼
454: Unit ──┬── jit::run      (Cranelift)
455:        └── codegen       (LLVM IR → clang)
456: ```
457: 
458: ---
459: 
460: ## Testing
461: 
462: ```bat
463: cargo test --release
464: ```
465: 
466: **613 tests**:
467: - **17 unit tests** (`--lib`) — type system, cblock, tmp
468: - **96 dual-backend consistency tests** (`tests/consistency.rs`) — same source, both backends, identical stdout
469: - **500 frontend bulk tests** (`tests/bulk.rs`) — parse + type-check coverage
470: 
471: ---
472: 
473: ## Documentation
474: 
475: | Doc | Content |
476: |---|---|
477: | [doc/LANGUAGE.md](doc/LANGUAGE.md) | Language reference (17 chapters) |
478: | [doc/PERFORMANCE.md](doc/PERFORMANCE.md) | Performance characteristics |
479: | [doc/COMPARISON.md](doc/COMPARISON.md) | GTLang vs Python (summary) |
480: | [doc/GTLang_vs_Python.md](doc/GTLang_vs_Python.md) | GTLang vs Python (detailed) |
481: | [doc/bench.md](doc/bench.md) | Benchmarks |
482: | [doc/syntax_status.md](doc/syntax_status.md) | Syntax / feature status |
483: 
484: All docs are available in both **Chinese** (`doc/`) and **English** (`doc/en/`).
485: 
486: ---
487: 
488: ## FAQ
489: 
490: **Q: Why Chinese identifiers?**
491: A: GTLang treats Unicode identifiers as first-class. Chinese names like `计数器` or `累加` are as valid as `counter` or `sum`. This makes the language more approachable for Chinese-speaking developers and enables domain-specific naming (e.g. math, geometry) without transliteration.
492: 
493: **Q: Why two backends?**
494: A: The JIT (Cranelift) gives instant feedback during development; the LLVM backend produces optimized native binaries for release. Both consume the same AST and produce identical output — tests enforce this.
495: 
496: **Q: How is performance so close to C?**
497: A: Overflow checks are the only cost. Flow-sensitive range analysis elides them when provably safe (e.g. `for i in 0..N { s += i }`), letting LLVM vectorize the loop. See `doc/bench.md`.
498: 
499: **Q: Is there a garbage collector?**
500: A: Not yet. Containers are reference-semantics and live until process exit. RC primitives (`gt_rc_inc` / `gt_rc_dec`) are in place; instrumentation is pending.
501: 
502: **Q: What's not implemented?**
503: A: Inline assembly (removed), `async/await` (overlaps with `go`), full GC instrumentation, cross-compilation, procedural macros. See [doc/syntax_status.md](doc/syntax_status.md).
504: 
505: ---
506: 
507: ## License
508: 
509: MIT (see [LICENSE](LICENSE) if present).
510: 

(End of file - total 510 lines)
</content>
