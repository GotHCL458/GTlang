# Vix Language v0.2.10 Release Notes

**Release Date:** 2026-06-06

## New Features

### Multi-object file linking support
- The compiler now supports linking multiple object files directly without recompilation.
- Usage: `vixc file1.o file2.o file3.o -o output`
- This enables separate compilation workflows where individual modules are compiled to `.o` files first, then linked together.
- Added new `vix_link_multi()` API in the linker module to handle multiple input files.
- Files: `src/main.c`, `src/compiler/Linker/Linker.h`, `src/compiler/Linker/Linker.cpp`

## Bug Fixes

### Enum type definition now accepts leading `|`
- The parser now accepts an optional leading `|` before the first variant in type alias enum definitions.
- This allows writing enums in a more consistent style:
  ```vix
  type T =
      | Int
      | Float
      | String
      | Bool
      | Unit
  ```
- Previously, the first variant had to omit the `|`, which was inconsistent with subsequent variants.
- File: `src/parser/parser.y`

### Linker: removed hardcoded CRT object paths
- Fixed the ELF linker path discovery to check file existence before adding CRT objects (`crt1.o`, `crti.o`, `crtbegin.o`, `crtend.o`, `crtn.o`) to the linker arguments.
- Eliminated a redundant second call to `probeSysPaths` during ELF linking; the sysroot is now probed once and reused.
- This fixes link failures on systems where the CRT objects are located in non-standard directories or where some objects are absent.
- File: `src/compiler/Linker/Linker.cpp`

### Vararg parameter type checking
- Added type checking for variadic function arguments (e.g., `printf`).
- Previously, arguments passed to vararg parameters were not type-checked, allowing complex ADT types like `Result` or `Option` to be passed directly, which caused silent runtime failures.
- Now only primitive types (numeric, bool, string, ptr) are allowed as vararg arguments.
- Passing ADT/struct/tuple/array types to vararg parameters now produces a clear type error.
- File: `src/Typeck/Typeck.cpp`

## Validation

- Rebuilt `vixc` successfully.
- Verified enum definitions with leading `|` on the first variant now parse correctly.
- Tested multi-object linking with `vixc a.o b.o -o prog`.
- Verified that passing `Result` type to `printf` now produces a type error.
- Verified correct usage with `match` unpacking compiles and runs correctly.
