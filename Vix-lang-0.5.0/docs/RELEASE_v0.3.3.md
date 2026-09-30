# Vix Compiler v0.3.3 Release Notes

**Release Date:** June 13, 2026

## Overview

Vix v0.3.3 focuses on match expression enhancements, making `match` more powerful and ergonomic. Key improvements include using `match` as an expression, supporting member access as scrutinee, and adding multi-pattern arms with `|`.

## What's New

### Match Expression Enhancements

#### Match as Expression

`match` can now be used as an expression that returns a value:

```vix
let result = match tag {
    "int" -> 1
    "bool" -> 2
    _ -> 0
}
```

This enables more concise code when assigning values based on pattern matching.

#### Member Access as Scrutinee

Match expressions now support member access expressions as the scrutinee:

```vix
match expr.tag {
    "int" -> "integer"
    "string" -> "text"
    _ -> "other"
}
```

Previously, only simple identifiers and literals were supported as the scrutinee.

#### Multi-Pattern Arms

Match expressions now support multiple patterns separated by `|` (pipe) for the same arm:

```vix
match tag {
    "int" | "bool" | "string" -> "Literal"
    "ident" -> "Identifier"
    "binop" -> "BinaryExpression"
    _ -> "Unknown"
}
```

This desugars to OR conditions: `(tag == "int") or (tag == "bool") or (tag == "string")`.

### Bug Fixes

#### Member Access Scrutinee

Fixed a bug where `match` with member access expressions (e.g., `match expr.tag { ... }`) would fail at runtime. The `clone_match_scrutinee` function now correctly handles `AST_MEMBER_ACCESS` nodes.

## Code Changes

### Modified Files
- `src/parser/parser.y`: Added match expression, member access scrutinee, and multi-pattern arm support
- `include/ast.h`: Added `is_multi_pattern` field to assign node for multi-pattern match arms

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

## Upgrading

This release is backward compatible. No code changes are required to upgrade from v0.3.2.

## Contributors

- Vix Language Team

## Feedback

Please report any issues or feature requests on our [GitHub Issues](https://github.com/vixlang/Vix-lang/issues) page.

---

For more information about the Vix programming language, visit [vixlang.github.io](https://vixlang.github.io).
