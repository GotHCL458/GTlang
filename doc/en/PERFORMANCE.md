# GTLang Performance

> [中文](../PERFORMANCE.md)

> Dual backend: LLVM (compile) + Cranelift (JIT) | opt levels -O0..3

## 1. Pipeline Optimizations

| Optimization | Location | Notes |
|---|---|---|
| Constant folding | opt::inline_and_fold | compile-time |
| Constant propagation | opt | |
| Function inlining | opt | small functions |
| Dead code elimination | opt | |
| Range analysis | range | elides provably-safe overflow checks |
| Redundant load/store elimination | codegen | SSA caching |
| Overflow-check elimination | range | proven-safe only |

## 2. Runtime Characteristics

| Operation | Complexity | Notes |
|---|---|---|
| Array index | O(1) | stack, bounds-checked |
| List push/pop | amortized O(1) | doubling array |
| Set insert/has | O(n) | linear scan |
| Map get/insert | O(n) | linear scan |
| String concat | O(n) | StringBuilder (rt_sb_*) |
| String find | O(n*m) | |
| Channel send/recv | O(1) | unbounded + mutex |

## 3. Backend Comparison

| Aspect | LLVM (--c) | Cranelift (--run) |
|---|---|---|
| Compile speed | slow (clang) | fast (in-memory JIT) |
| Run speed | fast (optimized) | medium (baseline) |
| Artifact | standalone exe | none (in-memory) |
| Use | release | dev/script |

## 4. Memory Management

| Object | Allocation | Reclamation |
|---|---|---|
| Struct | stack | scope exit |
| Fixed array | stack | scope exit |
| Tuple | heap block | process exit (RC base ready) |
| List/Set/Map | heap | process exit (RC base ready) |
| Enum/Result/Option | heap block | process exit (RC base ready) |
| String | heap/static pool | process exit |
| Closure | heap block | process exit |

Note: RC base (gt_rc_inc/dec) implemented; instrumentation pending.

## 5. vs Python (theoretical)

| Aspect | GTLang | Python |
|---|---|---|
| Type | static | dynamic |
| Execution | native machine code | bytecode interpreter |
| Loop | native | interpreter overhead |
| Function call | direct | dynamic lookup |
| Integer | i64 | arbitrary precision |
| Expected | **10-100x** Python (numeric) | baseline |

## 6. Benchmarks

    gtc --run bench/xxx.gt        # JIT
    gtc bench/xxx.gt -o x.exe -O2 # compile -O2
    gtc bench/xxx.gt -o x.exe -O3 # compile -O3
