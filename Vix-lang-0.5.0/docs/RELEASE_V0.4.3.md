# Vix Language v0.4.3 Release Notes

## Release Date: June 19, 2026

## Overview

Vix 0.4.3 focuses on fixing struct-related code generation bugs, improving array indexing with struct elements, and enhancing semantic analysis accuracy.

## Bug Fixes

### For-Range Loop

- **Fixed for-range direction bug**: `for (i in 1..n)` now uses half-open ascending range semantics (`<` comparison) instead of bidirectional logic, simplifying behavior and fixing edge cases with empty ranges (e.g., `1..0` correctly executes zero iterations)

### Array Indexing with Struct Elements

- **Struct element type hints**: `computeIndexPtr` and `visitIndexAssign` now check `pointerElementHints` before falling back to `getPointerElementTypeSafely`, fixing type resolution when indexing arrays of structs
- **Pointer-to-struct indexing**: `visitIndex` now correctly propagates struct type hints through PHI nodes when indexing pointer-to-struct arrays, using `storageElemType` for null values and GEP operations

### Struct Push Operations

- **Struct push codegen**: Array `push()` with struct values now heap-allocates the struct and stores a pointer with proper type hints, instead of attempting inline struct storage which caused LLVM type mismatches

### Function Argument Passing

- **Struct-to-ptr argument conversion**: Struct values are now automatically converted to pointers when passed to `ptr`-typed function parameters, with proper `pointerElementHints` tracking

### Array Length Inference

- **Extended array length inference**: `inferArrayLengthFromArgument` now handles pointer arguments, global variables, member access expressions, and index expressions for `.len` resolution

### Semantic Analysis

- **Struct literal variable tracking**: `is_variable_used_in_node` now correctly detects variable usage inside struct literals (e.g., `State{tape: tape, ptr: 0}`)
- **Struct literal unused variable check**: `check_unused_variables_with_usage` now traverses struct literal fields to avoid false "unused variable" warnings

## New Regression Tests

- **test404.vix**: Validates for-range edge cases — empty range `1..0`, single-iteration `1..1`, and normal range `1..2`
- **test405.vix**: Tests struct member array indexing, mutation via pointer methods, and struct field state tracking

## Build System

- Moved `examples/Coreutils/echo.vix` to `examples/coreutils/echo.vix` (lowercase directory)

## Known Issues

- Some ownership tests correctly fail (expected behavior for error detection)
- Some import tests fail due to missing module files (not compiler bugs)
