# Vix Language v0.2.9 Release Notes

**Release Date:** 2026-05-31

## Diagnostics

### Array parameter value-semantics warnings
- Added compile-time warnings for array parameters that are passed by value and then modified inside a function.
- The compiler now warns when a copied array parameter is modified without returning the modified array, helping catch code that accidentally assumes reference semantics.
- The compiler now warns when an array parameter length is read before modifying the copied parameter, highlighting stale-length patterns such as returning the old length after `push`.
- The compiler now warns when an array parameter is never used.
- Files: `src/Typeck/Typeck.cpp`, `tests/test_types.py`

### Cleaner warning output
- Improved warning formatting so multiline diagnostic messages render separate `note` lines instead of being flattened into one long line.
- Warning headers now use a more readable `warning: ...` plus `--> file:line:column` layout.
- Warning summaries now report the total warning count as `Found N warnings`.
- Removed `help` text from warnings to avoid suggesting unsupported syntax or language features.
- Files: `src/utils/error.c`, `include/compiler.h`, `src/main.c`

## Code Generation

### Global dynamic array push now writes back reallocated pointers
- Fixed `array.push(...)` on global dynamic arrays so the data pointer returned by `realloc` is stored back into the global variable.
- Fixed pushed struct elements so arrays such as `[Point]` allocate using the struct element size and store the struct value, not a pointer byte sequence.
- This fixes programs that build a global array with repeated `push` calls and then iterate it with `for`.
- Files: `src/compiler/Funcs.cpp`

## Bug Fixes

### For-loop numeric type promotion
- Fixed a type mismatch error when using a `for` loop with a range where the start and end are different numeric types (e.g., `0 .. bytes_read` where `0` is `I32` and `bytes_read` is `I64`).
- The compiler now allows implicit numeric promotion between integer and float types in range expressions.
- Modified `src/Typeck/Typeck.cpp` to use `promote_numeric` for range bounds.
- Modified `include/unify.h` to allow numeric type compatibility in unification.
- Files: `src/Typeck/Typeck.cpp`, `include/unify.h`

## Example

Given:

```vix
fn add_point(points: [Point], p: Point): i32 {
    let id = points.length
    points.push(p)
    return id
}
```

`vixc` now reports that `points` is copied, its mutation is lost, and the earlier length read may be stale relative to the copied mutation.

## Validation

- Rebuilt `vixc` successfully.
- Verified `src/test.vix` emits the new array-parameter warnings.
- Verified `src/test.vix` now compiles successfully (only an unused variable warning remains).
- Verified warning output no longer emits `help` lines or `#[warn(unused_variables)]` text.
- Verified `src/test2.vix` now prints the pushed global `Point` values instead of producing no output.
- Verified standalone fixtures for copied-array mutation and unused array parameter warnings.
- Python test execution could not be run in this environment because `pytest` is not installed.
