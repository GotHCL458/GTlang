# GTLang Language Reference

> [中文](../LANGUAGE.md)

> Version 0.0.1d | Compiler: gtc (dual backend) | Encoding: UTF-8 (auto-detected)

---

## 1. Lexical

| Element | Syntax | Notes |
|---|---|---|
| Comments | // ... , # ... | line |
| Doc comments | /// ... | |
| Identifiers | letters/underscore/any Unicode (incl. Chinese) | 累加, 计数器, x |
| Integers | 123, 0xFF, 0b1010, 0o17, 1_000 | i64 |
| Floats | 3.14, 1.5e-3 | f64 |
| Bools | true, false | bool |
| Strings | "..." | interpolation: $name / \${expr} |
| Raw strings | r"..." | no escapes, no interpolation |

---

## 2. Variables

    a = 1          // bare: auto-declare + infer (type may change)
    x := 2         // inferred declaration
    let mut y = 3  // inferred, mutable
    let n: int = 4 // explicit fixed type
    const K = 5    // constant

| Declaration | Type | Reassign type |
|---|---|---|
| a = v | inferred | yes |
| x := v | inferred | yes |
| let mut y | inferred | yes |
| let n: T | explicit | no |
| const K | fixed | no |

---

## 3. Types

| Type | Syntax | Semantics |
|---|---|---|
| Integer | int (i64) | value |
| Float | f64 / float | value |
| Bool | bool | value |
| String | str / string | heap (NUL-terminated) |
| Array | [T; N] | value (stack) |
| List | list / List[T] | reference |
| Set | set | reference |
| Map | map / dict | reference |
| Tuple | (T1, T2) | value (heap block, t.0) |
| Struct | struct Name { ... } | value (stack) |
| Enum | enum Name { V1(T) V2 } | reference (heap block [tag, payload]) |
| Result | Result[T, E] | reference |
| Option | ?T / Option[T] | reference |
| Borrow | &T / &mut T | - |
| Trait object | dyn Trait | reference (vtable) |
| Closure | |x| ... | value ([fn_ptr, caps]) |

---

## 4. Control Flow

    if c { ... } elif c2 { ... } else { ... }
    loop 3 { ... }              // count loop
    while c { ... }
    do { ... } while c
    for i in 0..n { ... }
    for v in container { ... }
    for ... else { ... }
    outer: for ... { break outer }

    match v {
        1 => { ... }
        1..10 => { ... }
        1 | 2 | 3 => { ... }
        n if n > 0 => { ... }
        _ => { ... }
    }

Exhaustiveness: enum must list all variants / bool needs true+false / Option needs Some+None / Result needs Ok+Err (or _).

---

## 5. Functions

    fn add(a: int, b: int) -> int { a + b }   // last expr = return
    fn f(a, b) { ... }                        // types may be inferred
    fn g(x: int = 10) { ... }                 // default args
    g(x: 5)                                   // named args
    fn id[T](x: T) -> T { x }                // generics
    fn max[T](a: T, b: T) -> T where T: Ord { ... }
    |x| x * 2                                 // closure
    |x: int| -> int { x * x }                 // annotated closure

### Closures & first-class functions

    // closure captures an outer variable
    n := 10
    f := |x: int| x + n
    f(5)                       // 15

    // functions are first-class: a top-level fn name is a value
    fn inc(x: int) -> int { x + 1 }
    g := inc                   // store in a variable
    g(10)                      // 11
    apply(inc, 5)              // pass as an argument (higher-order)
    pick := if true { inc } else { dbl }    // if/match branch
    fs := list()               // put in a container
    push(fs, inc)
    fs[0](5)                   // any expression as callee

    // a function returning a closure
    fn mul(n: int) { return |x: int| x * n }
    m3 := mul(3)
    m3(10)                     // 30

    // higher-order: an unannotated closure param infers as a closure
    fn fold(f, init: int, xs: list) -> int {
        acc := init
        i := 0
        while i < len(xs) { acc = f(acc, xs[i])  i = i + 1 }
        return acc
    }
    fold(|a: int, b: int| a + b, 0, xs)

### Ergonomic borrows

    struct Point { x: int, y: int }
    p := Point { x: 1, y: 2 }
    r := &p
    r.x                      // same as p.x (field access auto-derefs)
    r.y = 9                  // &mut can write fields too

    trait Area { fn area(self) -> int }
    impl Area for Circle { fn area(self) -> int { ... } }
    fn f(s: dyn Area) -> int { return s.area() }   // dynamic dispatch (vtable)
    f(Circle { ... })        // struct args are auto-boxed into dyn Trait

### Destructuring & defaults

    match o {
        Some(Some(v)) => { ... }        // nested destructuring
        Ok(Some(v))   => { ... }
        n if n > 0    => { ... }        // ident binding + guard
        _ => { ... }
    }

    v := o or 0                         // Option default (Some(v)->v, None->0)

---

## 6. Struct / Enum / trait

    struct Point { x: int, y: int }
    p := Point { x: 1, y: 2 }

    enum Shape { Circle(f64) Rect(f64, f64) Unit }
    match s { Shape::Circle(r) => { ... } _ => { ... } }

    trait Area { fn area(self) -> f64 }
    impl Area for Circle { fn area(self) -> f64 { ... } }
    fn f(s: dyn Area) { s.area() }            // dynamic dispatch

    @derive(Eq, Clone, Debug, Default, Hash, Ord, PartialEq, Display)
    struct Point { x: int }

