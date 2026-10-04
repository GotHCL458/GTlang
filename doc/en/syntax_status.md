# GTLang Syntax / Feature Status

> [中文](../syntax_status.md)

> Version 0.0.1b | Dual backend (LLVM + Cranelift), byte-for-byte consistent

## 1. Implemented (100%)

### 1.1 Lexical
Chinese/Unicode identifiers, // and # comments, /// doc comments,
integers (0x/0b/0o/_), floats, string interpolation $X/\${expr}, raw strings r"..."

### 1.2 Variables
:= / let / let mut / const / **bare a = v** (auto-declare); inferred mutable, explicit fixed

### 1.3 Control Flow
if/elif/else, while, do-while, for i in a..b, for v in container, for-else,
**labeled loops**, break/continue, match (literal/wildcard/guard/**range**/**OR**),
**membership in**, **index sugar**, **++/--**, **loop N**

### 1.4 Functions
optional param/return types, Chinese names, recursion, **nested functions**,
generics (monomorphization + **where**), closures, **default args**, **named args**,
**trait default methods**, **closure annotations** |x: int| -> int,
**first-class functions** (a top-level `fn` name works as a value: arg / let / `if` / `match` / `return` / container),
**any expression as callee** (`fs[0](5)`, `(f)(10)`), **higher-order functions** (unannotated closure param),
**function returning a closure** (`return |x| x * n`)

### 1.5 Types
basic, fixed array, list/set/map, struct, generic struct, trait+impl (blanket),
borrow &T/&mut T, **tuple**, **enum**, **dyn Trait** (vtable dynamic dispatch)

### 1.6 Data Extensions
tuple (1,2) (t.0), destructuring let (a,b) = t, slicing s[0..5], negative index l[-1],
swap a,b = b,a, list comprehension [x*2 for x in l if c]

### 1.7 Operator Overloading
impl T { fn add/sub/mul/div/rem/eq/ne/lt/le/gt/ge/neg } -> a + b / -a

### 1.8 Macros
declarative macro; @derive: **Eq / PartialEq / Clone / Debug / Display / Default / Hash / Ord**

### 1.9 Concurrency
go f(args) (real threads), chan()/chan_send/chan_recv (unbounded), sleep(ms)

### 1.10 Modules
import math (builtin), import a.b (user), import "x.gt", import c "a.h"

### 1.11 C Interop
inline C block, extern "C", import c "a.h"

### 1.12 Error Handling
Result/Option + ?, try/expt/fily + throw/raise, expr or default

### 1.13 Ownership
move semantics, borrow conflicts, **flow-sensitive NLL**

### 1.14 Memory Safety
bounds checks, div-by-zero checks, **add/sub/mul overflow detection**

### 1.15 Diagnostics
stable codes (E001-E8xx) + Help + ariadne + bilingual + **multiple errors**
+ **smart suggestions** ("did you mean X?", Levenshtein <= 2)

### 1.16 Optimizations
inline + constant folding + propagation + dead code + range analysis

### 1.17 Inference
bidirectional (let x: T = v), match arm type_join, iterative call-site inference

### 1.18 Match Exhaustiveness
enum all variants / bool true+false / Option Some+None / Result Ok+Err (or _)

### 1.19 Builtins
range, assert, ternary, pad_left/right/fmt_int, put/len/str/int/f64/bool,
container ops, string ops, mem_*, chan/sleep

## 2. Toolchain

| Tool | Function |
|---|---|
| gtc | compiler/interpreter (--c/--run/--check/--lint/--emit-llvm/--test/--watch) |
| gtfmt | formatter (after full check) |
| gtc --lint | static check (unused fn/var/param, unreachable, empty if, const cond, self-compare) |

## 3. Not Implemented / Not Planned

| Item | Notes |
|---|---|
| Inline assembly | removed |
| async/await | overlaps with go |
| Full GC instrumentation | RC base ready, instrumentation pending |
| Cross-platform cross-compilation | currently Windows |
| Procedural macros | declarative macros + @derive only |
| Full HM inference | local inference currently |

## 4. Tests

**613 tests** (17 unit + 96 dual-backend consistency + 500 frontend bulk),
cargo test --release all green, 0 warnings.
