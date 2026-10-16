//! 内置函数 / 标准库函数名清单。
//!
//! 供语义分析在"未定义函数"时报拼写建议（closest match）。
//! 与具体后端无关（原先放在 jit::symbol，JIT 移除后独立出来）。

/// 语言内置函数（无需 import）。
pub(crate) const BUILTIN_NAMES: &[&str] = &[
    "put", "print", "len", "str", "string", "int", "i64", "f64", "float", "bool",
    "read_line", "readline", "input", "read_int", "readint",
    "list", "set", "map", "dict", "range", "assert", "sleep", "chan", "chan_send", "chan_recv",
    "push", "append", "pop", "at", "insert", "has", "contains", "keys", "values", "remove",
    "abs", "min", "max", "sum",
    "substr", "split", "join", "find", "upper", "lower", "trim",
    "repeat", "replace", "pad_left", "pad_right", "lpad", "rpad", "fmt_int",
    "mem_alloc", "mem_free", "mem_store_i64", "mem_load_i64", "mem_store_u8", "mem_load_u8", "mem_copy", "mem_set",
];

/// 标准库函数（需 import 对应模块；此处仅用于拼写建议）。
pub(crate) const GTLIB_NAMES: &[&str] = &[
    // os
    "getcwd", "getenv", "setenv", "system", "args", "exit",
    // file
    "path_exists", "file_exists", "is_file", "is_dir", "getsize", "file_size",
    "listdir", "basename", "dirname", "path_join", "abspath", "mkdir", "rmdir", "os_remove", "remove_file",
    "read_text", "write_text", "append_text", "read_lines", "write_lines", "file_copy", "file_rename", "touch", "glob",
    // random
    "seed", "random", "randint", "randrange", "uniform", "choice", "shuffle", "sample", "gauss",
    // ast (pure C)
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
    // http
    "http_get", "http_post", "http_put", "http_delete", "http_request", "http_download", "http_status",
    // web
    "html_escape", "url_encode", "url_decode", "parse_query", "build_query", "html_page", "route_match", "query_get", "serve", "serve_fn", "match_route",
    // json
    "json_dump", "json_load", "json_pretty", "json_minify", "json_valid",
    "json_escape", "json_number", "json_number_f", "json_bool", "json_null", "json_array", "json_object", "json_unquote",
    "json_dumps", "json_loads",
    // toml
    "toml_loads", "toml_load", "toml_dumps",
    // math
    "sqrt", "pow", "floor", "ceil", "round", "sin", "cos", "tan", "asin", "acos", "atan",
    "atan2", "exp", "log", "log2", "log10", "fmod", "hypot", "cbrt", "gcd", "lcm", "pi", "e",
    "ipow", "isnumeric", "isdigit", "capitalize", "reverse", "count", "startswith", "endswith",
    "center", "fabs", "isqrt", "powmod", "factorial", "fib", "isprime", "comb",
    "zfill", "ljust", "rjust", "title", "swapcase", "isalpha", "isspace",
    "strip", "lstrip", "rstrip", "index", "rindex", "replace_all", "join_list", "split_str", "format", "isalnum", "islower", "isupper", "partition", "rpartition", "contains", "is_ascii", "utf8_len",
    // net
    "tcp_connect", "tcp_listen", "accept", "net_send", "net_recv", "recv_all", "net_close", "close_listener", "peer_addr",
    // sql
    "sql_open", "sql_close", "sql_exec", "sql_query", "sql_run", "sql_error",
    "sql_begin", "sql_commit", "sql_rollback", "sql_exec_many",
    // core
    "core_free", "core_version", "core_echo",
    // crypto
    "sha256", "hmac_sha256", "sha256_hexlen", "sha512", "sha1", "md5", "sha512_hexlen", "hex_encode", "hex_decode", "password_hash", "password_verify",
    // entropy
    "entropy_random_hex", "entropy_random_int", "entropy_random_bytes", "entropy_uuid",
    // session
    "session_create", "session_get", "session_destroy", "session_gc", "session_count",
];
