# Vix Compiler v0.3.3-dev Release Notes

**Release Date:** June 13, 2026

## Overview

Vix v0.3.3-dev is a development release focused on syntax modernization, ADT bug fixes, boolean type support, and compiler correctness fixes. Key changes include replacing the `ref` keyword with `&` for safe references, adding proper `true`/`false` boolean literals, fixing custom ADT constructors and match expressions, and expanding the vixc0 bootstrap compiler.

## What's New

### Syntax Change: `ref` replaced by `&`

The `ref` keyword has been replaced by the `&` symbol for safe references. This applies to both type annotations and expressions:

- **Type position**: `ref i32` is now `&i32`
- **Expression position**: `ref x` is now `&x`
- **Mutable reference**: `mut ref x` is now `mut &x`

Example:

```vix
fn add(a: &i32, b: &i32): i32 {
    return @a + @b
}

fn main(): i32 {
    let x = 7
    let y = 8
    print(add(&x, &y))
    return 0
}
```

### Proper Boolean Type

`true` and `false` are now proper boolean literals with type `bool`, instead of being aliases for `1` and `0` as `i32`:

- `true` has type `bool` (previously `i32`)
- `false` has type `bool` (previously `i32`)
- Comparison operators (`==`, `!=`, `<`, `<=`, `>`, `>=`) return `bool` values
- Boolean operators (`and`, `or`, `!`) work with `bool` type

### Custom ADT Constructor Fixes

Fixed several bugs with custom Algebraic Data Type (ADT) constructors:

- **Payload type resolution**: ADT constructors with payloads (e.g., `Int(i32)`) now correctly resolve the payload type from the type definition, instead of using a fresh type variable.
- **Tag value consistency**: Custom ADT constructors now use the constructor index as the tag value (consistent with the parser), instead of a hash of the constructor name.
- **Match with custom ADTs**: Match expressions with custom ADT constructor patterns (both simple identifiers like `Red` and payload patterns like `Int(v)`) now correctly compare tag values.
- **Function-local ADTs**: ADT definitions inside functions now generate correct tag values, matching the behavior of top-level ADT definitions.
- **No-payload constructors**: Simple enum constructors without payloads (e.g., `Red`, `Green`, `Blue`) now create proper tagged structs instead of returning raw integer values.

### vixc0 Bootstrap Compiler

The vixc0 bootstrap compiler has been expanded with:
- Multi-token lexer support for `&`, `true`, `false`, escape sequences
- Expression parser with operator precedence (arithmetic, comparison, logical)
- `let` statements, `print` statements, assignment statements
- QBE IR codegen for expressions and basic statements

### Bug Fixes

#### Ownership Checker
- **Copy Classification Fixes**: `string` and `struct` values are no longer incorrectly treated as Copy by the Owner pass.
  - Assigning a `string` now moves ownership instead of silently allowing reuse of the source variable.
  - Assigning a `struct` that contains non-Copy data now also moves ownership as expected.
- **Removed Copy Naming Hack**: Ownership no longer treats identifiers starting with an uppercase letter as implicitly Copy.
  - This closes a hole where move checks could be bypassed by variable naming alone.
- **By-Value Call Semantics**: Function calls now distinguish between by-value and `&` arguments.
  - Non-`&` parameters consume non-Copy arguments.
  - `&` parameters are checked as borrows instead of generic reads.
- **Persistent Borrow Tracking**: Borrows stored in variables now remain active across statements until the borrow variable leaves scope or is reassigned.
  - This prevents mutating a value while an outstanding borrow to it still exists.
- **Improved Borrow Source Propagation**: Ownership now preserves borrow provenance through more expression forms.
  - Member access and indexing now retain the underlying borrowed base.
  - Borrow provenance is also propagated through simple expression and call-return paths needed for dangling-reference detection.
- **Indirect Dangling Reference Detection**: Returning a local reference through an intermediate function call is now rejected.

Example now correctly rejected:

```vix
fn take(xs: [i32]): i32 {
    return xs[0]
}

fn main(): i32 {
    let xs = [1, 2, 3]
    print(take(xs))
    print(xs[0])
    return 0
}
```

Diagnostic:

```text
error [SemanticError]: use of moved value 'xs'
```

Borrow stored across statements is now also rejected:

```vix
fn main(): i32 {
    let mut x = 1
    let r = mut &x
    x = 2
    print(@r)
    return 0
}
```

Diagnostic:

```text
error [SemanticError]: cannot assign to 'x' while it is borrowed
```

