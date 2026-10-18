# GTLang Performance

> [中文](../PERFORMANCE.md)

> LLVM native backend (no interpreter) | opt levels -O0..3

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
| Set insert/has | amortized O(1) | open-addressing hash table (power-of-two) |
| Map get/insert | amortized O(1) | open-addressing hash (Fibonacci hashing) |
| String concat | O(n) | StringBuilder (rt_sb_*) |
| String find | O(n*m) | |
| Channel send/recv | O(1) | provided by GTO (`gto.dll`) |
| `go` dispatch | ~µs | fixed worker thread pool (not thread-per-task) |
| Integer arithmetic | O(1) | raw i64, native add/sub/mul |
| Decimal arithmetic | O(digits²) | exact base-10 (`decimal`) |

## 3. Backend

GTLang has a single **LLVM native backend**: programs compile via clang to a standalone executable — no interpreter/JIT. The standard library ships as shared DLLs loaded on demand at startup (zero `.lib`); the artifact depends only on system DLLs.

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
| decimal | heap | process exit |

Note: RC base (gt_rc_inc/dec) implemented; instrumentation pending.

## 5. vs Python (theoretical)

| Aspect | GTLang | Python |
|---|---|---|
| Type | static | dynamic |
| Execution | native machine code | bytecode interpreter |
| Loop | native | interpreter overhead |
| Function call | direct | dynamic lookup |
| Integer | raw i64 (wraps on overflow) | arbitrary precision |
| Decimal | built-in `decimal` | `decimal` module |
| Expected | **10-100x** Python (numeric) | baseline |

## 6. Benchmarks

    gtc bench/xxx.gt -o x.exe -O2 # compile -O2
    gtc bench/xxx.gt -o x.exe -O3 # compile -O3
