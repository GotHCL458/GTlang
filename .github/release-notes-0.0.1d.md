# GTLang v0.0.1d — Early Preview

> ⚠️ **Early-stage project.** APIs, syntax, and the standard library may change
> without notice. Bugs and missing features are expected.
>
> 🪟 **Windows x64 only (for now).** Linux/macOS support is not yet available.

**GTLang** is a statically-typed, compiled, expression-oriented language with
first-class Chinese identifiers, dual backends (LLVM + Cranelift), and memory
safety.

---

## ✨ Highlights

### Functions are first-class
A top-level `fn` name is now a value — pass it as an argument, store it in a
variable, return it from a function, put it in a container, or pick between two
functions with `if`/`match`:

```gt
fn inc(x: int) -> int { x + 1 }
fn dbl(x: int) -> int { x * 2 }

fn apply(f, x: int) -> int { f(x) }

fn main() {
    put(apply(inc, 10))              // 11
    g := inc                         // store
    put(g(5))                        // 6
    pick := if true { inc } else { dbl }
    put(pick(20))                    // 21
    fs := list(); push(fs, inc)
    put(fs[0](7))                    // 8  (any expression as callee)
}
```

### Closures capture more
Closure bodies now see the types of captured containers/strings, and
`return |x| g(f(x))` captures the outer parameters `g`/`f`.

### Ergonomic borrows & richer patterns
`&T`/`&mut T` auto-derefs for fields, methods and params; `struct` args
auto-box into `dyn Trait`; `match` supports ident bindings, nested
destructuring (`Some(Some(v))`), and `x or default` works for `Result` too.

### Friendlier errors
`loop {` (missing count) now says *loop requires a count: loop N { ... }*.

## 🐛 Fixes

- **Windows newline consistency** — JIT and AOT now emit identical bytes
  (`\n`, not `\r\n`). The consistency test no longer normalizes CRLF, so this
  class of bug is caught in the future.
- Calling a closure **returned by a function**; **higher-order** closure params
  (`g(f(x))`); closures called inside `if`/`match`/`slice`/`tuple`/`struct`.
- **JIT** `CallValue` uses the callee's signature (fixes function factories).
- **`mono`** substitutes the inferred `ret_ty` (generic multi-instantiation).
- Deep nesting / long binary chains report a syntax error instead of overflowing
  the stack; `-9223372036854775808` (i64::MIN) parses.
- `gtlib/random` RNG seed is per-thread (no shared sequence under `go`).
- **Pipe `|>` to a non-callable** no longer silently drops the LHS.
- **`if` on the same line as the previous statement** no longer misparsed as a ternary.
- **`"${x}"` (single interpolation)** no longer degrades to `x`.
- **Non-enum `match`** always requires `_` (empty match rejected).
- **Composite-type match patterns** rejected in both backends.
- **Nested enum payload destructuring** no longer crashes / mis-binds inner tags.
- **Method calls on borrows** (`a.方法()` where `a: &T`) across sema/mono/LLVM.
- **`Option[T]` / generic-struct `Box[T]` bracket types** now parse correctly.
- **`x or y` works for `Result`** too.
- `ariadne` spans align to UTF-8 char boundaries; **deep nesting** reports an error (no stack overflow).

## 📈 Performance

- **`go` thread pool** in the JIT (was thread-per-task): 2000 tasks in ~7 ms.
- **Set / Map** are open-addressing hash tables (amortized O(1)).
- Compile speed is near-linear (5000 top-level fns ≈ 82 ms `--check`).

## 🧪 Tests

**814 tests**: 142 unit + 149 dual-backend consistency + 522 frontend bulk.
0 warnings. Includes new **AST traversal completeness** tests.

---

## 📦 Bundle

`gtlang-res-win-x64.zip` — a portable, no-install bundle (`gtc.exe` /
`lib/` / `runtime/` / `tcc/`). **Not bundled**: `clang` / `lld-link`; GTLang locates
them on `PATH` (or via `GTC_CLANG`).

## 🔧 Build

```bat
build.bat        REM needs Rust >= 1.75 and LLVM/clang >= 15 on PATH
```

## 📚 Docs

- `doc/LANGUAGE.md` — language reference (incl. closures & first-class functions)
- `doc/STDLIB.md` — standard library
- `doc/PERFORMANCE.md` · `doc/bench.md` — performance
- `doc/syntax_status.md` — feature status


