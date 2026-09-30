# Declarative macros

Vix macros expand source fragments before parsing. A macro declaration starts
with `macro`, and the macro name and each invocation start with `$`.

```vix
macro $make_add(name: ident) {
    fn $name(a: i32, b: i32): i32 {
        return a + b
    }
}

macro $vec[values: expr*] {
    [$($values),*]
}
```

The declaration delimiter is part of the macro interface. A macro declared as
`$make_add(...)` must use parentheses, while `$vec[...]` must use brackets.

Parameters use `name: fragment` syntax. Supported fragment names are `ident`,
`expr`, `stmt`, `type`, `pat`, and `tt`. `ident` arguments are checked by the
macro expander; other fragments are checked when the expanded source is
parsed. Add `*` to the final parameter to accept zero or more comma-separated
arguments.

Use `$name` for a normal substitution. Repeat a variadic parameter with
`$($name),*`; the example-compatible spelling `$(name),*` is also accepted.
The text between `)` and `*` is the separator, so `$(+ values)*` emits a
leading `+` expression for each value without a separator.

Macros can call macros declared later in the same file and can be nested up to
64 expansion levels. Macro declarations are local to their source file;
imported modules expand their own declarations before being parsed.
