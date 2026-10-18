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
| GTLang (bignum fast path) | ~43 ms |

fib is **recursive** (`n-1`/`n-2`); range analysis cannot prove non-overflow, so every call carries 2 checks — the price of safety. Use `--no-overflow-check` to disable (wraps on overflow).

## 2. Results (ms)

| Lang / Impl | fib(35) | loop_sum(2e8) | Notes |
|---|---|---|---|
| C (clang -O2) | ~22 | ~5 | vectorized |
| Rust (rustc -O) | ~24 | ~5 | wrapping_add |
| nasm (hand-written) | ~36 | ~50 | no optimization |
| **GTLang -O4 (two-tier int)** | **~113** | **~818** | tagged int + overflow->bignum |
| GTLang -O2 | ~137 | ~909 | |
| GTLang -O0 | ~309 | ~1456 | |
| Node.js 24 (V8 JIT) | ~144 | ~195 | JIT |
| Lua 5.4 | ~445 | ~673 | interpreter |
| Python 3.12 | ~1300 | ~6788 | interpreter |

> **Cost of two-tier int**: GTLang's `int` is an 8-byte tagged value — small ints inline, overflow auto-promotes to bignum (never aborts). This buys safety (no wraparound, no bignum loss) but every arithmetic op in a tight loop pays tagged decode/encode, so it is slower than C/Rust's raw i64. `loop_sum`'s `s = s + i` calls `gt_int_add` 2e8 times — the main cost.
>
> **The optimizer works**: `-O0 -> -O4` cuts loop_sum from 1456 to 818 ms, fib from 309 to 113 ms (constant folding / inlining / DCE).
>
> **Next**: "safe-variable raw lowering" — when range analysis proves a loop variable cannot overflow, use a raw i64 slot (drop tagged overhead), targeting C-level speed.

## 3. Analysis

GTLang's range analysis (src/range.rs) is flow-sensitive: when an Add/Sub/Mul
is provably non-overflowing (e.g. for i in 0..N with monotone accumulation
s = s + i), the overflow check is elided while keeping semantics safe.

- Range analysis currently only elides **bounds checks**; integer arithmetic still goes through tagged `gt_int_*` (to preserve overflow->bignum semantics).
- `loop_sum` is currently ~818 ms (-O4), far slower than C — each `s = s + i` calls `gt_int_add`. The planned "safe-variable raw lowering" will specialize such loops to raw i64, targeting C-level speed.

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
