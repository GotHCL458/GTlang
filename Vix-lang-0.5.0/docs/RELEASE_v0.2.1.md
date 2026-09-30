# Vix v0.2.1 Release Notes

**Type System Hardening & Cleanup Release**

## Breaking Changes

### 1. Removed `str` Keyword

The `str` type keyword has been removed. Use `string` instead.

**Before (v0.2.0):**
```vix
let name: str = "hello"
fn greet(name: str) { }
```

**After (v0.2.1):**
```vix
let name: string = "hello"
fn greet(name: string) { }
```

### 2. Added `usize` Keyword

`usize` is now a recognized type keyword, aliased to `i64` (64-bit unsigned size type).

```vix
let len: usize = 42
```

## Bug Fixes

### Type Checker Fixes

- **Fixed `check_assign` return type**: Assignment expressions now correctly return `Void` instead of the RHS type. Previously, when an assignment was the last expression in a block, the block's type would incorrectly become the RHS type, causing false type mismatches (e.g., `fn swap(mut a: &i32, mut b: &i32) { ... @b = temp }` reported "I32 vs Void").

- **Fixed extern block scope isolation**: Functions declared inside `extern "C" { ... }` blocks are now properly registered in the outer scope. Previously, extern function declarations were lost when the block's scope exited, causing undefined function errors when importing standard library files like `std/io.vix`.

- **Fixed function type pre-declaration for nested programs**: Functions inside nested `PROGRAM` nodes (extern blocks) are now pre-declared alongside top-level functions, ensuring correct type resolution during the main checking pass.

### Standard Library Fixes

- **Fixed `std/io.vix` compilation**: Removed recursive wrapper functions that shadowed extern declarations (e.g., `pub fn fopen` calling itself instead of the extern `fopen`). Removed unsupported `usize` type from function signatures (now uses `i64`). Fixed `panic` function return type from `i32` to `void`.

### Example Fixes

All 41 example files now compile successfully:

- **`fib.vix`**: Added `let mut` for mutable loop variables `a` and `b`
- **`swap.vix`**: Fixed implicitly void return type handling
- **`struct2.vix`**: Moved module-level code into `main()` function
- **`strcmp.vix`**: Added `let mut` for loop variable; added missing return after `while(1)` loop
- **`arr.vix`**: Added `let` declarations for variables
- **`function_arg.vix`**: Fixed `f32`/`f64` return type mismatch
- **`if.vix`**: Replaced `input()` call with string literal (input returns void)
- **`lexer.vix`**: Replaced `&&`/`||` operators with `and`/`or` keywords; used `elif` instead of `else if`
- **`list_op.vix`**: Replaced unsupported `add!()` syntax with array indexing
- **`quicksort.vix`**: Added complete quicksort implementation (was empty)
- **`error_test.vix`**: Fixed syntax for struct literal inside function
- **`fileio.vix`**, **`generics.vix`**, **`import_test.vix`**, **`import_test2.vix`**: Fixed import paths
- **`match_strings.vix`**, **`global.vix`**: Replaced `str` with `string`
- **`string_arr.vix`**: Moved module-level arrays into `main()` function
- **`lambda.vix`**: Rewrote with function pointer pattern
- **`net_test.vix`**: Removed unavailable net module import
- **`pointer.vix`**: Fixed pointer mutation example (added `let mut`)

## New Features

### Type System Tests

Added `tests/test_types.py` with 65 comprehensive type system tests covering:

- **Basic Type Annotations** (6 tests): `i32`, `i64`, `f32`, `f64`, `string`, `bool`
- **Type Inference** (5 tests): Integer, string, float, expression, and function call inference
- **Function Type Signatures** (7 tests): Void, i32, string, pointer, f64 params, no return, multi-params
- **Numeric Type Promotion** (2 tests): i32-to-i64 promotion, literal in typed context
- **Pointer Types** (4 tests): Address-of, dereference, mutation via pointers, swap
- **Array Types** (4 tests): Fixed array, dynamic array, mutation, string arrays
- **Struct Types** (4 tests): Definition, string fields, mutation, nested structs
- **ADT Types** (3 tests): Option, Result, custom enum
- **String Type** (6 tests): Literal, variable, annotation, length, index, print
- **Extern Function Types** (2 tests): Basic extern call, varargs
- **Type Error Detection** (5 tests): Type mismatch, immutable assignment, undefined variable/function, missing main
- **Compound Assignment** (5 tests): `+=`, `-=`, `*=`, `/=`, `%=`
- **Void Type** (2 tests): Void function as statement, void without return
- **Implicit Return Type** (2 tests): Function without return type, body ending with void
- **Type Compatibility** (4 tests): i32 arithmetic, comparisons, string comparison, logical operators

### Import Path Support

Added `examples/std` symlink to `../std` so that examples using `import "std/io.vix"` resolve correctly.

## Migration Guide

1. Replace all `str` type annotations with `string`.
2. Replace `usize` type annotations with `i64` (or use the new `usize` keyword).
3. Ensure functions without explicit return types end with void-compatible expressions (not value-returning assignments).
4. For extern blocks, ensure function names don't shadow public wrapper functions with the same name.

## Test Results

- 512/512 feature tests passing
- 65/65 type system tests passing
- 220/220 regression tests passing
- 15/15 error handling tests passing
- **812 total tests passing**
- 41/41 example files compiling successfully
