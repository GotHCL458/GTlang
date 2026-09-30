# Vix v0.2.0 Release Notes

**Breaking Changes** — This is a major language redesign release.

## Syntax Changes

### 1. Removed `global` and `const` Keywords

Module-level `let` declarations now automatically have **persistent storage duration** (like globals). No special keyword is needed.

**Before (v0.1.x):**
```vix
global counter: i32 = 100
const MAX_SIZE = 1024
let const N = 10
```

**After (v0.2.0):**
```vix
let counter: i32 = 100
let MAX_SIZE = 1024
```

- Variables declared at **module level** with `let` are automatically global and persistent.
- Variables declared at **function level** with `let` are stack-local (as before).
- Use `let mut` for mutable variables at any scope.

### 2. Removed Auto-Generated `main()` Function

The compiler no longer auto-generates a `main()` function. You **must** define `fn main()` explicitly.

**Before (v0.1.x):**
```vix
print("hello")
```

**After (v0.2.0):**
```vix
fn main(): i32
{
    print("hello")
    return 0
}
```

If no `fn main()` is defined and `#[no_main]` is not set, the compiler will emit an error.

### 3. Variables Are Immutable by Default

All `let` declarations are now **immutable by default**. Use `let mut` to declare mutable variables.

```vix
let x = 10        // immutable
let mut y = 20    // mutable
y = 30            // OK
```

### 4. Function-Level Static Variables

For rare cases requiring static storage duration inside functions, use `let static mut`:

```vix
fn get_id(): i32
{
    let static mut id = 0
    id += 1
    return id
}
```

## Bug Fixes

### Windows Linker

- Fixed MinGW linker failing to find `-lmoldname`, `-lpthread`, `-ladvapi32`, `-lshell32`, `-luser32` when the bundled `libc/` directory is incomplete.
- Added automatic probing of Windows SDK and MSVC library paths.
- Libraries are now only linked if they are actually found on the system, preventing linker errors from missing optional libraries.

## Migration Guide

1. Replace all `global` declarations with `let`.
2. Replace all `const` and `let const` declarations with `let`.
3. Replace all `pub global` declarations with `let` (add `pub` support for `let` in future releases).
4. Add `fn main(): i32 { ... return 0 }` to any file that previously relied on top-level code execution.
5. Add `mut` to any variable that needs to be reassigned.

## Test Results

- 732/732 pytest tests passing
- 220/220 compilation tests passing
- 209/209 run tests passing
