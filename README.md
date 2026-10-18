<div align="center">

<img src="GTlangLOGO.png" alt="GTLang" width="160">

# GTLang

**GTLang is a statically-typed, compiled, expression-oriented language (LLVM backend) that speaks Chinese identifiers natively — and its safe defaults match C on tight loops.**

[![Tests](https://img.shields.io/badge/tests-827%20passed-brightgreen)]()
[![Backend](https://img.shields.io/badge/backend-LLVM-blue)]()
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
- **Native code, no interpreter.** Every program compiles to a standalone executable via LLVM — no JIT, no runtime VM.
- **Safe, not sloppy.** Static types, ownership/borrow checking (flow-sensitive NLL), bounds/overflow/divide-by-zero checks, exhaustive `match`.
- **Bilingual from the ground up.** Chinese identifiers (`计数器`, `累加`, `点`) are first-class, and diagnostics are localized.
- **Genuinely productive.** Generics, traits, `dyn Trait`, enums with payloads, closures, **first-class functions** (pass a `fn` name as a value), macros, `@derive`, real threads, channels, and C interop.

---

## Table of Contents

- [What is GTLang?](#what-is-gtlang)
- [Highlights](#highlights)
- [Quick Start](#quick-start)
- [A Taste of GTLang](#a-taste-of-gtlang)
- [Language Features](#language-features)
- [Dual Backend](#dual-backend)
- [Performance](#performance)
- [CLI Reference](#cli-reference)
- [Project Layout](#project-layout)
- [Architecture](#architecture)
- [Testing](#testing)
- [Documentation](#documentation)
- [FAQ](#faq)

---

## What is GTLang?

GTLang is a **statically-typed**, **compiled**, **expression-oriented** programming language that combines:

- **Rust's memory safety** — ownership, borrowing, flow-sensitive NLL, bounds/overflow/divide-by-zero checks
- **C-level performance** — native machine code via LLVM; tight loops match C
- **Python-like brevity** — expression-oriented, optional type annotations, closures, comprehensions
- **First-class Chinese identifiers** — `计数器`, `累加`, `点` are all valid names
- **Raw 64-bit integers** — `int` is a native i64 (overflow wraps, C semantics); tight loops reach C speed
- **Exact decimals** — a built-in `decimal` type for arbitrary-precision base-10 arithmetic
- **High-precision decimals** — a built-in `decimal` type for exact base-10 arithmetic (`0.1 + 0.2 == 0.3`)
- **Zero-link stdlib** — standard library ships as shared DLLs loaded at startup; no `.lib` needed

---

## Highlights

| | Feature |
|---|---|
| 🧠 | **Static typing** with bidirectional inference |
| 🛡️ | **Memory safety** — ownership, borrowing, NLL, bounds/overflow checks |
| ⚡ | **Native LLVM backend** — compiles to a standalone executable |
| 🔢 | **Raw i64** — native 64-bit int (C semantics, C-speed loops) |
| 🔢 | **Decimal** — exact high-precision base-10 (`3.14d`) |
| 🎯 | **Decimal** — exact high-precision base-10 (`3.14d`) |
| 🧵 | **GTO concurrency** — `chan` backed by the GTO runtime (`gto.dll`) |
| 🌏 | **Chinese identifiers** — no transliteration needed |
| 🧬 | **Generics** (monomorphization), traits, **dyn Trait** (vtable dispatch) |
| 🎯 | **Enums with payloads** + exhaustive `match` (ranges, guards, OR-patterns) |
| 🚀 | **Concurrency** — `go` (real threads) + `chan` (unbounded channels) |
| 🔌 | **C interop** — inline C blocks, `extern "C"`, `import c "header.h"` |
| 🧩 | **Macros** — declarative `macro` + `@derive(Eq, Clone, Debug, ...)` |
| 🪄 | **First-class functions** — pass a `fn` name as a value; closures capture containers/params |
| 🔗 | **Ergonomic borrows** — automatic deref for `&T`/`&mut T` fields, methods and params; `dyn Trait` auto-boxing |
| 🧷 | **Rich patterns** — ident bindings, nested destructuring (`Some(Some(v))`) |
| 💬 | **Smart diagnostics** — stable codes, bilingual, "did you mean X?" |
| 📦 | **Modules** — `import math` (builtin), `import a.b` (user), `import "x.gt"` |
| 🛠️ | **Toolchain** — `gtc` (the compiler) |

---

## Quick Start

### Build

> **Prebuilt bundle** (no Rust/LLVM needed): download [`dist/gtlang-res-win-x64.zip`](dist/gtlang-res-win-x64.zip),
extract it anywhere, and run `gtc.exe`. It bundles `gtc`, the standard
library (shared DLLs), the runtime and GTO. It does **not** bundle `clang`/`lld-link`
— GTLang locates them on your system `PATH` (or via `GTC_CLANG`).

Or build from source:

```bat
REM Requires Rust (>= 1.75) and LLVM/clang (>= 15) on PATH.
REM Set GTC_CLANG to the full path of clang.exe to override auto-detection.
build.bat
```

`build.bat` checks the toolchain versions, then builds `gtc`, the
standard library, GTO (`gto.dll`) and assembles a portable `res/` directory.
Inline `C { ... }` blocks are compiled with clang — no TCC needed.

This produces:
- `target\release\gtc.exe` — the compiler
- `res\lib\*.dll` — standard library (shared DLLs) + `gto.dll`
- `res\runtime\*` — runtime sources/objects

### Run your first program

Create `hello.gt`:

```gt
fn main() {
    put("你好，世界！")
}
```

Then:

```bat
REM Compile to a standalone executable (LLVM + clang)
target\release\gtc.exe hello.gt -o hello.exe -O 2
hello.exe
```

---

## A Taste of GTLang

```gt
// Structs, enums, traits, generics, pattern matching, closures, concurrency
@derive(Debug, Clone, Eq)
struct 点 { x: int, y: int }

enum 形状 {
    Circle(f64)
    Rect(f64, f64)
    Unit
}

trait 面积 {
    fn area(self) -> f64
}

impl 面积 for 形状 {
    fn area(self) -> f64 {
        match self {
            形状::Circle(r) => { return 3.14159 * r * r }
            形状::Rect(w, h) => { return w * h }
            形状::Unit => { return 0.0 }
        }
    }
}

// Generic function (monomorphized)
fn 映射[T](xs: list, f) -> list {
    r := list()
    for x in xs { push(r, f(x)) }
    return r
}

fn main() {
    p := 点 { x: 3, y: 4 }
    put(p.to_str())                    // 点 { x: 3, y: 4 }

    // List comprehension
    平方 := [x * x for x in 0..6]
    put(len(平方))                      // 6

    // Count loop
    loop 3 { put("hi") }

    // Exhaustive match
    s := 形状::Circle(2.0)
    put(s.area())                       // 12.56636

    // Real threads + channels
    ch := chan()
    go 生产者(ch)
    put(chan_recv(ch))                  // 42
}

fn 生产者(ch) {
    chan_send(ch, 42)
}
```

---

## Language Features

### Variables

```gt
a = 1          // bare assignment: auto-declare + infer
x := 2         // inferred declaration (type may change)
let mut y = 3  // inferred, mutable
let n: int = 4 // explicit fixed type
const K = 5    // constant
```

### Control Flow

```gt
if c { ... } elif c2 { ... } else { ... }
loop 3 { ... }              // count loop
while c { ... }
do { ... } while c
for i in 0..n { ... }        // range
for v in container { ... }   // list / [T;N] / str / set / map (iterates keys)
for ... else { ... }        // else runs if no break
outer: for ... { break outer }

match v {
    1 => { ... }
    1..10 => { ... }        // range
    1 | 2 | 3 => { ... }    // OR
    n if n > 0 => { ... }   // guard
    _ => { ... }            // wildcard
}
```

### Functions

```gt
fn add(a: int, b: int) -> int { a + b }   // last expression is returned

fn f(a, b) { ... }                        // types may be inferred
fn g(x: int = 10) { ... }                 // default arguments
g(x: 5)                                   // named arguments

fn id[T](x: T) -> T { x }                // generic
fn max[T](a: T, b: T) -> T where T: Ord { ... }  // where clause

|x| x * 2                                 // closure
|x: int| -> int { x * x }                 // annotated closure

fn inc(x: int) -> int { x + 1 }
g := inc                                  // first-class function (store in a variable)
apply(inc, 5)                             // higher-order (pass as an argument)
fs[0](5)                                  // any expression as callee

o := Some(1)
v := o or 0                              // Option default value

// ergonomic borrows: automatic deref
p := 点 { x: 1, y: 2 }
r := &p
r.x                                     // same as p.x
```

### Structs / Enums / Traits

```gt
struct 点 { x: int, y: int }

enum 形状 { Circle(f64) Rect(f64, f64) Unit }

trait 面积 { fn area(self) -> f64 }
impl 面积 for 圆 { fn area(self) -> f64 { ... } }

fn print_area(s: dyn 面积) { put(s.area()) }  // dynamic dispatch

@derive(Eq, PartialEq, Clone, Debug, Display, Default, Hash, Ord)
struct 点 { x: int }
```

**Available `@derive`s:**

| Derive | Generates |
|---|---|
| `Eq` | `eq`, `ne` |
| `PartialEq` | `eq` |
| `Clone` | `clone` |
| `Debug` | `to_str` → `"Name { f: v }"` |
| `Display` | `to_str` → `"v1, v2"` |
| `Default` | `default` |
| `Hash` | `hash` |
| `Ord` | `cmp`, `lt`, `le`, `gt`, `ge` (+ `<`, `<=`, `>`, `>=`) |

### Operator Overloading

```gt
impl 向量 {
    fn add(self, o: 向量) -> 向量 { ... }
    fn lt(self, o: 向量) -> bool { ... }
}
a + b    // → 向量__add(a, b)
a < b    // → 向量__lt(a, b)
```

### Concurrency

```gt
go 工作者(42)          // spawn real thread
ch := chan()           // unbounded channel
chan_send(ch, v)
v := chan_recv(ch)     // blocking receive
sleep(500)             // milliseconds
```

### C Interop

```gt
C {
    static long long 平方(long long x) { return x * x; }
}
put(平方(5))           // auto-resolves signature

extern "C" { fn puts(s: str) -> int }

import c "native.h" as n            // project-local header only
put(m.sqrt(2.0))
```

### Error Handling

```gt
fn f(n: int) -> Result[int, str] {
    if n < 0 { return Err("negative") }
    return Ok(n * 2)
}

v := f(21)?                                     // ? propagates
match f(-1) { Ok(v) => { ... } Err(e) => { ... } }

o := Some(1)
v := o or 0                                     // default

try { throw "boom" } expt e { put(e) } fily { ... }
```

### Data Extensions

```gt
t := (1, 2)                // tuple
let (a, b) = t             // destructuring
s[0..3]                    // slicing
l[-1]                      // negative index
a, b = b, a                // swap
[x * 2 for x in l if x > 0]  // list comprehension
```

---

## Backend & Runtime

GTLang compiles to a **standalone native executable** via LLVM + clang. There is no interpreter/JIT — every program is a native binary.

- **Standard library**: shipped as shared DLLs (`res/lib/*.dll`). At build time a small shim is generated that `LoadLibrary`s only the DLLs the program actually uses and forwards each call by its exact signature. The produced executable therefore links **no stdlib `.lib`** — only system DLLs (`KERNEL32.dll`).
- **Concurrency**: `go` runs real threads; `chan` is backed by **GTO** (`gto.dll`), loaded at runtime.
- **Integers**: `int` is a native 64-bit integer (overflow wraps, C semantics); tight loops compile to plain LLVM arithmetic with no runtime overhead.
- **Decimals**: the `decimal` type (`3.14d`) provides exact base-10 arithmetic.
- **Two-stage optimizer**: `-O0..-O4` drives both GTLang's own AST-level optimizer (constant folding/propagation, function inlining, dead-code elimination) and clang's backend. `--emit-optimized-gt[=DIR]` writes the optimized `.gt` source back out (flat, machine-oriented, re-compilable) — like clang's optimized IR.

---

## Performance

Benchmark: `loop_sum(2e8)` — sum `0..200_000_000`.

| Benchmark | GTLang -O4 | C (clang -O2) | Rust (rustc -O) |
|---|---:|---:|---:|
| loop_sum(2e8) | **5.2 ms** | 4.8 ms | 7.5 ms |
| fib(35) | **23.9 ms** | 22.5 ms | 25.2 ms |
| loop_lcg(1e7) | **34.3 ms** | 25.7 ms | 34.4 ms |
| array(1e8) | **9.1 ms** | 12.1 ms | 11.9 ms |

GTLang matches or beats Rust on all four, and is on par with C (faster on array; slower on loop_lcg because C inlines the C-block helper). `-O0..-O4` drives both the GTLang AST optimizer and clang; `--native` adds `-march=native` for max speed. See [doc/bench.md](doc/bench.md).

**GTLang's `int` is a raw 64-bit integer** — overflow wraps (C semantics). Tight loops compile to plain LLVM `add/sub/mul` with zero runtime overhead, so GTLang matches C on `loop_sum` (~4.2 ms) and comes close on recursive `fib` (~26 ms). For exact high-precision arithmetic, use the separate `decimal` type. The GTLang AST optimizer (`-O0..-O4`) folds constants, inlines, and removes dead code before LLVM.

See [doc/bench.md](doc/bench.md) for methodology.

---

## CLI Reference

```text
gtc --c        <file.gt> [...] [-o out.exe] [-O 0..4]   compile to executable
gtc --check    <file.gt> [...]                          lex/parse/type-check only
gtc --lint     <file.gt> [...]                          static checks (unused fns/vars, unreachable, ...)
gtc --lint --strict <file.gt>                           warnings are errors
gtc --lint --json   <file.gt>                           JSON output (for CI)
gtc --emit-llvm <file.gt> [...]                         emit LLVM IR
gtc --c <file.gt> --emit-optimized-gt[=DIR] [-O n]      also write optimized .gt source
gtc --watch/-w <file.gt> ...                            watch and rebuild
gtc --version / --verbose                               version / verbose
gtc ... zh                                              Chinese diagnostics

```

---

## Project Layout

```text
src/
lib.rs            module declarations + public API
main.rs           CLI (arg parsing / diagnostic rendering)

lint.rs           static checks (for gtc --lint)
ast.rs            unified AST
lexer.rs          lexer (Chinese identifiers, string interpolation, raw strings)
parser.rs         recursive-descent parser
parser_recover.rs syntax error recovery (panic-mode)
sema/             semantic / type checking (mod + sema_const/infer/match/stmt/util)
type/             type system (mod.rs single source of truth + type_gtlib.rs)
codegen/          LLVM backend (mod / expr / call / builtins / value / stmt)
own/              ownership / borrow checking (flow-sensitive NLL)
cblock.rs         inline C extraction + bridging + C header parsing
stdlib_shim.rs    generates runtime DLL loader/forwarding C for zero-.lib AOT
unparse.rs        AST -> .gt source printer (--emit-optimized-gt)
driver.rs         toolchain location + clang invocation
module.rs         import system + import c
hoist.rs          nested fn / closure lifting + impl/trait flattening
mono.rs           generic monomorphization + method lowering + where checks
opt.rs            optimization passes + macro expansion + list-comp desugaring
range.rs          integer range analysis (elides redundant overflow checks)
unit.rs           frontend output Unit
diag.rs           diagnostic types + source spans
encoding.rs       source decoding
tmp.rs            temp directories
lang.rs           global language switch + bilingual messages
runtime/gt_rt.c   built-in C runtime (decimal, containers, chan->GTO)
GTO/              GTO concurrency framework (compiled to gto.dll)
gtlib/            standard library source (math.rs / string.rs / ...)
res/lib/            stdlib artifacts (math.dll / string.dll / gto.dll, no .lib)
examples/  tests/  bench/
toolchain/          (optional) vendored Rust + LLVM
```

---

## Architecture

```text
source text
│  cblock::extract  (extract inline C)
▼
lexer → parser → AST
│  module::Linker  (resolve imports, incl. import c)
▼
hoist  (lift nested fns/closures, flatten impl/trait)
│  opt.resolve_named + opt.apply_defaults
│  opt.expand_list_comp + opt.expand_macros
│  expand_derives (@derive)
▼
sema(1)  (fill call-site types)
│  mono  (method lowering + generic monomorphization + operator overloading)
│  opt.inline_and_fold  + sema(2) re-infer
▼
sema(2)  (full type check)
│  own   (ownership / borrow, NLL)
│  opt.dead_code
▼
Unit ── codegen       (LLVM IR → clang)  +  stdlib_shim (runtime DLL loader)
```

---

## Testing

```bat
cargo test --release
```

**827 tests**:
- **143 unit tests** (`--lib`) — type system, unify, cblock, tmp, AST-traversal completeness
- **166 end-to-end tests** (`tests/consistency.rs`) — compile + run each sample (incl. examples/ recursively), assert expected output
- **518 frontend bulk tests** (`tests/bulk.rs`) — parse + type-check coverage

---

## Documentation

| Doc | Content |
|---|---|
| [doc/LANGUAGE.md](doc/LANGUAGE.md) | Language reference (17 chapters) |
| [doc/PERFORMANCE.md](doc/PERFORMANCE.md) | Performance characteristics |
| [doc/COMPARISON.md](doc/COMPARISON.md) | GTLang vs Python (summary) |
| [doc/GTLang_vs_Python.md](doc/GTLang_vs_Python.md) | GTLang vs Python (detailed) |
| [doc/bench.md](doc/bench.md) | Benchmarks |
| [doc/syntax_status.md](doc/syntax_status.md) | Syntax / feature status |

All docs are available in both **Chinese** (`doc/`) and **English** (`doc/en/`).

---

## FAQ

**Q: Why Chinese identifiers?**
A: GTLang treats Unicode identifiers as first-class. Chinese names like `计数器` or `累加` are as valid as `counter` or `sum`. This makes the language more approachable for Chinese-speaking developers and enables domain-specific naming (e.g. math, geometry) without transliteration.

**Q: Is there an interpreter or JIT?**
A: No. GTLang compiles every program to a standalone native executable via LLVM. The standard library ships as shared DLLs loaded at startup — the produced executable links no stdlib `.lib`, only system DLLs.

**Q: What about big integers and decimals?**
A: `int` is a raw 64-bit integer (overflow wraps, C semantics) — tight loops reach C speed. For exact base-10 math there is a separate `decimal` type (literal suffix `d`, e.g. `3.14d`).

**Q: How is performance so close to C?**
A: Overflow checks are the only cost. Flow-sensitive range analysis elides them when provably safe (e.g. `for i in 0..N { s += i }`), letting LLVM vectorize the loop. See `doc/bench.md`.

**Q: Is there a garbage collector?**
A: Not yet. Containers are reference-semantics and live until process exit. RC primitives (`gt_rc_inc` / `gt_rc_dec`) are in place; instrumentation is pending. Bignum/decimal objects are likewise heap-allocated and live until exit.

**Q: What's not implemented?**
A: Inline assembly (removed), `async/await` (overlaps with `go`), full GC instrumentation, cross-compilation, procedural macros. See [doc/syntax_status.md](doc/syntax_status.md).

---

## License

MIT (see [LICENSE](LICENSE) if present).


(End of file - total 510 lines)
</content>
