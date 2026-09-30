# Vix Language v0.2.8 Release Notes

**Release Date:** 2026-05-31

## Language And Compiler

### Match scrutinee now supports call-like expressions
- Fixed the parser/desugaring pipeline so `match` can accept call-like scrutinees in addition to existing identifier and literal forms.
- In practice this enables patterns such as `match choose(1) { ... }` and `match peek_char(src, pos) { ... }`.
- Non-trivial scrutinees are materialized into an internal temporary binding before desugaring to nested `if` chains, preserving existing `Option` and `Result` pattern semantics.
- Files: `src/parser/parser.y`

### Match arms now accept single-line bodies
- Fixed `match` parsing so existing project examples like `Some(v) -> print(v)` and `None -> print("none")` parse correctly without requiring `{ ... }` blocks.
- This restores compatibility with the established Option/Result/match style already used across `examples/` and `tests/regression/`.
- Files: `src/parser/parser.y`

### Compound assignment on general lvalues
- Extended compound assignment parsing from bare identifiers to general lvalues such as member access, indexing, and dereference-based lvalues.
- Fixed AST ownership so the desugared binary expression uses a cloned lvalue instead of reusing the assignment target node.
- Files: `src/parser/parser.y`

### Safer runtime string concatenation in codegen
- Hardened generated string concatenation so null pointer operands are converted to empty strings before any `strlen`/`strcpy`/`strcat` calls.
- This prevents `strlen(null)`-style crashes on defensive or partially-initialized paths.
- Files: `src/compiler/CodeGen.cpp`

### Type checking for string concatenation tightened
- Refined `+` typing so string concatenation is only selected when at least one operand is a string and the other operand is string-compatible.
- This avoids accidentally treating arbitrary pointer arithmetic as string concatenation.
- Files: `src/Typeck/Typeck.cpp`

### Struct field array mutation tracking improved
- Fixed array field mutation writeback so `s.field.push(x)` correctly updates the underlying struct field storage instead of only mutating a transient loaded pointer.
- This unblocks `vixc0` internals that store token streams and other compiler state inside structs with array fields.
- Files: `src/compiler/CodeGen.cpp`

## vixc0 Progress

### Bootstrap pipeline is now observable and structured
- `vixc0` now has a visible end-to-end prototype chain with:
  - lexer module
  - parser module
  - AST module
  - QBE text builder/codegen module
  - entrypoint driver
- Added debug entry modes to the `vixc0` prototype:
  - `--lex`
  - `--parser`
  - `--ast`
- Default mode still emits QBE text output.

### Lexer implementation now uses Vix Option and match
- Reworked `vixc0/lexer.vix` so the implementation itself uses Vix `Option` and `match` style helpers such as `peek_char(...)` and keyword classification by optional result.
- This makes the bootstrap compiler implementation closer to the intended Vix coding style rather than a purely C-like transliteration.

### Token stream representation refactored
- Replaced the earlier token-array shape with a `TokenStream` based on parallel arrays.
- This avoids fragile struct-array indexed field access paths while keeping the public lexer/debugging flow intact.

### Parser moved to token-driven recursive descent
- Replaced the earlier source-driven parser prototype with a token-driven recursive-descent parser while keeping the external `parse_program(src)` interface stable.
- This reduces the largest early bootstrap debt without forcing a later CLI redesign.

### Current vixc0 status
- `vixc0` remains an early bootstrap prototype and is not yet a full self-hosting frontend.
- The current codegen remains intentionally narrow and focused on the bootstrap path it currently covers.
- Files: `vixc0/main.vix`, `vixc0/lexer.vix`, `vixc0/parser.vix`, `vixc0/ast.vix`, `vixc0/codegen.vix`, `vixc0/ir-builder.vix`

## Examples

### Added match-on-call examples
- Added `examples/match_call_expr.vix` to demonstrate `match choose(1)`.
- Reworked `examples/match_peek_char.vix` as a self-contained demonstration of `match peek_char("abc", 1)` with both `Some` and `None` branches.

## Validation

- Rebuilt `vixc` successfully with parser/typecheck/codegen changes.
- Verified existing match/Option examples:
  - `examples/option.vix`
  - `examples/match_strings.vix`
- Verified new call-expression match examples:
  - `examples/match_call_expr.vix`
  - `examples/match_peek_char.vix`
- Ran Option/Result regression coverage including:
  - `tests/regression/test172.vix`
  - `tests/regression/test173.vix`
  - `tests/regression/test153.vix`
  - `tests/regression/test154.vix`
- Built and exercised `vixc0` debug modes:
  - `--lex`
  - `--parser`
  - `--ast`
  - default QBE output path
