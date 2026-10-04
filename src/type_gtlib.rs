// ============================================================
// 标准库（libGT.dll）：Python 风格函数名 → C 符号 + 签名
// ============================================================

use super::Ty;

/// 标准库函数签名：C 符号名、返回类型、参数类型
pub struct StdFn {
    pub symbol: &'static str,
    pub ret: Ty,
    pub params: &'static [Ty],
}

/// 查询标准库函数。名字对齐 Python（sqrt/pow/floor/...）。
///
/// 两个后端与 sema 共用此表：sema 做类型检查，codegen/jit 生成对 `py_*` 的调用。
pub fn gtlib_fn(name: &str) -> Option<StdFn> {
    let f = |symbol, ret, params: &'static [Ty]| StdFn { symbol, ret, params };
    Some(match name {
        // ---- math ----
        "sqrt" => f("py_sqrt", Ty::F64, &[Ty::F64]),
        "pow" => f("py_pow", Ty::F64, &[Ty::F64, Ty::F64]),
        "floor" => f("py_floor", Ty::F64, &[Ty::F64]),
        "ceil" => f("py_ceil", Ty::F64, &[Ty::F64]),
        "round" => f("py_round", Ty::I64, &[Ty::F64]),
        "sin" => f("py_sin", Ty::F64, &[Ty::F64]),
        "cos" => f("py_cos", Ty::F64, &[Ty::F64]),
        "tan" => f("py_tan", Ty::F64, &[Ty::F64]),
        "asin" => f("py_asin", Ty::F64, &[Ty::F64]),
        "acos" => f("py_acos", Ty::F64, &[Ty::F64]),
        "atan" => f("py_atan", Ty::F64, &[Ty::F64]),
        "atan2" => f("py_atan2", Ty::F64, &[Ty::F64, Ty::F64]),
        "exp" => f("py_exp", Ty::F64, &[Ty::F64]),
        "log" => f("py_log", Ty::F64, &[Ty::F64]),
        "log2" => f("py_log2", Ty::F64, &[Ty::F64]),
        "log10" => f("py_log10", Ty::F64, &[Ty::F64]),
        "fmod" => f("py_fmod", Ty::F64, &[Ty::F64, Ty::F64]),
        "hypot" => f("py_hypot", Ty::F64, &[Ty::F64, Ty::F64]),
        "cbrt" => f("py_cbrt", Ty::F64, &[Ty::F64]),
        "gcd" => f("py_gcd", Ty::I64, &[Ty::I64, Ty::I64]),
        "lcm" => f("py_lcm", Ty::I64, &[Ty::I64, Ty::I64]),
        "pi" => f("py_pi", Ty::F64, &[]),
        "e" => f("py_e", Ty::F64, &[]),
        "ipow" => f("py_ipow", Ty::I64, &[Ty::I64, Ty::I64]),
        "fabs" => f("py_fabs", Ty::F64, &[Ty::F64]),
        "isqrt" => f("py_isqrt", Ty::I64, &[Ty::I64]),
        "powmod" => f("py_powmod", Ty::I64, &[Ty::I64, Ty::I64, Ty::I64]),
        "factorial" => f("py_factorial", Ty::I64, &[Ty::I64]),
        "fib" => f("py_fib", Ty::I64, &[Ty::I64]),
        "isprime" => f("py_isprime", Ty::Bool, &[Ty::I64]),
        "comb" => f("py_comb", Ty::I64, &[Ty::I64, Ty::I64]),
        // ---- random ----
        "seed" => f("py_seed", Ty::Void, &[Ty::I64]),
        "random" => f("py_random", Ty::F64, &[]),
        "randint" => f("py_randint", Ty::I64, &[Ty::I64, Ty::I64]),
        "randrange" => f("py_randrange", Ty::I64, &[Ty::I64, Ty::I64]),
        "uniform" => f("py_uniform", Ty::F64, &[Ty::F64, Ty::F64]),
        "choice" => f("py_choice", Ty::Str, &[Ty::Str]),
        "shuffle" => f("py_shuffle", Ty::Str, &[Ty::Str]),
        "sample" => f("py_sample", Ty::Str, &[Ty::Str, Ty::I64]),
        "gauss" => f("py_gauss", Ty::F64, &[Ty::F64, Ty::F64]),
        // ---- string ----
        "isnumeric" | "isdigit" => f("py_isnumeric", Ty::Bool, &[Ty::Str]),
        "capitalize" => f("py_capitalize", Ty::Str, &[Ty::Str]),
        "reverse" => f("py_reverse", Ty::Str, &[Ty::Str]),
        "count" => f("py_count", Ty::I64, &[Ty::Str, Ty::Str]),
        "startswith" => f("py_startswith", Ty::Bool, &[Ty::Str, Ty::Str]),
        "endswith" => f("py_endswith", Ty::Bool, &[Ty::Str, Ty::Str]),
        "center" => f("py_center", Ty::Str, &[Ty::Str, Ty::I64]),
        "zfill" => f("py_zfill", Ty::Str, &[Ty::Str, Ty::I64]),
        "ljust" => f("py_ljust", Ty::Str, &[Ty::Str, Ty::I64]),
        "rjust" => f("py_rjust", Ty::Str, &[Ty::Str, Ty::I64]),
        "title" => f("py_title", Ty::Str, &[Ty::Str]),
        "swapcase" => f("py_swapcase", Ty::Str, &[Ty::Str]),
        "isalpha" => f("py_isalpha", Ty::Bool, &[Ty::Str]),
        "isspace" => f("py_isspace", Ty::Bool, &[Ty::Str]),
        "strip" => f("py_strip", Ty::Str, &[Ty::Str, Ty::Str]),
        "lstrip" => f("py_lstrip", Ty::Str, &[Ty::Str, Ty::Str]),
        "rstrip" => f("py_rstrip", Ty::Str, &[Ty::Str, Ty::Str]),
        "index" => f("py_index", Ty::I64, &[Ty::Str, Ty::Str]),
        "rindex" => f("py_rindex", Ty::I64, &[Ty::Str, Ty::Str]),
        "replace_all" => f("py_replace_all", Ty::Str, &[Ty::Str, Ty::Str, Ty::Str]),
        "sb_new" => f("gt_sb_new", Ty::I64, &[]),
        "sb_push" => f("gt_sb_push", Ty::Void, &[Ty::I64, Ty::Unknown]),
        "sb_finish" => f("gt_sb_finish", Ty::Str, &[Ty::I64]),
        "join_list" => f("py_join_list", Ty::Str, &[Ty::Str, Ty::Str]),
        "split_str" => f("py_split_str", Ty::Str, &[Ty::Str, Ty::Str]),
        "format" => f("py_format", Ty::Str, &[Ty::Str, Ty::Str]),
        "isalnum" => f("py_isalnum", Ty::Bool, &[Ty::Str]),
        "islower" => f("py_islower", Ty::Bool, &[Ty::Str]),
        "isupper" => f("py_isupper", Ty::Bool, &[Ty::Str]),
        "partition" => f("py_partition", Ty::Str, &[Ty::Str, Ty::Str]),
        "rpartition" => f("py_rpartition", Ty::Str, &[Ty::Str, Ty::Str]),
        "contains" => f("py_contains", Ty::Bool, &[Ty::Str, Ty::Str]),
        "is_ascii" => f("py_is_ascii", Ty::Bool, &[Ty::Str]),
        "utf8_len" => f("py_utf8_len", Ty::I64, &[Ty::Str]),
        // ---- net ----
        "tcp_connect" => f("py_tcp_connect", Ty::I64, &[Ty::Str, Ty::I64]),
        "tcp_listen" => f("py_tcp_listen", Ty::I64, &[Ty::I64]),
        "accept" => f("py_accept", Ty::I64, &[Ty::I64]),
        "net_send" => f("py_net_send", Ty::I64, &[Ty::I64, Ty::Str]),
        "net_recv" => f("py_net_recv", Ty::Str, &[Ty::I64, Ty::I64]),
        "recv_all" => f("py_recv_all", Ty::Str, &[Ty::I64]),
        "net_close" => f("py_net_close", Ty::Void, &[Ty::I64]),
        "close_listener" => f("py_close_listener", Ty::Void, &[Ty::I64]),
        "peer_addr" => f("py_peer_addr", Ty::Str, &[Ty::I64]),
        // ---- os（环境 / 进程）----
        "getcwd" => f("py_getcwd", Ty::Str, &[]),
        "getenv" => f("py_getenv", Ty::Str, &[Ty::Str]),
        "setenv" => f("py_setenv", Ty::Bool, &[Ty::Str, Ty::Str]),
        "system" => f("py_system", Ty::I64, &[Ty::Str]),
        "args" => f("py_args", Ty::Str, &[]),
        "exit" => f("py_exit", Ty::Void, &[Ty::I64]),
        // ---- file（路径 + 文件）----
        "path_exists" => f("py_path_exists", Ty::Bool, &[Ty::Str]),
        "file_exists" => f("py_file_exists", Ty::Bool, &[Ty::Str]),
        "is_file" => f("py_is_file", Ty::Bool, &[Ty::Str]),
        "is_dir" => f("py_is_dir", Ty::Bool, &[Ty::Str]),
        "getsize" => f("py_getsize", Ty::I64, &[Ty::Str]),
        "file_size" => f("py_file_size", Ty::I64, &[Ty::Str]),
        "listdir" => f("py_listdir", Ty::Str, &[Ty::Str]),
        "basename" => f("py_basename", Ty::Str, &[Ty::Str]),
        "dirname" => f("py_dirname", Ty::Str, &[Ty::Str]),
        "path_join" => f("py_path_join", Ty::Str, &[Ty::Str, Ty::Str]),
        "abspath" => f("py_abspath", Ty::Str, &[Ty::Str]),
        "mkdir" => f("py_mkdir", Ty::Bool, &[Ty::Str]),
        "rmdir" => f("py_rmdir", Ty::Bool, &[Ty::Str]),
        "os_remove" | "remove_file" => f("py_remove", Ty::Bool, &[Ty::Str]),
        "read_text" => f("py_read_text", Ty::Str, &[Ty::Str]),
        "write_text" => f("py_write_text", Ty::Bool, &[Ty::Str, Ty::Str]),
        "append_text" => f("py_append_text", Ty::Bool, &[Ty::Str, Ty::Str]),
        "read_lines" => f("py_read_lines", Ty::Str, &[Ty::Str]),
        "write_lines" => f("py_write_lines", Ty::Bool, &[Ty::Str, Ty::Str]),
        "file_copy" => f("py_file_copy", Ty::Bool, &[Ty::Str, Ty::Str]),
        "file_rename" => f("py_file_rename", Ty::Bool, &[Ty::Str, Ty::Str]),
        "touch" => f("py_touch", Ty::Bool, &[Ty::Str]),
        "glob" => f("py_glob", Ty::Str, &[Ty::Str, Ty::Str]),
        // ---- http ----
        "http_get" => f("py_http_get", Ty::Str, &[Ty::Str]),
        "http_post" => f("py_http_post", Ty::Str, &[Ty::Str, Ty::Str]),
        "http_put" => f("py_http_put", Ty::Str, &[Ty::Str, Ty::Str]),
        "http_delete" => f("py_http_delete", Ty::Str, &[Ty::Str]),
        "http_request" => f("py_http_request", Ty::Str, &[Ty::Str, Ty::Str, Ty::Str]),
        "http_download" => f("py_http_download", Ty::Bool, &[Ty::Str, Ty::Str]),
        "http_status" => f("py_http_status", Ty::I64, &[Ty::Str]),
        // ---- web ----
        "html_escape" => f("py_html_escape", Ty::Str, &[Ty::Str]),
        "url_encode" => f("py_url_encode", Ty::Str, &[Ty::Str]),
        "url_decode" => f("py_url_decode", Ty::Str, &[Ty::Str]),
        "parse_query" => f("py_parse_query", Ty::Str, &[Ty::Str]),
        "build_query" => f("py_build_query", Ty::Str, &[Ty::Str]),
        "html_page" => f("py_html_page", Ty::Str, &[Ty::Str, Ty::Str]),
        "route_match" => f("py_route_match", Ty::Str, &[Ty::Str, Ty::Str]),
        "query_get" => f("py_query_get", Ty::Str, &[Ty::Str, Ty::Str]),
        // ---- sql (SQLite) ----
        "sql_open" => f("py_sql_open", Ty::I64, &[Ty::Str]),
        "sql_close" => f("py_sql_close", Ty::Void, &[Ty::I64]),
        "sql_exec" => f("py_sql_exec", Ty::I64, &[Ty::I64, Ty::Str]),
        "sql_query" => f("py_sql_query", Ty::Str, &[Ty::I64, Ty::Str]),
        "sql_run" => f("py_sql_run", Ty::I64, &[Ty::I64, Ty::Str]),
        "sql_error" => f("py_sql_error", Ty::Str, &[Ty::I64]),
        "sql_begin" => f("py_sql_begin", Ty::I64, &[Ty::I64]),
        "sql_commit" => f("py_sql_commit", Ty::I64, &[Ty::I64]),
        "sql_rollback" => f("py_sql_rollback", Ty::I64, &[Ty::I64]),
        "sql_exec_many" => f("py_sql_exec_many", Ty::I64, &[Ty::I64, Ty::Str]),
        // ---- crypto ----
        "sha256" => f("py_sha256", Ty::Str, &[Ty::Str]),
        "hmac_sha256" => f("py_hmac_sha256", Ty::Str, &[Ty::Str, Ty::Str]),
        "sha256_hexlen" => f("py_sha256_hexlen", Ty::I64, &[]),
        "hex_encode" => f("py_hex_encode", Ty::Str, &[Ty::Str]),
        "hex_decode" => f("py_hex_decode", Ty::Str, &[Ty::Str]),
        "password_hash" => f("py_password_hash", Ty::Str, &[Ty::Str, Ty::Str]),
        "password_verify" => f("py_password_verify", Ty::Bool, &[Ty::Str, Ty::Str]),
        // ---- boot（裸机引导库；仅 --bare 目标有实现）----
        "boot_serial_init" => f("boot_serial_init", Ty::Void, &[]),
        "boot_serial_putc" => f("boot_serial_putc", Ty::Void, &[Ty::I64]),
        "boot_serial_puts" => f("boot_serial_puts", Ty::Void, &[Ty::Str]),
        "boot_serial_getc" => f("boot_serial_getc", Ty::I64, &[]),
        "boot_clear" => f("boot_clear", Ty::Void, &[]),
        "boot_putc_at" => f("boot_putc_at", Ty::Void, &[Ty::I64, Ty::I64, Ty::I64]),
        "boot_puts" => f("boot_puts", Ty::Void, &[Ty::Str]),
        "boot_getkey" => f("boot_getkey", Ty::I64, &[]),
        "boot_mem_alloc" => f("boot_mem_alloc", Ty::I64, &[Ty::I64]),
        "boot_mem_free" => f("boot_mem_free", Ty::Void, &[Ty::I64]),
        "boot_mem_size" => f("boot_mem_size", Ty::I64, &[]),
        "boot_time_ms" => f("boot_time_ms", Ty::I64, &[]),
        "boot_sleep_ms" => f("boot_sleep_ms", Ty::Void, &[Ty::I64]),
        "boot_hlt" => f("boot_hlt", Ty::Void, &[]),
        "boot_exit" => f("boot_exit", Ty::Void, &[]),
        "boot_reboot" => f("boot_reboot", Ty::Void, &[]),
        "boot_shutdown" => f("boot_shutdown", Ty::Void, &[]),
        "boot_disk_read" => f("boot_disk_read", Ty::I64, &[Ty::I64, Ty::I64, Ty::I64]),
        "boot_disk_write" => f("boot_disk_write", Ty::I64, &[Ty::I64, Ty::I64, Ty::I64]),
        "boot_inb" => f("boot_inb", Ty::I64, &[Ty::I64]),
        "boot_outb" => f("boot_outb", Ty::Void, &[Ty::I64, Ty::I64]),
        "boot_inw" => f("boot_inw", Ty::I64, &[Ty::I64]),
        "boot_outw" => f("boot_outw", Ty::Void, &[Ty::I64, Ty::I64]),
        "boot_version" => f("boot_version", Ty::Str, &[]),
        "boot_arch" => f("boot_arch", Ty::I64, &[]),
        // ---- core ----
        "core_free" => f("py_free", Ty::Void, &[Ty::Str]),
        "core_version" => f("py_core_version", Ty::Str, &[]),
        "core_echo" => f("py_core_echo", Ty::Str, &[Ty::Str]),
        // ---- entropy ----
        "sha512" => f("py_sha512", Ty::Str, &[Ty::Str]),
        "sha1" => f("py_sha1", Ty::Str, &[Ty::Str]),
        "md5" => f("py_md5", Ty::Str, &[Ty::Str]),
        "sha512_hexlen" => f("py_sha512_hexlen", Ty::I64, &[]),
        "entropy_random_hex" => f("py_entropy_random_hex", Ty::Str, &[Ty::I64]),
        "entropy_random_int" => f("py_entropy_random_int", Ty::I64, &[Ty::I64]),
        "entropy_random_bytes" => f("py_entropy_random_bytes", Ty::Str, &[Ty::I64]),
        "entropy_uuid" => f("py_entropy_uuid", Ty::Str, &[]),
        // ---- session ----
        "session_create" => f("py_session_create", Ty::Str, &[Ty::I64, Ty::Str, Ty::I64]),
        "session_get" => f("py_session_get", Ty::Str, &[Ty::I64, Ty::Str]),
        "session_destroy" => f("py_session_destroy", Ty::Void, &[Ty::I64, Ty::Str]),
        "session_gc" => f("py_session_gc", Ty::Void, &[Ty::I64]),
        "session_count" => f("py_session_count", Ty::I64, &[Ty::I64]),
        "serve" => f("py_serve", Ty::I64, &[Ty::I64, Ty::Str]),
        "serve_fn" => f("py_serve_fn", Ty::I64, &[Ty::I64, Ty::I64]),
        "match_route" => f("py_match_route", Ty::Str, &[Ty::Str, Ty::Str, Ty::Str]),
        // ---- ast（纯 C，gto_ 前缀）----
        "ast_lit_int" => f("gto_ast_lit_int", Ty::I64, &[Ty::I64]),
        "ast_lit_float" => f("gto_ast_lit_float", Ty::I64, &[Ty::F64]),
        "ast_lit_str" => f("gto_ast_lit_str", Ty::I64, &[Ty::Str]),
        "ast_lit_char" => f("gto_ast_lit_char", Ty::I64, &[Ty::I64]),
        "ast_lit_bool" => f("gto_ast_lit_bool", Ty::I64, &[Ty::I64]),
        "ast_id" => f("gto_ast_id", Ty::I64, &[Ty::Str]),
        "ast_binary" => f("gto_ast_binary", Ty::I64, &[Ty::I64, Ty::I64, Ty::I64]),
        "ast_unary" => f("gto_ast_unary", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_call" => f("gto_ast_call", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_add_arg" => f("gto_ast_add_arg", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_if" => f("gto_ast_if", Ty::I64, &[Ty::I64, Ty::I64, Ty::I64]),
        "ast_while" => f("gto_ast_while", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_block" => f("gto_ast_block", Ty::I64, &[Ty::I64]),
        "ast_add_stmt" => f("gto_ast_add_stmt", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_fn" => f("gto_ast_fn", Ty::I64, &[Ty::Str, Ty::I64]),
        "ast_add_param" => f("gto_ast_add_param", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_ret" => f("gto_ast_ret", Ty::I64, &[Ty::I64]),
        "ast_let" => f("gto_ast_let", Ty::I64, &[Ty::Str, Ty::I64, Ty::I64]),
        "ast_assign" => f("gto_ast_assign", Ty::I64, &[Ty::Str, Ty::I64]),
        "ast_assign_expr" => f("gto_ast_assign_expr", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_member" => f("gto_ast_member", Ty::I64, &[Ty::I64, Ty::Str]),
        "ast_index" => f("gto_ast_index", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_for" => f("gto_ast_for", Ty::I64, &[Ty::Str, Ty::I64, Ty::I64]),
        "ast_loop" => f("gto_ast_loop", Ty::I64, &[Ty::I64]),
        "ast_loopn" => f("gto_ast_loopn", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_break" => f("gto_ast_break_stmt", Ty::I64, &[]),
        "ast_continue" => f("gto_ast_continue_stmt", Ty::I64, &[]),
        "ast_import" => f("gto_ast_import", Ty::I64, &[Ty::Str]),
        "ast_cast" => f("gto_ast_cast", Ty::I64, &[Ty::I64, Ty::Str]),
        "ast_enum_val" => f("gto_ast_enum_val", Ty::I64, &[Ty::Str, Ty::I64]),
        "ast_slice" => f("gto_ast_slice", Ty::I64, &[Ty::I64, Ty::I64, Ty::I64]),
        "ast_tuple" => f("gto_ast_tuple", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_add_elem" => f("gto_ast_add_elem", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_defer" => f("gto_ast_defer", Ty::I64, &[Ty::I64]),
        "ast_asm" => f("gto_ast_asm", Ty::I64, &[Ty::Str]),
        "ast_none" => f("gto_ast_none", Ty::I64, &[]),
        "ast_some" => f("gto_ast_some", Ty::I64, &[Ty::I64]),
        "ast_match" => f("gto_ast_match", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_add_arm" => f("gto_ast_add_arm", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_arm" => f("gto_ast_arm", Ty::I64, &[Ty::I64, Ty::I64, Ty::I64]),
        "ast_set_line" => f("gto_ast_set_line", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_line" => f("gto_ast_line", Ty::I64, &[Ty::I64]),
        "ast_type" => f("gto_ast_type", Ty::I64, &[Ty::I64]),
        "ast_tag" => f("gto_ast_tag", Ty::I64, &[Ty::I64]),
        "ast_ival" => f("gto_ast_ival", Ty::I64, &[Ty::I64]),
        "ast_fval" => f("gto_ast_fval", Ty::F64, &[Ty::I64]),
        "ast_sval" => f("gto_ast_sval", Ty::Str, &[Ty::I64]),
        "ast_name" => f("gto_ast_name", Ty::Str, &[Ty::I64]),
        "ast_a" => f("gto_ast_a", Ty::I64, &[Ty::I64]),
        "ast_b" => f("gto_ast_b", Ty::I64, &[Ty::I64]),
        "ast_c" => f("gto_ast_c", Ty::I64, &[Ty::I64]),
        "ast_d" => f("gto_ast_d", Ty::I64, &[Ty::I64]),
        "ast_nkids" => f("gto_ast_nkids", Ty::I64, &[Ty::I64]),
        "ast_kid" => f("gto_ast_kid", Ty::I64, &[Ty::I64, Ty::I64]),
        "ast_type_name" => f("gto_ast_type_name", Ty::Str, &[Ty::I64]),
        "ast_free" => f("gto_ast_free", Ty::Void, &[Ty::I64]),
        "ast_dump" => f("gto_ast_dump", Ty::Str, &[Ty::I64]),
        "ast_str_free" => f("gto_ast_str_free", Ty::Void, &[Ty::Str]),
        "ast_walk" => f("gto_ast_walk", Ty::I64, &[Ty::I64]),
        "ast_walk_next" => f("gto_ast_walk_next", Ty::I64, &[Ty::I64]),
        "ast_walk_free" => f("gto_ast_walk_free", Ty::Void, &[Ty::I64]),
        // ---- json ----
        "json_dumps" => f("py_json_dumps", Ty::Str, &[Ty::Str]),
        "json_loads" => f("py_json_loads", Ty::Str, &[Ty::Str]),
        "json_dump" => f("py_json_dump", Ty::Bool, &[Ty::Str, Ty::Str]),
        "json_load" => f("py_json_load", Ty::Str, &[Ty::Str]),
        "json_pretty" => f("py_json_pretty", Ty::Str, &[Ty::Str]),
        "json_minify" => f("py_json_minify", Ty::Str, &[Ty::Str]),
        "json_valid" => f("py_json_valid", Ty::Bool, &[Ty::Str]),
        "json_escape" => f("py_json_escape", Ty::Str, &[Ty::Str]),
        "json_number" => f("py_json_number", Ty::Str, &[Ty::I64]),
        "json_number_f" => f("py_json_number_f", Ty::Str, &[Ty::F64]),
        "json_bool" => f("py_json_bool", Ty::Str, &[Ty::I64]),
        "json_null" => f("py_json_null", Ty::Str, &[]),
        "json_array" => f("py_json_array", Ty::Str, &[Ty::Str]),
        "json_object" => f("py_json_object", Ty::Str, &[Ty::Str]),
        "json_unquote" => f("py_json_unquote", Ty::Str, &[Ty::Str]),
        // ---- toml ----
        "toml_loads" => f("py_toml_loads", Ty::Str, &[Ty::Str]),
        "toml_load" => f("py_toml_load", Ty::Str, &[Ty::Str]),
        "toml_dumps" => f("py_toml_dumps", Ty::Str, &[Ty::Str]),
        _ => return None,
    })
}

/// 该名字是否留作内置函数（用户不可重定义）
pub fn is_builtin_name(name: &str) -> bool {
    matches!(
        name,
        // 输出 / 长度 / 转换
        "put" | "print" | "len" | "str" | "string" | "int" | "i64" | "f64" | "float" | "bool" | "read_line" | "readline" | "input" | "read_int" | "readint"
        // 容器构造
        | "list" | "List" | "set" | "Set" | "map" | "Map" | "dict" | "range" | "assert" | "sleep" | "chan" | "chan_send" | "chan_recv"
        // list 操作
        | "push" | "append" | "pop" | "at" | "insert" | "has" | "contains"
        | "keys" | "values" | "remove"
        // 数值
        | "abs" | "min" | "max" | "sum"
        // 字符串
        | "substr" | "split" | "join" | "find" | "upper" | "lower" | "trim"
        | "repeat" | "replace" | "pad_left" | "pad_right" | "lpad" | "rpad" | "fmt_int"
        // 裸内存
        | "mem_alloc" | "mem_free" | "mem_store_i64" | "mem_load_i64"
        | "mem_store_u8" | "mem_load_u8" | "mem_copy" | "mem_set"
        // 标准库（os / json / toml）
        | "getcwd" | "getenv" | "setenv" | "path_exists" | "mkdir" | "system"
        | "json_dumps" | "json_loads" | "json_dump" | "json_load" | "json_pretty" | "json_minify" | "json_valid" | "json_escape" | "json_number" | "json_number_f" | "json_bool" | "json_null" | "json_array" | "json_object" | "json_unquote" | "toml_loads" | "toml_load" | "toml_dumps"
        | "listdir" | "rmdir" | "basename" | "dirname" | "path_join" | "abspath" | "is_file" | "is_dir" | "getsize"
        | "read_text" | "write_text" | "append_text" | "read_lines" | "write_lines" | "file_exists" | "file_copy" | "file_size" | "file_rename" | "touch" | "args" | "exit" | "glob"
        | "strip" | "lstrip" | "rstrip" | "index" | "rindex" | "replace_all" | "join_list" | "split_str" | "format" | "isalnum" | "islower" | "isupper" | "partition" | "rpartition" | "is_ascii" | "utf8_len"
        | "tcp_connect" | "tcp_listen" | "accept" | "net_send" | "net_recv" | "recv_all" | "net_close" | "close_listener" | "peer_addr"
        | "sql_open" | "sql_close" | "sql_exec" | "sql_query" | "sql_run" | "sql_error" | "sql_begin" | "sql_commit" | "sql_rollback" | "sql_exec_many"
        | "core_free" | "core_version" | "core_echo"
        | "boot_serial_init" | "boot_serial_putc" | "boot_serial_puts" | "boot_serial_getc"
        | "boot_clear" | "boot_putc_at" | "boot_puts" | "boot_getkey"
        | "boot_mem_alloc" | "boot_mem_free" | "boot_mem_size"
        | "boot_time_ms" | "boot_sleep_ms"
        | "boot_hlt" | "boot_exit" | "boot_reboot" | "boot_shutdown"
        | "boot_disk_read" | "boot_disk_write"
        | "boot_inb" | "boot_outb" | "boot_inw" | "boot_outw"
        | "boot_version" | "boot_arch"
        | "sha256" | "hmac_sha256" | "sha256_hexlen" | "sha512" | "sha1" | "md5" | "sha512_hexlen" | "hex_encode" | "hex_decode" | "password_hash" | "password_verify"
        | "entropy_random_hex" | "entropy_random_int" | "entropy_random_bytes" | "entropy_uuid"
        | "session_create" | "session_get" | "session_destroy" | "session_gc" | "session_count"
        | "seed" | "random" | "randint" | "randrange" | "uniform" | "choice" | "shuffle" | "sample" | "gauss"
        | "ast_lit_int" | "ast_lit_float" | "ast_lit_str" | "ast_lit_char" | "ast_lit_bool"
        | "ast_id" | "ast_binary" | "ast_unary" | "ast_call" | "ast_add_arg"
        | "ast_if" | "ast_while" | "ast_block" | "ast_add_stmt" | "ast_fn" | "ast_add_param"
        | "ast_ret" | "ast_let" | "ast_assign" | "ast_assign_expr" | "ast_member" | "ast_index"
        | "ast_for" | "ast_loop" | "ast_loopn" | "ast_break" | "ast_continue"
        | "ast_import" | "ast_cast" | "ast_enum_val" | "ast_slice" | "ast_tuple" | "ast_add_elem"
        | "ast_defer" | "ast_asm" | "ast_none" | "ast_some" | "ast_match" | "ast_add_arm" | "ast_arm"
        | "ast_set_line" | "ast_line" | "ast_type" | "ast_tag" | "ast_ival" | "ast_fval"
        | "ast_sval" | "ast_name" | "ast_a" | "ast_b" | "ast_c" | "ast_d" | "ast_nkids" | "ast_kid"
        | "ast_type_name" | "ast_free" | "ast_dump" | "ast_str_free" | "ast_walk" | "ast_walk_next" | "ast_walk_free"
        | "http_get" | "http_post" | "http_put" | "http_delete" | "http_request" | "http_download" | "http_status"
        | "html_escape" | "url_encode" | "url_decode" | "parse_query" | "build_query" | "html_page" | "route_match" | "query_get" | "serve" | "serve_fn" | "match_route"
    ) || gtlib_fn(name).is_some()
}


#[path = "type_gtlib_tests.rs"]
#[cfg(test)]
mod type_gtlib_tests;
