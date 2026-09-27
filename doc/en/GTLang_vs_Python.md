# GTLang vs Python - Detailed Comparison

> [中文](../GTLang_vs_Python.md)

> Based on gtc_rust current implementation vs Python 3.12.

See [COMPARISON.md](COMPARISON.md) for the summary version.

## 1. Overview

| Aspect | GTLang | Python 3.12 |
|---|---|---|
| Type system | static (inference + optional annotations) | dynamic |
| Execution | LLVM exe / Cranelift JIT | CPython interpreter |
| Indentation-sensitive | no (braces) | yes |
| Identifiers | Chinese + any Unicode | Unicode |
| Memory model | value (struct/array) + reference (containers) | all reference |
| Generics | monomorphization | TypeVar |
| Ownership/borrowing | NLL borrow check | none (GC) |
| Error handling | Result + ? propagation | exceptions |

## 2. Variables & Constants

| Feature | GTLang | Python |
|---|---|---|
| Declaration | x := 1 | x = 1 |
| Immutable | let x = 1 | none |
| Explicit mutable | let mut x = 1 | none |
| Constant | const K = 10 | none |
| Type annotation | yes | yes |
| Destructuring | yes | yes |

## 3. Basic Types

| Type | GTLang | Python |
|---|---|---|
| Integer | int (i64, overflow-checked) | int (arbitrary precision) |
| Float | f64 | float |
| Bool | bool | bool |
| String | str (UTF-8) | str (Unicode) |
| Fixed array | [T; N] (value) | none |
| List | list (reference, l[i]) | list |
| Set | set | set |
| Map | map | dict |
| Tuple | (T1, T2) | tuple |
| Enum | enum (with payloads) | Enum |
| Option/Result | Option / Result | Optional |
