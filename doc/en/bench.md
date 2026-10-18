# GTLang Benchmarks

> [中文](../bench.md)

> Env: Windows x64, clang 23.1 (-O2), rustc 1.98 (-O), Python 3.12. 2026-09-27.

## 1. Tests

| Test | Algorithm |
|---|---|
| fib(35) | naive recursion (~30M calls) |
| loop_sum | for i in 0..200000000 (2e8 iterations) |

**fib(35) measured (2026-09)**:

| Version | Time |
|---|---|
| GTLang default (overflow checks) | ~43 ms |
| GTLang `--no-overflow-check` | ~33 ms |
| C (clang -O2) | ~26 ms |
| GTLang -O4 (raw i64) | ~26 ms |

fib is **recursive** (`n-1`/`n-2`); range analysis cannot prove non-overflow, so every call carries 2 checks — the price of safety. Use `--no-overflow-check` to disable (wraps on overflow).

## 2. Results (ms)

| Lang / Impl | fib(35) | loop_sum(2e8) | Notes |
|---|---|---|---|
| C (clang -O2) | ~22 | ~5 | vectorized |
| Rust (rustc -O) | ~24 | ~5 | wrapping_add |
| nasm (hand-written) | ~36 | ~50 | no optimization |
| **GTLang -O4 (raw i64)** | **~26** | **~4.2** | raw i64, overflow wraps (C semantics) |
| GTLang -O2 | ~30 | ~5 | |
| GTLang -O0 | ~40 | ~40 | |
| Node.js 24 (V8 JIT) | ~144 | ~195 | JIT |
| Lua 5.4 | ~445 | ~673 | interpreter |
| Python 3.12 | ~1300 | ~6788 | interpreter |

> **Raw i64 (C semantics)**: GTLang's `int` is a raw 64-bit integer — overflow wraps (same as C). Tight loops compile to plain LLVM `add/sub/mul` with zero runtime overhead, so `loop_sum` matches C (~4.2ms) and `fib` comes close (~26ms).
>
> **The optimizer works**: `-O0 -> -O4` cuts loop_sum from ~40 to ~4.2 ms, fib from ~40 to ~26 ms (constant folding / inlining / DCE).

## 3. Analysis

GTLang's range analysis (src/range.rs) is flow-sensitive: when an Add/Sub/Mul
is provably non-overflowing (e.g. for i in 0..N with monotone accumulation
s = s + i), the overflow check is elided while keeping semantics safe.

- Integer arithmetic is raw i64 (`add/sub/mul`), no runtime overhead; overflow wraps (C semantics).
- `loop_sum` is ~4.2 ms (-O4), on par with C (~5 ms).

## 4. Disabling Overflow Checks

    gtc bench/xxx.gt -o x.exe -O2                     # default (checked + range analysis)
    gtc bench/xxx.gt -o x.exe -O2 --no-overflow-check # disabled

| Version | fib(35) | loop_sum(2e8) |
|---|---|---|
| GTLang default (range analysis) | ~40 ms | 5.9 ms |
| GTLang --no-overflow-check | ~25 ms | ~6 ms |
| C (clang -O2) | ~26 ms | ~5.5 ms |

## 5. Conclusion

- Default (safe): loop matches C; fib slightly slower (overflow checks)
- Range analysis: key optimization; safe checks elided only when provable
- --no-overflow-check: further speedup (silent wraparound)

## 6. Concurrency & compile speed

**`go` dispatch**: uses a **fixed worker thread pool** (not thread-per-task).
2000 `go` tasks total ~7 ms of dispatch overhead.

**Compile speed** (`gtc --check`, Windows x64): near-linear.

| Size | `--check` time |
|---|---|
| 300 fns | ~11 ms |
| 2000 fns | ~43 ms |
| 5000 fns | ~82 ms |

**Set / Map**: open-addressing hash, amortized O(1) (previously linear scan).

---

## 7. Reproduce

See bench/ (bench.c / bench.rs / bench.lua / bench.py).
