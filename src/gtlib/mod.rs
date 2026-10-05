//! GT 标准库元信息。

pub mod boot;

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
            "strip", "lstrip", "rstrip", "index", "rindex", "replace_all", "join_list", "split_str", "format", "isalnum", "islower", "isupper", "partition", "rpartition", "contains", "is_ascii", "utf8_len",
        ],
    },
    StdModule {
        dll: "net",
        funcs: &[
            "tcp_connect", "tcp_listen", "accept", "net_send", "net_recv",
            "recv_all", "net_close", "close_listener", "peer_addr",
        ],
    },
    StdModule {
        dll: "os",
        funcs: &["getcwd", "getenv", "setenv", "system", "args", "exit"],
    },
    StdModule {
        dll: "json",
        funcs: &[
            "json_dumps", "json_loads", "json_dump", "json_load", "json_pretty",
            "json_minify", "json_valid", "json_escape", "json_number", "json_number_f",
            "json_bool", "json_null", "json_array", "json_object", "json_unquote",
        ],
    },
    StdModule {
        dll: "toml",
        funcs: &["toml_loads", "toml_load", "toml_dumps"],
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
            "html_page", "route_match", "query_get", "serve", "serve_fn", "match_route",
        ],
    },
    StdModule {
        dll: "sql",
        funcs: &[
            "sql_open", "sql_close", "sql_exec", "sql_query", "sql_run", "sql_error",
            "sql_begin", "sql_commit", "sql_rollback", "sql_exec_many",
        ],
    },
    StdModule {
        dll: "core",
        funcs: &["core_free", "core_version", "core_echo"],
    },
    StdModule {
        dll: "crypto",
        funcs: &[
            "sha256", "hmac_sha256", "sha256_hexlen", "sha512", "sha1", "md5", "sha512_hexlen",
            "hex_encode", "hex_decode",
            "password_hash", "password_verify",
        ],
    },
    StdModule {
        dll: "entropy",
        funcs: &[
            "entropy_random_hex", "entropy_random_int", "entropy_random_bytes", "entropy_uuid",
        ],
    },
    StdModule {
        dll: "session",
        funcs: &[
            "session_create", "session_get", "session_destroy", "session_gc", "session_count",
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
        dll: "boot",
        funcs: &[
            // 串口
            "boot_serial_init", "boot_serial_putc", "boot_serial_puts", "boot_serial_getc", "boot_serial_poll",
            // 屏幕
            "boot_clear", "boot_putc_at", "boot_puts",
            "boot_vga_clear", "boot_vga_set_color", "boot_vga_putc", "boot_vga_puts",
            // 键盘
            "boot_getkey", "boot_keyboard_modifiers",
            // 内存
            "boot_mem_alloc", "boot_mem_free", "boot_mem_size", "boot_mem_free_bytes", "boot_paging_init",
            // 时间
            "boot_time_ms", "boot_sleep_ms",
            "boot_rtc_read",
            // 系统
            "boot_hlt", "boot_exit", "boot_reboot", "boot_shutdown",
            "boot_cpuid", "boot_cpu_vendor",
            // 磁盘
            "boot_disk_read", "boot_disk_write",
            "boot_disk_partitions",
            // 端口
            "boot_inb", "boot_outb", "boot_inw", "boot_outw",
            // 信息
            "boot_version", "boot_arch",
            // 中断
            "boot_idt_init", "boot_irq_enable", "boot_irq_disable", "boot_pic_init", "boot_keyboard_handler", "boot_irq_register",
            // 文件系统：内存盘
            "boot_fs_ram_create", "boot_fs_ram_write", "boot_fs_ram_read", "boot_fs_ram_size", "boot_fs_ram_delete", "boot_fs_ram_count", "boot_fs_ram_list",
            // FAT8（只读）
            "boot_fat8_find", "boot_fat8_read", "boot_fat8_list",
            // FAT16（完整读写）
            "boot_fat16_find", "boot_fat16_read", "boot_fat16_write", "boot_fat16_delete", "boot_fat16_list",
            // FAT32（完整读写）
            "boot_fat32_find", "boot_fat32_read", "boot_fat32_write", "boot_fat32_delete", "boot_fat32_list",
            // 多任务
            "boot_task_create", "boot_task_yield", "boot_task_start", "boot_task_exit",
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

#[cfg(test)]
#[path = "../gtlib_tests/mod_tests.rs"]
mod mod_tests;
