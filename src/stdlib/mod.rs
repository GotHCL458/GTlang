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
        funcs: &["getcwd", "getenv", "setenv", "system", "args", "exit"],
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
        dll: "ast",
        funcs: &[
            "ast_lit_int", "ast_lit_float", "ast_lit_str", "ast_lit_char", "ast_lit_bool",
            "ast_id", "ast_binary", "ast_unary", "ast_call", "ast_add_arg",
            "ast_if", "ast_while", "ast_block", "ast_add_stmt", "ast_fn", "ast_add_param",
            "ast_ret", "ast_let", "ast_assign", "ast_assign_expr", "ast_member", "ast_index",
            "ast_for", "ast_loop", "ast_loopn", "ast_break", "ast_continue",
            "ast_import", "ast_cast", "ast_enum_val", "ast_slice", "ast_tuple", "ast_add_elem",
            "ast_defer", "ast_asm", "ast_none", "ast_some", "ast_match", "ast_add_arm", "ast_arm",
            "ast_set_line", "ast_line", "ast_type", "ast_tag", "ast_ival", "ast_fval",
            "ast_sval", "ast_name", "ast_a", "ast_b", "ast_c", "ast_d", "ast_nkids", "ast_kid",
            "ast_type_name", "ast_free", "ast_dump", "ast_str_free", "ast_walk", "ast_walk_next", "ast_walk_free",
        ],
    },
    StdModule {
        dll: "http",
        funcs: &[
            "http_get", "http_post", "http_put", "http_delete", "http_request",
            "http_download", "http_status",
        ],
    },
    StdModule {
        dll: "web",
        funcs: &[
            "html_escape", "url_encode", "url_decode", "parse_query", "build_query",
            "html_page", "route_match", "query_get", "serve", "match_route",
        ],
    },
    StdModule {
        dll: "random",
        funcs: &[
            "seed", "random", "randint", "randrange", "uniform",
            "choice", "shuffle", "sample", "gauss",
        ],
    },
    StdModule {
        dll: "file",
        funcs: &[
            "path_exists", "file_exists", "is_file", "is_dir", "getsize", "file_size",
            "listdir", "basename", "dirname", "path_join", "abspath", "mkdir", "rmdir",
            "os_remove", "remove_file", "read_text", "write_text", "append_text",
            "read_lines", "write_lines", "file_copy", "file_rename", "touch",
        ],
    },
];

pub fn dll_of(func: &str) -> Option<&'static str> {
    for m in MODULES {
        if m.funcs.contains(&func) { return Some(m.dll); }
    }
    None
}