#### Lexer & String Processing
- **Octal Escape Sequences**: Added support for octal escape sequences (e.g., `\033`) in string literals. This enables proper handling of ANSI escape codes for terminal control.
  - Example: `printf("\033[2J\033[H")` now correctly outputs screen clearing sequences
  - Previously, `\033` was incorrectly parsed as `\0` followed by literal characters

#### Type Checker
- **Void Function Returns**: Fixed type checking for void functions that don't have explicit return statements
  - Functions declared with `void` return type no longer require explicit `return` statements
  - Improved type inference for function bodies

### Code Changes

#### Modified Files
- `src/parser/lexer.l`: Removed `ref` keyword token (replaced by `&` symbol)
- `src/parser/parser.y`: Removed `REF_KW` grammar rules, unified reference syntax under `AMPERSAND`
- `src/Ownership/Ownership.cpp`: Corrected move/copy classification, call-site ownership handling, persistent borrow tracking, and dangling-reference propagation
- `src/Typeck/Typeck.cpp`: Improved void function type checking
- `src/compiler/Stmts.cpp`: Minor statement handling improvements
- `src/compiler/Structs.cpp`: Struct processing enhancements
- `examples/*.vix`: Updated all examples from `ref` to `&` syntax
- `tests/test_ownership.py`: Updated test source strings for `&` syntax
- `tests/test_types.py`: Updated test source strings for `&` syntax
- `tests/feat.py`: Updated test source strings for `&` syntax
- `tests/regression/ownership_return_via_call.vix`: Updated to `&` syntax
- `tests/regression/ownership_persistent_borrow.vix`: Updated to `&` syntax

#### New Files
- `src/ed.vix`: Text editor implementation in Vix language
- `src/test_cursor2.vix`: Test program for cursor movement functionality
- `tests/regression/ownership_string_move.vix`: Regression fixture for string move errors
- `tests/regression/ownership_struct_move.vix`: Regression fixture for struct move errors
- `tests/regression/ownership_call_move.vix`: Regression fixture for by-value call move semantics
- `tests/regression/ownership_persistent_borrow.vix`: Regression fixture for borrow-across-statement assignment rejection
- `tests/regression/ownership_return_via_call.vix`: Regression fixture for indirect dangling-reference rejection

## Technical Details

### Octal Escape Sequence Support

The lexer now properly handles octal escape sequences in the format `\NNN` where N is an octal digit (0-7):

```vix
printf("\033[2J\033[H")
printf("\033[10;5H")  // Move to row 10, column 5
printf("\033[31m")    // Set red text color
```

### Arrow Key Handling

The editor now processes ANSI arrow key sequences:
- Up Arrow: `\033[A`
- Down Arrow: `\033[B`
- Right Arrow: `\033[C`
- Left Arrow: `\033[D`

### Ownership Validation

Ownership-focused validation in this release covered:
- Use-after-move for arrays, strings, and structs.
- By-value function call move semantics.
- Shared and mutable borrow conflict checks.
- Borrow persistence across statements when a reference is stored in a variable.
- Indirect dangling local reference detection through function returns.

Targeted validation performed:
- Rebuilt `vixc` successfully with the updated `src/Ownership/Ownership.cpp`.
- Ran `venv/bin/pytest tests/test_ownership.py` successfully.
- Verified dedicated regression fixtures fail with the expected ownership diagnostics.

## Installation

### From Source

```bash
git clone https://github.com/vixlang/Vix-lang.git
cd Vix-lang
mkdir build && cd build
cmake ..
make
sudo make install
```

### Using Pre-built Binaries

Download the appropriate binary for your platform from the [releases page](https://github.com/vixlang/Vix-lang/releases).

## Known Issues

- Some warnings are generated during compilation of example files (unused variables)
- The editor (ed.vix) is a basic implementation and may not handle all edge cases
- The Owner system still does not implement full non-lexical lifetimes, control-flow-sensitive move joins, or field-level partial move analysis

## Upgrading

This release introduces a syntax change: the `ref` keyword is replaced by `&` for safe references. All code using `ref` must be updated:

- `ref T` (type) → `&T`
- `ref x` (expression) → `&x`
- `mut ref x` → `mut &x`

This is also intended to be source-compatible for valid programs (with the syntax update), but some code that previously compiled due to ownership-checker gaps will now be rejected.

In particular, v0.3.1 now diagnoses:
- Reuse of moved `string` and `struct` values.
- Reuse of non-Copy values after passing them to by-value function parameters.
- Mutation while a borrow stored in a variable is still live.
- Returning references to locals through simple intermediate call paths.

## Contributors

- Vix Language Team

## Feedback

Please report any issues or feature requests on our [GitHub Issues](https://github.com/vixlang/Vix-lang/issues) page.

---

For more information about the Vix programming language, visit [vixlang.github.io](https://vixlang.github.io).