@derive methods:

| Derive | Methods | Notes |
|---|---|---|
| Eq | eq / ne | field-wise |
| PartialEq | eq | eq only |
| Clone | clone | field copy |
| Debug | to_str | "Name { f: v }" |
| Display | to_str | "v1, v2" |
| Default | default | zero values |
| Hash | hash | FNV mix |
| Ord | cmp/lt/le/gt/ge | lexicographic + < <= > >= |

---

## 7. Operator Overloading

    impl Vec {
        fn add(self, o: Vec) -> Vec { ... }
        fn eq(self, o: Vec) -> bool { ... }
    }
    a + b   // -> Vec__add(a, b)
    a == b  // -> Vec__eq(a, b)

Overloadable: add sub mul div rem eq ne lt le gt ge neg

---

## 8. Concurrency

    go worker(42)
    ch := chan()
    chan_send(ch, v)
    v := chan_recv(ch)
    sleep(500)

---

## 9. C Interop

    C {
        static long long square(long long x) { return x * x; }
        static void cb(const char *s) { gt_report(s); }
    }
    put(square(5))

    extern "C" { fn puts(s: str) -> int }

    // C header import only works for project-local .h files (their signatures are parsed).
    import c "native.h" as n
    put(n.native_fn(2.0))

    // System headers (e.g. math.h): use an inline C block instead.
    C {
        #include <math.h>
        static double my_sqrt(double x) { return sqrt(x); }
    }
    put(my_sqrt(2.0))

---

## 10. Error Handling

    fn f(n: int) -> Result[int, str] {
        if n < 0 { return Err("negative") }
        return Ok(n * 2)
    }
    v := f(21)?
    match f(-1) { Ok(v) => { ... } Err(e) => { ... } }

    o := Some(1)

    try { throw "boom" } expt e { put(e) } fily { ... }

---

## 11. Data Extensions

    t := (1, 2)
    let (a, b) = t
    s[0..3]
    l[-1]
    a, b = b, a
    [x * 2 for x in l if x > 0]

---

## 12. Modules

    import math
    import math.vector
    import "x.gt" as x
    import c "a.h"

---

## 13. Macros

    macro square(x) { (x) * (x) }
    put(square(3))

---

## 14. Builtins

| Category | Functions |
|---|---|
| Output | put / print |
| Convert | str / int / f64 / bool |
| Length | len |
| Math | abs min max sum range |
| Assert | assert |
| Format | pad_left / pad_right / fmt_int |
| String | upper lower trim split join find substr replace repeat |
| Container | push pop insert remove has keys values |
| Concurrency | chan chan_send chan_recv sleep |
| Memory | mem_alloc mem_free mem_store_i64 mem_load_i64 |
| Ternary | a if c else b |

---

## 14b. Stdlib modules

The standard library ships as `*.dll` under `res/lib`; names mirror **Python**.

### os
| Function | Signature | Notes |
|---|---|---|
| `getcwd()` | `() -> str` | current working directory |
| `getenv(name)` | `(str) -> str` | env var (empty if unset) |
| `setenv(k, v)` | `(str, str) -> bool` | set env var |
| `path_exists(p)` | `(str) -> bool` | path exists |
| `is_file(p)` / `is_dir(p)` | `(str) -> bool` | file / dir |
| `getsize(p)` | `(str) -> int` | bytes (-1 on error) |
| `listdir(p)` | `(str) -> str` | entries (`\n` separated) |
| `basename(p)` / `dirname(p)` | `(str) -> str` | file name / parent |
| `path_join(a, b)` | `(str, str) -> str` | join paths |
| `abspath(p)` | `(str) -> str` | absolute path |
| `mkdir(p)` / `rmdir(p)` | `(str) -> bool` | create / remove dir |
| `os_remove(p)` | `(str) -> bool` | remove file |
| `system(cmd)` | `(str) -> int` | run command, exit code |

### json
| Function | Signature | Notes |
|---|---|---|
| `json_dumps(s)` | `(str) -> str` | escape to JSON string |
| `json_loads(s)` | `(str) -> str` | unescape |
| `json_dump(s, path)` | `(str, str) -> bool` | write to file |
| `json_load(path)` | `(str) -> str` | read from file |

### toml
| Function | Signature | Notes |
|---|---|---|
| `toml_loads(text)` | `(str) -> str` | parse to `k=v;k=v` |
| `toml_load(path)` | `(str) -> str` | read from file |

### math / string
**Python-style**: `sqrt` / `pow` / `floor` / `sin` …; `capitalize` / `title` / `zfill` / `isalpha` …

## 15. Ownership

- move: non-Copy values transferred on pass
- borrow: &T (shared) / &mut T (exclusive)
- flow-sensitive NLL: borrow ends at last use; branch join; unreachable after return/break

---

## 16. Memory Safety

- array/list bounds checks
- divide-by-zero checks
- add/sub/mul overflow detection (both backends)

---

## 17. Diagnostics

- stable error codes: E001 (lex) to E8xx (ownership)
- suggestions: "did you mean X?" (Levenshtein <= 2), bilingual
- Help: line
- ariadne source spans
- zh suffix for Chinese diagnostics
- multiple errors (panic-mode recovery in parser)
