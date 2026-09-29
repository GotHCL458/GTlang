//! GT 标准库元信息。

/// 一个标准库模块。
pub struct StdModule {
    pub dll: &'static str,
    pub funcs: &'static [&'static str],
}

pub const MODULES: &[StdModule] = &[
    StdModule {
        dll: "math",
        funcs: &[
            "sqrt", "pow", "floor", "ceil", "round", "sin", "cos", "tan",
            "asin", "acos", "atan", "atan2", "exp", "log", "log2", "log10",
            "fmod", "hypot", "cbrt", "gcd", "lcm", "pi", "e", "ipow",
            "fabs", "isqrt", "powmod", "factorial", "fib", "isprime", "comb",
        ],
    },
    StdModule {
        dll: "string",
        funcs: &[
            "isnumeric", "isdigit", "capitalize", "reverse", "count",
            "startswith", "endswith", "center", "zfill", "ljust", "rjust",
            "title", "swapcase", "isalpha", "isspace",
        ],
    },
    StdModule {
        dll: "os",
        funcs: &[
            "getcwd", "getenv", "setenv", "path_exists", "remove", "mkdir", "system",
        ],
    },
    StdModule {
        dll: "json",
        funcs: &["dumps", "loads"],
    },
    StdModule {
        dll: "toml",
        funcs: &["loads", "load"],
    },
    StdModule {
        dll: "file",
        funcs: &[
            "read_text", "write_text", "append_text", "read_lines",
            "file_exists", "file_copy", "file_size",
        ],
    },
];

pub fn dll_of(func: &str) -> Option<&'static str> {
    for m in MODULES {
        if m.funcs.contains(&func) { return Some(m.dll); }
    }
    None
}
