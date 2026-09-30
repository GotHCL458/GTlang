# Vix Language v0.3.0 Release Notes

**Release Date:** 2026-06-07

## New Features

### Lightweight Owner system

Vix v0.3.0 introduces the first version of the lightweight Owner system: a compile-time ownership and borrow checker designed to provide most of Rust-style memory safety with a much simpler language model.

The goal of this release is practical safety without Rust-level complexity:

- Ownership transfer for non-Copy values.
- Exclusive mutable references.
- Shared immutable references.
- Use-after-move detection.
- Dangling local reference detection.
- Copy-type inference for small/simple values.
- Statement-scoped temporary borrows as a simplified “last use” model.

The Owner pass runs after type checking, using inferred AST type information from `typecheck_program(...)`.

Files:

- `include/ownership.h`
- `src/Ownership/Ownership.cpp`
- `src/main.c`
- `CMakeLists.txt`

### New reference syntax: `ref` and `mut ref`

References now use explicit keyword syntax instead of the old `&` spelling.

Immutable shared reference:

```vix
fn read(x: ref i32): i32
{
    return @x
}

fn main(): i32
{
    let value = 10
    print(read(ref value))
    return 0
}
```

Mutable exclusive reference:

```vix
fn inc(mut p: ref i32)
{
    @p = @p + 1
}

fn main(): i32
{
    let mut value = 10
    inc(mut ref value)
    print(value)
    return 0
}
```

Rules:

- `ref value` creates an immutable shared borrow.
- `mut ref value` creates a mutable exclusive borrow.
- A value may have many immutable borrows in the same statement.
- A mutable borrow cannot overlap another mutable or immutable borrow in the same statement.
- Temporary borrows are released at the end of the statement.

The parser still maps references internally to the existing pointer AST representation, so existing code generation can continue to reuse pointer lowering.

Files:

- `src/parser/lexer.l`
- `src/parser/parser.y`
- `src/Ownership/Ownership.cpp`

### Move checking for non-Copy values

Non-Copy values are moved by ownership-transfer operations such as assignment into another variable.

Example rejected by the Owner checker:

```vix
fn main(): i32
{
    let xs = [1, 2, 3]
    let ys = xs
    return xs[0]
}
```

Diagnostic:

```text
error [SemanticError]: use of moved value 'xs'
```

Legal last-use move:

```vix
fn take(xs: [i32]): i32
{
    return xs[0]
}

fn main(): i32
{
    let xs = [1, 2, 3]
    let ys = xs
    return take(ys)
}
```

### Copy type inference

The Owner system uses a heuristic Copy model instead of requiring explicit user annotations.

Currently treated as Copy:

- Integer types.
- Floating-point types.
- Boolean values.
- References/pointers.
- Function values.
- Strings as lightweight reference-like values.
- Struct values in the current lightweight model.
- Small fixed-size arrays when their element type is Copy.
- Uppercase enum-like constructors/constants.

Currently treated as non-Copy:

- Dynamic arrays.
- Generic application/container values by default.

This keeps common Vix code ergonomic while still catching important ownership errors around arrays, containers, references, and moves.

### Dangling reference checks

The Owner checker rejects returning a reference to a local variable.

Example rejected:

```vix
fn bad(): ref i32
{
    let local = 99
    return ref local
}
```

Diagnostic:

```text
error [SemanticError]: cannot return reference to local variable 'local'
```

### Optional references

Optional references can be expressed with `?ref T`.

```vix
fn maybe_ref(flag: i32, value: ref i32): ?ref i32
{
    if (flag == 0)
    {
        return None
    }
    return Some(value)
}
```

This provides a simple way to model “maybe a reference” without explicit lifetime annotations.

## Diagnostics

### Ownership diagnostics now include file, line, and column

Example:

```text
error [SemanticError]: use of moved value 'xs'
  --> src/test.vix:15:11
   |
14 |     print(ys[0])
15 |     return xs[0]
   |           ^
```

Fixes:

- Ownership errors now fall back to `current_input_filename` when AST nodes do not have `source_file` set.
- Ownership errors now call `set_location_with_column(...)` before reporting.
- Parser-generated unary reference/deref nodes now preserve source locations through `create_unaryop_node_with_yyltype(...)`.

Files:

- `src/Ownership/Ownership.cpp`
- `src/parser/parser.y`

## Examples and Tests

### Updated examples for `ref`

The following examples were updated from old `&` reference syntax to `ref`:

- `examples/quicksort.vix`
- `examples/swap.vix`
- `examples/pointer.vix`
- `examples/pointer2.vix`
- `examples/nullptr.vix`
- `examples/list.vix`

### Ownership test fixture

`src/test.vix` now acts as a negative Owner-system test file.

It intentionally triggers:

- Use-after-move.
- Duplicate mutable borrow with `mut ref`.
- Returning a reference to a local variable.

It also contains legal comparison functions for:

- Last-use move.
- Copy values.
- Immutable shared borrows.

### Python tests

Added ownership-focused tests covering:

- Move consumes non-Copy values.
- Last-use move is allowed.
- Copy values remain usable after assignment.
- Duplicate mutable borrow is rejected.
- Returning local references is rejected.

File:

- `tests/test_ownership.py`

Updated existing test strings for `ref` syntax:

- `tests/test_types.py`
- `tests/feat.py`

## Compatibility Notes

### What v0.3.0 intentionally does not include

The Owner system is intentionally smaller than Rust’s ownership model.

Not included:

- Explicit lifetime annotations such as `<'a>`.
- Full non-lexical lifetime analysis.
- Pin/Unpin.
- Drop checking.
- Higher-ranked trait bounds.
- Full support for complex self-referential structures.

### Current simplifications

- Temporary borrow scopes are statement-based.
- Function arguments are treated as read/borrowed by default for compatibility with existing Vix code.
- The Copy model is heuristic and may evolve as the type system gains explicit Copy/Move traits.
- References are still lowered through the existing pointer representation internally.

## Validation

- Rebuilt `vixc` successfully with the new `Ownership.cpp` compilation unit.
- Verified `src/test.vix` emits the expected Owner errors.
- Verified ownership diagnostics include real file names and columns.
- Verified `ref value` permits shared immutable borrows.
- Verified `mut ref value` enforces mutable borrow exclusivity.
- Verified updated examples pass `--check`:
  - `examples/quicksort.vix`
  - `examples/swap.vix`
  - `examples/pointer.vix`
  - `examples/pointer2.vix`
  - `examples/nullptr.vix`
  - `examples/list.vix`
- Verified regression sources no longer fail with ownership-specific `moved`/`borrow` diagnostics after compatibility adjustments.
- Full Python test execution was not run in this environment because `pytest` is not installed.
