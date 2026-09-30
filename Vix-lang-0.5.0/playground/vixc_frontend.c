#include "../src/libvixc_frontend.h"
#include "parser.h"
#include "macro.h"
#include "semantic.h"
#include "typeck.h"
#include "ownership.h"
#include <stdlib.h>
#include <string.h>

extern FILE *yyin;
extern ASTNode *root;
extern int yyparse(void);
extern void load_source_file(const char *name);

static char error_buf[4096];

static void set_error(const char *msg) {
    strncpy(error_buf, msg, sizeof(error_buf) - 1);
    error_buf[sizeof(error_buf) - 1] = '\0';
}

CompileResult vixc_compile_string(const char *source) {
    CompileResult result = {NULL, 0};
    error_buf[0] = '\0';

    char tmp_path[L_tmpnam];
    if (tmpnam(tmp_path) == NULL) {
        result.error_count = 1;
        set_error("failed to create temp file name");
        return result;
    }

    char *expanded_source = vix_expand_macros(source, tmp_path);
    if (!expanded_source) {
        result.error_count = 1;
        set_error("macro expansion failed");
        return result;
    }

    FILE *src_file = fopen(tmp_path, "w");
    if (!src_file) {
        free(expanded_source);
        result.error_count = 1;
        set_error("failed to open temp file for writing");
        return result;
    }
    fwrite(expanded_source, 1, strlen(expanded_source), src_file);
    free(expanded_source);
    fclose(src_file);

    FILE *parse_file = fopen(tmp_path, "r");
    if (!parse_file) {
        result.error_count = 1;
        remove(tmp_path);
        set_error("failed to open temp file for reading");
        return result;
    }

    load_source_file(tmp_path);
    yyin = parse_file;

    if (yyparse() != 0 || !root) {
        result.error_count = 1;
        fclose(parse_file);
        remove(tmp_path);
        return result;
    }

    fclose(parse_file);
    remove(tmp_path);

    if (!inline_imports(root)) {
        result.error_count = 1;
        free_ast(root);
        root = NULL;
        return result;
    }

    if (check_undefined_symbols(root) > 0) {
        result.error_count = 1;
        free_ast(root);
        root = NULL;
        return result;
    }

    if (typecheck_program(root) != 0) {
        result.error_count = 1;
        free_ast(root);
        root = NULL;
        return result;
    }

    if (ownership_check_program(root) != 0) {
        result.error_count = 1;
        free_ast(root);
        root = NULL;
        return result;
    }

    result.root = root;
    return result;
}

void vixc_free_result(CompileResult *result) {
    if (result && result->root) {
        free_ast(result->root);
        result->root = NULL;
    }
}

const char *vixc_get_last_error(void) {
    return error_buf;
}
