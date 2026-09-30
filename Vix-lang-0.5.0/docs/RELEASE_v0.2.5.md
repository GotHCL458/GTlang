# Vix v0.2.5 Release Notes

**Comprehensive Parser, ADT Runtime, and Syntax Enhancements Release**

## New Features

### 1. Prefix Logical NOT Operator `!` (v0.2.2)
Added the `!` (logical NOT) prefix operator as an alternative to `== false`:
```vix
let x = true
if (!x) {
    print("x is false")
}
```
Supports double negation: `!!x`.

### 2. Unit Type `()` (v0.2.2)
Added `()` as the unit/void type and value:
```vix
fn do_nothing(): () { }/*void*/

let x: () = ()/*  as a value (nil):  */
```

### 3. Syntax Deprecation Warnings (v0.2.2)
The compiler now emits warnings for deprecated syntax patterns:
- **`struct NAME {...}` syntax**: Use `type NAME = struct {...}` instead.
- **`-> type` return syntax**: Use `: type` instead for function return types.

## Bug Fixes

### Parser & Syntax Fixes
- **Fixed `lvalue` assignment rule**: Replaced `lvalue` in `assignment_statement` with `factor_unary`, resolving a fundamental shift/reduce conflict where complex targets like `arr[0] = ...` would fail.
- **Fixed chained index assignment**: `arr[0][0] = value` syntax now parses correctly.
- **Fixed member access assignment**: `p.x = value` now parses correctly through `factor_unary`.
- **Fixed `type NAME[T] = struct {...}` (v0.2.2)**: Generic struct definitions using the `type` keyword now work correctly.
- **Fixed `match` for `Some`/`None` (v0.2.2)**: Corrected desugared match patterns and tag-based comparison.

### ADT Runtime Fixes (v0.2.2)
- **Tagged Struct Representation**: `Some(x)`, `Ok(x)`, etc., now use a proper tagged struct representation `{tag: i32, payload: i8*}`.
- **Heap Allocation**: ADT constructors now heap-allocate the tagged struct, fixing dangling pointer issues when returning ADT values.
- **Payload Type Detection**: Fixed `Result[T, E].1` returning `E` instead of `T` in `Err` match arms.
- **Polymorphic Sharing**: Added `freshen_type()` to ensure unique type variables for each constructor instance.

### CodeGen Fixes
- **Type Coercion Safety**: Fixed integer type mismatches in arithmetic (e.g., `i8 + i32` promotion in `hash = hash + s[i]`).
- **Chained Index Codegen**: Added `computeIndexPtr` helper for correct lvalue pointer calculation in nested indexing.
- **ADT Hint Propagation (v0.2.2)**: `pointerElementHints` are now correctly propagated through variables for ADT fields.

### Type Checker Fixes
- **Numeric Promotion (v0.2.2)**: `i32` literals now automatically promote to `i64`/`usize` in function arguments.
- **Nil Coercion (v0.2.2)**: `nil` can now be assigned to `FixedArray` fields in struct literals.
- **Return Type Error Location**: Points to the actual `return` expression instead of the function declaration.
- **Dereference Mutability (v0.2.2)**: Added checks that pointer targets are mutable when assigning through dereference.

## Error Reporting Improvements
- **Improved type error messages**: Changed to `"expected type 'X', but got 'Y'"` format.
- **Enhanced Suggestions**: Updated help text for type and syntax errors.
- **Better Context**: Added `find_return_node` helper to traverse function bodies for return statement context.
- **Dereference Errors (v0.2.2)**: Improved error messages for dereferencing non-pointer types.

## Standard Library & Tests
- **Standard Library**: Fixed compilation errors in `std/net.vix` and improved `HashMap` in `src/test.vix`.
- **Regression Tests**: Total now reaches **224 passing tests**, including 24 tests from v0.2.2 and 4 new tests for v0.2.5.

## Migration Guide
1. Replace `struct NAME {...}` with `type NAME = struct {...}`.
2. Replace `-> type` with `: type` for function return types.
3. Use `!expr` instead of `expr == false`.

