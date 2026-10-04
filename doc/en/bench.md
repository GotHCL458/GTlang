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
| GTLang JIT | ~62 ms |

fib is **recursive** (`n-1`/`n-2`); range analysis cannot prove non-overflow, so every call carries 2 checks — the price of safety. Use `--no-overflow-check` to disable (wraps on overflow).

## 2. Results (ms)

| Lang / Impl | fib(35) | loop_sum(2e8) | Notes |
|---|---|---|---|
| C (clang -O2) | ~26 | ~5.5 | vectorized |
| Rust (rustc -O) | ~28 | ~8 | wrapping_add |
| GTLang (-O2, checked) | ~40 | 5.9 | overflow checks + range analysis |
| Node.js 24 (V8 JIT) | ~144 | ~195 | JIT |
| Lua 5.4 | ~445 | ~673 | interpreter |
| Python 3.12 | ~1300 | ~6788 | interpreter |

## 3. Analysis

GTLang's range analysis (src/range.rs) is flow-sensitive: when an Add/Sub/Mul
is provably non-overflowing (e.g. for i in 0..N with monotone accumulation
s = s + i), the overflow check is elided while keeping semantics safe.

- loop_sum(2e8): 5.9 ms (matches C's 5.5 ms)
- covers Stmt::Assign arithmetic (bare s = s + i and compound s += i)

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

**`go` dispatch**: both AOT and JIT use a **fixed worker thread pool** (not
thread-per-task). In the JIT, 2000 `go` tasks total ~7 ms of dispatch overhead.

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
