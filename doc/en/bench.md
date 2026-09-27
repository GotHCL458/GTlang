# GTLang Benchmarks

> [中文](../bench.md)

> Env: Windows x64, clang 23.1 (-O2), rustc 1.98 (-O), Python 3.12. 2026-09-27.

## 1. Tests

| Test | Algorithm |
|---|---|
| fib(35) | naive recursion (~30M calls) |
| loop_sum | for i in 0..200000000 (2e8 iterations) |

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

## 6. Reproduce

See bench/ (bench.c / bench.rs / bench.lua / bench.py).
