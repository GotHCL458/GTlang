# Vix 0.4.0-dev Release Notes

## Overview

Vix 0.4.0-dev removes the legacy `[]` bracket generic syntax and standardizes on `<>` angle bracket syntax for all generic constructs. This is a breaking change that simplifies the language grammar and improves consistency.

## Breaking Changes

### Removed `[]` Generic Syntax

The `:[T]` bracket syntax for generics has been completely removed. All generic constructs now use `<>` angle brackets exclusively.

**Before (removed):**
```vix
// Type definitions
type Result:[T, E] = Ok(T) | Err(E)
struct Box:[T] { value: T }

// Function definitions
fn id:[T](x: T): T { return x }
fn add:[T](a: T, b: T): T { return a + b }

// Function calls
let x = id:[i32](42)
let y = add:[i32](4, 6)

// Struct literals
let b = Box:[i32]{ value: 10 }

// Type references
let arr: Box:[i32] = b
```

**After (required):**
```vix
// Type definitions
type Result<T, E> = Ok(T) | Err(E)
struct Box<T> { value: T }

// Function definitions
fn id<T>(x: T): T { return x }
fn add<T>(a: T, b: T): T { return a + b }

// Function calls
let x = id<i32>(42)
let y = add<i32>(4, 6)

// Struct literals
let b = Box<i32>{ value: 10 }

// Type references
let arr: Box<i32> = b
```

## Migration Guide

To migrate existing code:

1. Replace all `:[` with `<` and corresponding `]` with `>`
2. Update type references from `TypeName[T]` to `TypeName<T>`
3. Ensure nested generics like `Box<Box:[i32]]` become `Box<Box<i32>>`

## What Changed

- **Parser**: Removed 16 grammar rules for `:[T]` bracket syntax
- **Examples**: Updated 5 example files to use `<>` syntax
- **Tests**: Updated 32 regression test files to use `<>` syntax
- **Documentation**: Updated release notes and compiler documentation

## Compatibility

- The standard library (`std/`) already uses `<>` syntax
- The bootstrap compiler (`vixc0/`) already uses `<>` syntax
- All existing code using `:[T]` syntax must be updated to compile

## Known Issues

- None reported for this change

## Contributors

- Daweidie