# Vix Language v0.4.2 Release Notes

## Release Date: June 18, 2026

## Overview

Vix 0.4.2 introduces the LLVM backend for vixc0 (the self-hosted compiler), along with several compiler improvements and bug fixes.

## New Features

### LLVM Backend for vixc0

- **LLVM C API Integration**: vixc0 now uses the LLVM C API for code generation instead of the QBE backend
- **Bootstrap C Helpers**: `bootstrap/src/helper.c` wraps LLVM C API functions that require array parameters, while `bootstrap/src/runtime.c` contains the runtime helpers linked into generated executables
- **Variable Tracking**: Added global variable tracking system for LLVM IR generation, allowing proper alloca/store/load operations

### Compiler Improvements

- **Void Extern Functions**: The parser now supports extern "C" functions without return types (void functions)
- **Array to Ptr Conversion**: Type checking now allows passing arrays to `ptr` parameters with implicit decay
- **`-l` Linker Flag**: Added support for `-l <lib>` flag to specify additional libraries to link
- **`global` Keyword Fix**: The `global` keyword can now be used as an identifier in extern declarations

### vixc0 Enhancements

- **File Input**: vixc0 can now read and compile `.vix` files directly (e.g., `./vixc0 test.vix`)
- **Improved Argument Parsing**: Better handling of command-line arguments with `.vix` file detection

## Bug Fixes

### Parser Fixes

- Fixed parsing of void extern functions that caused syntax errors
- Fixed `global` keyword not returning any token in the lexer

### Type System Fixes

- Fixed type checking to allow array-to-ptr implicit conversion in function arguments
- Fixed return value handling in LLVM backend (proper load before return)

### Code Generation Fixes

- Fixed LLVM function type creation to use proper parameter arrays
- Fixed alloca results not being tracked for subsequent store/load operations
- Fixed LLVM API function names for LLVM 22 compatibility (`LLVMBuildLoad2`, `LLVMBuildGEP2`, `LLVMBuildCall2`)

## Build System

- **Updated build.sh**: Now compiles the bootstrap helper/runtime objects and links the compiler with LLVM libraries
- **LLVM Linking**: Uses `-l LLVM-22` to link against LLVM 22

## Testing

All `.vix` files in the following directories have been tested:

- `examples/`: 50+ example files compile successfully
- `tests/regression/`: 170+ regression tests pass (some expected failures for ownership/error tests)
- `std/`: All standard library modules compile successfully

## Known Issues

- Some ownership tests correctly fail (expected behavior for error detection)
- Some import tests fail due to missing module files (not compiler bugs)
- stdin input mode still reads from hardcoded test file (existing issue)

## Contributors

- Vix Language Team

## Download

- Source: [GitHub Repository](https://github.com/anomalyco/vix-lang)
- Documentation: [Vix Language Docs](https://vix-lang.org)
