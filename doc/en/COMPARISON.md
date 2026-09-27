# GTLang vs Python

> [中文](../COMPARISON.md)

> For Python users: quick overview of differences.

## 1. Overview

| Aspect | GTLang | Python |
|---|---|---|
| Types | **static** (compile-time) | dynamic |
| Execution | **compiled / JIT** (LLVM/Cranelift) | interpreted (CPython) |
| Chinese identifiers | **native** | supported (PEP 3131) |
| Indentation | **braces** | indentation |
| Concurrency | **real threads** (go + chan) | GIL-limited |
| Errors | Result/Option + try/expt | try/except |
| Memory | manual/RC | GC |
| Performance | native code | bytecode |

## 2. Syntax

| Scenario | GTLang | Python |
|---|---|---|
| Variable | x := 1 / a = 1 | x = 1 |
| Constant | const K = 5 | (convention) |
| Type annotation | let n: int = 4 | n: int = 4 |
| Function | fn f(x: int) -> int { x + 1 } | def f(x: int) -> int: return x + 1 |
| Print | put(x) | print(x) |
| Interpolation | "v=$x" / "\${x}" | f"v={x}" |
| List | list() + push | [] + append |
| Dict | map() + insert/get | {} |
| Tuple | (1, 2), t.0 | (1, 2), t[0] |
| Range loop | for i in 0..n | for i in range(n) |
| Ternary | a if c else b | a if c else b |
| Match | match v { 1 => ... _ => ... } | match (3.10+) |
| Enum | enum E { A B } | enum.Enum |
| Class | struct + impl | class |
| trait | trait T + impl T for X | duck typing / ABC |
| Closure | |x| x * 2 | lambda x: x * 2 |

## 3. Key Differences

- **Types**: GTLang compile-time; Python runtime
- **Performance**: GTLang native code; Python bytecode
- **Concurrency**: GTLang real threads; Python GIL
- **Memory**: GTLang struct on stack; Python all heap + GC

## 4. Migration Example

Python:
    def fib(n):
        if n < 2: return n
        return fib(n-1) + fib(n-2)
    print([fib(i) for i in range(10)])

GTLang:
    fn fib(n: int) -> int {
        if n < 2 { return n }
        return fib(n - 1) + fib(n - 2)
    }
    fn main() {
        for i in 0..10 { put(fib(i)) }
    }
