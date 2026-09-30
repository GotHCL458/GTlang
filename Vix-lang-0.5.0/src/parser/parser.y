%{
#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include "../include/ast.h"
#include "../include/compiler.h"
extern int yylex();
extern int yyparse();
extern FILE* yyin;
extern int yylineno;
extern char* yytext;
extern const char* current_input_filename; 
void yyerror(const char* s);
ASTNode* root;

static ASTNode* create_default_value_for_type(ASTNode* type_node, YYLTYPE* loc);
static ASTNode* build_type_alias_enum(const char* type_name, ASTNode* variants);
static ASTNode* mark_type_alias_public(ASTNode* program);
static ASTNode* clone_match_scrutinee(ASTNode* scrutinee);
static ASTNode* clone_lvalue(ASTNode* node);
static ASTNode* materialize_match_scrutinee(ASTNode* scrutinee, ASTNode** out_ref);
static ASTNode* build_match_desugared(ASTNode* scrutinee, ASTNode* arms);
static ASTNode* build_array_pattern_arm(ASTNode* scrutinee, ASTNode* pattern,
                                        ASTNode* body, ASTNode* fallback);
static ASTNode* build_pipe_desugared(ASTNode* value, ASTNode* callee, YYLTYPE* loc);
static ASTNode* build_string_slice_desugared(ASTNode* target, ASTNode* start, ASTNode* end, YYLTYPE* loc);

typedef struct {
    char* name;
    int payload_count;
    ASTNode* payload_type_node;
} AdtCtorEntry;

typedef struct {
    char* name;
    int generic_arity;
    int ctor_count;
    AdtCtorEntry* ctors;
} AdtDefEntry;

static AdtDefEntry g_adt_defs[128];
static int g_adt_def_count = 0;

/* Temporary storage for payload type nodes during enum_variant parsing */
static ASTNode* g_adt_payload_types[256];
static int g_adt_payload_type_count = 0;

/* Impl method registry: maps (type_name, method_name) -> mangled_function_name */
typedef struct {
    char* type_name;
    char* method_name;
    char* func_name;
} ImplMethodEntry;

static ImplMethodEntry g_impl_methods[512];
static int g_impl_method_count = 0;

/* ── Tiny inline hash table: name → index lookup ────────────────── */
#define ADT_HT_SIZE 256
static int g_adt_ht[ADT_HT_SIZE]; /* maps name hash → def index, -1 = empty */

static unsigned int str_hash(const char *s) {
    unsigned int h = 5381;
    while (*s) h = ((h << 5) + h) + (unsigned char)*s++;
    return h;
}
static void adt_ht_insert(const char *name, int idx) {
    unsigned int h = str_hash(name) & (ADT_HT_SIZE - 1);
    for (int i = 0; i < ADT_HT_SIZE; i++) {
        if (g_adt_ht[h] == -1) { g_adt_ht[h] = idx; return; }
        h = (h + 1) & (ADT_HT_SIZE - 1);
    }
    fprintf(stderr, "internal error: ADT hash table full (%d entries)\n", ADT_HT_SIZE);
}
static int adt_ht_lookup(const char *name) {
    if (!name) return -1;
    unsigned int h = str_hash(name) & (ADT_HT_SIZE - 1);
    for (int i = 0; i < ADT_HT_SIZE; i++) {
        int idx = g_adt_ht[h];
        if (idx == -1) break; /* empty slot → not found */
        if (g_adt_defs[idx].name && strcmp(g_adt_defs[idx].name, name) == 0)
            return idx;
        h = (h + 1) & (ADT_HT_SIZE - 1);
    }
    return -1;
}
#define IM_HT_SIZE 1024
static int g_im_ht[IM_HT_SIZE]; /* maps hash to impl_method index, -1 = empty */
static void im_ht_insert(const char *type_name, const char *method_name, int idx) {
    char buf[512]; snprintf(buf, sizeof(buf), "%s::%s", type_name, method_name);
    unsigned int h = str_hash(buf) & (IM_HT_SIZE - 1);
    for (int i = 0; i < IM_HT_SIZE; i++) {
        if (g_im_ht[h] == -1) { g_im_ht[h] = idx; return; }
        h = (h + 1) & (IM_HT_SIZE - 1);
    }
    fprintf(stderr, "internal error: Impl method hash table full (%d entries)\n", IM_HT_SIZE);
}
static int im_ht_lookup(const char *type_name, const char *method_name) {
    if (!type_name || !method_name) return -1;
    char buf[512]; snprintf(buf, sizeof(buf), "%s::%s", type_name, method_name);
    unsigned int h = str_hash(buf) & (IM_HT_SIZE - 1);
    for (int i = 0; i < IM_HT_SIZE; i++) {
        int idx = g_im_ht[h];
        if (idx == -1) break;
        if (g_impl_methods[idx].type_name && g_impl_methods[idx].method_name &&
            strcmp(g_impl_methods[idx].type_name, type_name) == 0 &&
            strcmp(g_impl_methods[idx].method_name, method_name) == 0)
            return idx;
        h = (h + 1) & (IM_HT_SIZE - 1);
    }
    return -1;
}

static void register_impl_method(const char* type_name, const char* method_name, const char* func_name) {
    if (!type_name || !method_name || !func_name) return;
    /* Check for duplicate method within the same type */
    for (int i = 0; i < g_impl_method_count; i++) {
        if (g_impl_methods[i].type_name && g_impl_methods[i].method_name &&
            strcmp(g_impl_methods[i].type_name, type_name) == 0 &&
            strcmp(g_impl_methods[i].method_name, method_name) == 0) {
            char msg[512];
            snprintf(msg, sizeof(msg), "duplicate method '%s' in impl block for type '%s'", method_name, type_name);
            report_simple_error(ERROR_LEVEL_ERROR, ERROR_SEMANTIC, msg);
            return;
        }
    }
    if (g_impl_method_count >= (int)(sizeof(g_impl_methods) / sizeof(g_impl_methods[0]))) return;
    g_impl_methods[g_impl_method_count].type_name = strdup(type_name);
    g_impl_methods[g_impl_method_count].method_name = strdup(method_name);
    g_impl_methods[g_impl_method_count].func_name = strdup(func_name);
    im_ht_insert(type_name, method_name, g_impl_method_count);
    g_impl_method_count++;
}

const char* vix_lookup_impl_method(const char* type_name, const char* method_name) {
    if (!type_name || !method_name) return NULL;
    /* Normalize type_name to lowercase for case-insensitive matching */
    char normalized[256];
    size_t len = strlen(type_name);
    if (len >= sizeof(normalized)) len = sizeof(normalized) - 1;
    for (size_t i = 0; i < len; i++) {
        normalized[i] = tolower((unsigned char)type_name[i]);
    }
    normalized[len] = '\0';
    int idx = im_ht_lookup(normalized, method_name);
    if (idx >= 0) return g_impl_methods[idx].func_name;
    return NULL;
}

static int is_builtin_union_ctor_name(const char* name) {
    if (!name) return 0;
    return strcmp(name, "Some") == 0 || strcmp(name, "None") == 0 ||
           strcmp(name, "Ok") == 0 || strcmp(name, "Err") == 0;
}

/**
 * @brief Reset all parser global state — must be called before each compilation
 *        to prevent state leakage across runs (e.g. when the compiler is used
 *        as a library via VIXC_FRONTEND_ONLY).
 */
void vix_reset_parser_state(void) {
    for (int i = 0; i < g_adt_def_count; i++) {
        free(g_adt_defs[i].name);
        g_adt_defs[i].name = NULL;
        if (g_adt_defs[i].ctors) {
            for (int j = 0; j < g_adt_defs[i].ctor_count; j++) {
                free(g_adt_defs[i].ctors[j].name);
                g_adt_defs[i].ctors[j].name = NULL;
                /* payload_type_node is owned by AST, not freed here */
            }
            free(g_adt_defs[i].ctors);
            g_adt_defs[i].ctors = NULL;
        }
    }
    g_adt_def_count = 0;
    g_adt_payload_type_count = 0;
    for (int i = 0; i < g_impl_method_count; i++) {
        free(g_impl_methods[i].type_name);
        g_impl_methods[i].type_name = NULL;
        free(g_impl_methods[i].method_name);
        g_impl_methods[i].method_name = NULL;
        free(g_impl_methods[i].func_name);
        g_impl_methods[i].func_name = NULL;
    }
    g_impl_method_count = 0;
    /* Reset hash tables: explicit -1 initialization (portable, no memset trick) */
    for (int i = 0; i < ADT_HT_SIZE; i++) g_adt_ht[i] = -1;
    for (int i = 0; i < IM_HT_SIZE; i++) g_im_ht[i] = -1;
}

static int find_adt_def_index(const char* name) {
    if (!name) return -1;
    int idx = adt_ht_lookup(name);
    if (idx >= 0) return idx;
    // Fallback: linear scan (for entries added before hash table was populated)
    for (int i = 0; i < g_adt_def_count; i++) {
        if (g_adt_defs[i].name && strcmp(g_adt_defs[i].name, name) == 0)
            return i;
    }
    return -1;
}

static int find_adt_ctor_index(const char* ctor_name, int* out_def_index) {
    if (!ctor_name) return -1;
    for (int i = 0; i < g_adt_def_count; i++) {
        for (int j = 0; j < g_adt_defs[i].ctor_count; j++) {
            if (g_adt_defs[i].ctors[j].name && strcmp(g_adt_defs[i].ctors[j].name, ctor_name) == 0) {
                if (out_def_index) *out_def_index = i;
                return j;
            }
        }
    }
    return -1;
}

static void register_adt_definition(const char* name, int generic_arity, ASTNode* variants) {
    if (!name) return;
    int idx = find_adt_def_index(name);
    if (idx < 0) {
        if (g_adt_def_count >= (int)(sizeof(g_adt_defs) / sizeof(g_adt_defs[0]))) { g_adt_payload_type_count = 0; return; }
        idx = g_adt_def_count++;
        adt_ht_insert(name, idx);
        g_adt_defs[idx].name = strdup(name);
        g_adt_defs[idx].ctor_count = 0;
        g_adt_defs[idx].ctors = NULL;
    } else {
        free(g_adt_defs[idx].ctors);
        g_adt_defs[idx].ctors = NULL;
        g_adt_defs[idx].ctor_count = 0;
    }
    g_adt_defs[idx].generic_arity = generic_arity;
    if (!variants || variants->type != AST_EXPRESSION_LIST) { g_adt_payload_type_count = 0; return; }

    int count = variants->data.expression_list.expression_count;
    if (count <= 0) { g_adt_payload_type_count = 0; return; }
    g_adt_defs[idx].ctors = (AdtCtorEntry*)calloc((size_t)count, sizeof(AdtCtorEntry));
    if (!g_adt_defs[idx].ctors) { g_adt_payload_type_count = 0; return; }
    g_adt_defs[idx].ctor_count = count;
    for (int i = 0; i < count; i++) {
        ASTNode* variant = variants->data.expression_list.expressions[i];
        if (!variant || variant->type != AST_IDENTIFIER || !variant->data.identifier.name) continue;
        g_adt_defs[idx].ctors[i].name = strdup(variant->data.identifier.name);
        g_adt_defs[idx].ctors[i].payload_count = (variant->mutability == (MutabilityType)1) ? 1 : 0;
        if (i < g_adt_payload_type_count) {
            g_adt_defs[idx].ctors[i].payload_type_node = g_adt_payload_types[i];
        } else {
            g_adt_defs[idx].ctors[i].payload_type_node = NULL;
        }
    }
    g_adt_payload_type_count = 0;
}

int vix_is_adt_definition(const char* name) {
    return find_adt_def_index(name) >= 0;
}

int vix_adt_generic_arity(const char* name) {
    int idx = find_adt_def_index(name);
    return idx >= 0 ? g_adt_defs[idx].generic_arity : -1;
}

int vix_adt_ctor_payload_count(const char* ctor_name) {
    int def_index = -1;
    int ctor_index = find_adt_ctor_index(ctor_name, &def_index);
    if (ctor_index < 0 || def_index < 0) return -1;
    return g_adt_defs[def_index].ctors[ctor_index].payload_count;
}

ASTNode* vix_adt_ctor_payload_type_node(const char* ctor_name) {
    int def_index = -1;
    int ctor_index = find_adt_ctor_index(ctor_name, &def_index);
    if (ctor_index < 0 || def_index < 0) return NULL;
    return g_adt_defs[def_index].ctors[ctor_index].payload_type_node;
}

int vix_adt_ctor_index(const char* ctor_name) {
    int def_index = -1;
    return find_adt_ctor_index(ctor_name, &def_index);
}

const char* vix_adt_ctor_base_name(const char* ctor_name) {
    int def_index = -1;
    int ctor_index = find_adt_ctor_index(ctor_name, &def_index);
    if (ctor_index < 0 || def_index < 0) return NULL;
    return g_adt_defs[def_index].name;
}

ASTNode* vix_adt_payload_type_for_base(const char* base_name) {
    if (!base_name) return NULL;
    int def_idx = find_adt_def_index(base_name);
    if (def_idx < 0) return NULL;
    for (int i = 0; i < g_adt_defs[def_idx].ctor_count; i++) {
        if (g_adt_defs[def_idx].ctors[i].payload_count > 0 &&
            g_adt_defs[def_idx].ctors[i].payload_type_node) {
            return g_adt_defs[def_idx].ctors[i].payload_type_node;
        }
    }
    return NULL;
}

static ASTNode* prepend_binding_to_match_body(ASTNode* body, const char* bind_name, ASTNode* scrutinee) {
    if (!body || !bind_name || !scrutinee) return body;

    ASTNode* bind_left = create_identifier_node(bind_name);
    ASTNode* bind_right = clone_match_scrutinee(scrutinee);
    if (!bind_left || !bind_right) return body;

    ASTNode* bind_decl = create_assign_node(bind_left, bind_right);
    if (bind_decl) bind_decl->data.assign.is_declaration = 1;

    ASTNode* wrapped = create_program_node();
    add_statement_to_program(wrapped, bind_decl);

    if (body->type == AST_PROGRAM) {
        for (int i = 0; i < body->data.program.statement_count; i++) {
            add_statement_to_program(wrapped, body->data.program.statements[i]);
        }
    } else {
        add_statement_to_program(wrapped, body);
    }

    return wrapped;
}

static ASTNode* prepend_bindings_to_match_body(ASTNode* body, ASTNode* bindings) {
    if (!body || !bindings || bindings->type != AST_PROGRAM ||
        bindings->data.program.statement_count == 0) {
        return body;
    }

    ASTNode* wrapped = create_program_node();
    for (int i = 0; i < bindings->data.program.statement_count; i++) {
        add_statement_to_program(wrapped, bindings->data.program.statements[i]);
    }
    if (body->type == AST_PROGRAM) {
        for (int i = 0; i < body->data.program.statement_count; i++) {
            add_statement_to_program(wrapped, body->data.program.statements[i]);
        }
    } else {
        add_statement_to_program(wrapped, body);
    }
    return wrapped;
}

typedef enum {
    GENERIC_KIND_FUNCTION = 0,
    GENERIC_KIND_STRUCT = 1,
    GENERIC_KIND_TYPE = 2
} GenericKind;

typedef struct {
    char* name;
    GenericKind kind;
    int arity;
} GenericArityEntry;

static GenericArityEntry g_generic_arities[512];
static int g_generic_arity_count = 0;

static int node_list_count(ASTNode* list) {
    if (!list || list->type != AST_EXPRESSION_LIST) return 0;
    return list->data.expression_list.expression_count;
}

static int find_generic_arity_index(const char* name, GenericKind kind) {
    if (!name) return -1;
    for (int i = 0; i < g_generic_arity_count; i++) {
        if (g_generic_arities[i].kind == kind &&
            g_generic_arities[i].name &&
            strcmp(g_generic_arities[i].name, name) == 0) {
            return i;
        }
    }
    return -1;
}

static void register_generic_arity(const char* name, GenericKind kind, int arity) {
    if (!name) return;
    int idx = find_generic_arity_index(name, kind);
    if (idx >= 0) {
        g_generic_arities[idx].arity = arity;
        return;
    }
    if (g_generic_arity_count >= (int)(sizeof(g_generic_arities) / sizeof(g_generic_arities[0]))) {
        return;
    }
    g_generic_arities[g_generic_arity_count].name = strdup(name);
    g_generic_arities[g_generic_arity_count].kind = kind;
    g_generic_arities[g_generic_arity_count].arity = arity;
    g_generic_arity_count++;
}

static int lookup_generic_arity(const char* name, GenericKind kind, int* found) {
    int idx = find_generic_arity_index(name, kind);
    if (idx < 0) {
        if (found) *found = 0;
        return 0;
    }
    if (found) *found = 1;
    return g_generic_arities[idx].arity;
}

static void check_generic_arity_usage(const char* name, GenericKind kind, ASTNode* type_args, YYLTYPE* loc) {
    int found = 0;
    int expected = lookup_generic_arity(name, kind, &found);
    if (!found) return;

    int actual = node_list_count(type_args);
    if (expected == actual) return;

    char msg[256];
    const char* what = (kind == GENERIC_KIND_FUNCTION) ? "function" :
                       (kind == GENERIC_KIND_STRUCT) ? "struct" : "type";
    snprintf(msg, sizeof(msg),
             "Generic %s '%s' expects %d type argument(s), but got %d",
             what, name ? name : "<unknown>", expected, actual);

    int line = (loc && loc->first_line > 0) ? loc->first_line : yylineno;
    int col = (loc && loc->first_column > 0) ? loc->first_column : 1;
    set_location_with_column(current_input_filename ? current_input_filename : "unknown", line, col);
    report_simple_error(ERROR_LEVEL_ERROR, ERROR_SEMANTIC, msg);
}

static ASTNode* create_default_scalar_value(ASTNode* type_node, YYLTYPE* loc) {
    if (!type_node) {
        return create_num_int_node_with_yyltype(0, (void*)loc);
    }

    switch (type_node->type) {
        case AST_TYPE_INT8:
            return create_char_node_with_yyltype(0, (void*)loc);
        case AST_TYPE_INT32:
        case AST_TYPE_INT64:
            return create_num_int_node_with_yyltype(0, (void*)loc);
        case AST_TYPE_FLOAT32:
        case AST_TYPE_FLOAT64:
            return create_num_float_node_with_yyltype(0.0, (void*)loc);
        case AST_TYPE_STRING:
        case AST_TYPE_POINTER:
            return create_nil_node_with_yyltype((void*)loc);
        case AST_IDENTIFIER:
            if (type_node->data.identifier.name) {
                if (strcmp(type_node->data.identifier.name, "char") == 0 ||
                    strcmp(type_node->data.identifier.name, "i8") == 0 ||
                    strcmp(type_node->data.identifier.name, "u8") == 0) {
                    return create_char_node_with_yyltype(0, (void*)loc);
                }
                if (strcmp(type_node->data.identifier.name, "f32") == 0 ||
                    strcmp(type_node->data.identifier.name, "f64") == 0) {
                    return create_num_float_node_with_yyltype(0.0, (void*)loc);
                }
                if (strcmp(type_node->data.identifier.name, "str") == 0 ||
                    strcmp(type_node->data.identifier.name, "ptr") == 0) {
                    return create_nil_node_with_yyltype((void*)loc);
                }
            }
            return create_num_int_node_with_yyltype(0, (void*)loc);
        case AST_TYPE_FIXED_SIZE_LIST:
        case AST_TYPE_LIST:
            return create_default_value_for_type(type_node, loc);
        default:
            return create_num_int_node_with_yyltype(0, (void*)loc);
    }
}

static ASTNode* create_default_value_for_type(ASTNode* type_node, YYLTYPE* loc) {
    if (!type_node) {
        return create_num_int_node_with_yyltype(0, (void*)loc);
    }

    if (type_node->type == AST_TYPE_FIXED_SIZE_LIST) {
        ASTNode* list = create_expression_list_node_with_yyltype((void*)loc);
        long long size = type_node->data.fixed_size_list_type.size;
        ASTNode* elem_type = type_node->data.fixed_size_list_type.element_type;
        if (size < 0) size = 0;
        for (long long i = 0; i < size; i++) {
            add_expression_to_list(list, create_default_scalar_value(elem_type, loc));
        }
        return list;
    }

    if (type_node->type == AST_TYPE_LIST) {
        return create_expression_list_node_with_yyltype((void*)loc);
    }

    return create_default_scalar_value(type_node, loc);
}

static ASTNode* build_type_alias_enum(const char* type_name, ASTNode* variants) {
    (void)type_name;
    ASTNode* program = create_program_node();
    if (!variants || variants->type != AST_EXPRESSION_LIST) {
        return program;
    }

    for (int i = 0; i < variants->data.expression_list.expression_count; i++) {
        ASTNode* variant = variants->data.expression_list.expressions[i];
        if (!variant || variant->type != AST_IDENTIFIER || !variant->data.identifier.name) continue;
        ASTNode* left = create_identifier_node(variant->data.identifier.name);
        ASTNode* right = create_num_int_node(i);
        ASTNode* decl = create_const_node(left, right);
        add_statement_to_program(program, decl);
    }
    return program;
}

static ASTNode* prepend_expression(ASTNode* list, ASTNode* expr, YYLTYPE* loc) {
    if (list && list->type == AST_EXPRESSION_LIST) {
        int count = list->data.expression_list.expression_count;
        ASTNode** resized = (ASTNode**)realloc(
            list->data.expression_list.expressions,
            sizeof(ASTNode*) * (size_t)(count + 1));
        if (!resized) return list;
        memmove(resized + 1, resized, sizeof(ASTNode*) * (size_t)count);
        resized[0] = expr;
        list->data.expression_list.expressions = resized;
        list->data.expression_list.expression_count = count + 1;
        return list;
    }
    ASTNode* result = create_expression_list_node_with_yyltype((void*)loc);
    add_expression_to_list(result, expr);
    return result;
}

static ASTNode* build_pipe_desugared(ASTNode* value, ASTNode* callee, YYLTYPE* loc) {
    if (!value || !callee) return callee;
    if (callee->type == AST_CALL) {
        callee->data.call.args = prepend_expression(callee->data.call.args, value, loc);
        return callee;
    }
    return create_call_node_with_yyltype(callee, prepend_expression(NULL, value, loc), (void*)loc);
}

static ASTNode* build_string_slice_desugared(ASTNode* target, ASTNode* start, ASTNode* end, YYLTYPE* loc) {
    ASTNode* args = create_expression_list_node_with_yyltype((void*)loc);
    add_expression_to_list(args, target);
    add_expression_to_list(args, start);
    add_expression_to_list(args, end);
    ASTNode* func = create_identifier_node_with_yyltype(VIX_STRING_SLICE_INTRINSIC, (void*)loc);
    return create_call_node_with_yyltype(func, args, (void*)loc);
}

static ASTNode* mark_type_alias_public(ASTNode* program) {
    if (!program || program->type != AST_PROGRAM) return program;
    for (int i = 0; i < program->data.program.statement_count; i++) {
        ASTNode* stmt = program->data.program.statements[i];
        if (stmt && stmt->type == AST_CONST) {
            stmt->data.assign.is_public = 1;
        }
    }
    return program;
}

static ASTNode* clone_match_scrutinee(ASTNode* scrutinee) {
    if (!scrutinee) return NULL;
    switch (scrutinee->type) {
        case AST_IDENTIFIER:
            if (scrutinee->data.identifier.name) {
                return create_identifier_node(scrutinee->data.identifier.name);
            }
            return NULL;
        case AST_MEMBER_ACCESS: {
            ASTNode* object = clone_match_scrutinee(scrutinee->data.member_access.object);
            ASTNode* field = clone_match_scrutinee(scrutinee->data.member_access.field);
            if (!object || !field) return NULL;
            return create_member_access_node(object, field);
        }
        case AST_NUM_INT:
            return create_num_int_node(scrutinee->data.num_int.value);
        case AST_NUM_FLOAT:
            return create_num_float_node(scrutinee->data.num_float.value);
        case AST_CHAR:
            return create_char_node(scrutinee->data.character.value);
        case AST_STRING:
            if (scrutinee->data.string.value) {
                return create_string_node(scrutinee->data.string.value);
            }
            return NULL;
        case AST_NIL:
            return create_nil_node();
        default:
            return NULL;
    }
}

static ASTNode* clone_lvalue(ASTNode* node) {
    if (!node) return NULL;
    switch (node->type) {
        case AST_IDENTIFIER:
            if (node->data.identifier.name) {
                return create_identifier_node_with_location(node->data.identifier.name, node->location);
            }
            return NULL;
        case AST_MEMBER_ACCESS: {
            ASTNode* object = clone_lvalue(node->data.member_access.object);
            ASTNode* field = clone_lvalue(node->data.member_access.field);
            if (!object || !field) return NULL;
            return create_member_access_node_with_location(object, field, node->location);
        }
        case AST_INDEX: {
            ASTNode* target = clone_lvalue(node->data.index.target);
            ASTNode* index = clone_match_scrutinee(node->data.index.index);
            if (!target || !index) return NULL;
            return create_index_node_with_location(target, index, node->location);
        }
        case AST_UNARYOP:
            if (node->data.unaryop.op == OP_DEREF) {
                ASTNode* expr = clone_lvalue(node->data.unaryop.expr);
                if (!expr) return NULL;
                return create_unaryop_node_with_location(OP_DEREF, expr, node->location);
            }
            return NULL;
        default:
            return clone_match_scrutinee(node);
    }
}

static ASTNode* materialize_match_scrutinee(ASTNode* scrutinee, ASTNode** out_ref) {
    if (out_ref) *out_ref = NULL;
    if (!scrutinee) return NULL;

    ASTNode* cloned = clone_match_scrutinee(scrutinee);
    if (cloned) {
        if (out_ref) *out_ref = cloned;
        return NULL;
    }

    static int match_tmp_counter = 0;
    char name_buf[64];
    snprintf(name_buf, sizeof(name_buf), "__match_tmp_%d", match_tmp_counter++);

    ASTNode* temp_ident = create_identifier_node(name_buf);
    ASTNode* temp_ref = create_identifier_node(name_buf);
    if (!temp_ident || !temp_ref) return NULL;

    ASTNode* bind_decl = create_assign_node(temp_ident, scrutinee);
    if (bind_decl) bind_decl->data.assign.is_declaration = 1;

    ASTNode* prelude = create_program_node();
    add_statement_to_program(prelude, bind_decl);
    if (out_ref) *out_ref = temp_ref;
    return prelude;
}

static void check_match_exhaustiveness(ASTNode* scrutinee, ASTNode* arms) {
    if (!arms || arms->type != AST_EXPRESSION_LIST) return;

    int count = arms->data.expression_list.expression_count;
    if (count <= 0) return;

    int has_wildcard = 0;
    const char* pattern_names[64];
    int pattern_count = 0;

    for (int i = 0; i < count; i++) {
        ASTNode* arm = arms->data.expression_list.expressions[i];
        if (!arm || arm->type != AST_ASSIGN || !arm->data.assign.left) continue;

        ASTNode* pattern = arm->data.assign.left;
        const char* name = NULL;

        if (pattern->type == AST_IDENTIFIER && pattern->data.identifier.name) {
            name = pattern->data.identifier.name;
        } else if (pattern->type == AST_CALL && pattern->data.call.func &&
                   pattern->data.call.func->type == AST_IDENTIFIER &&
                   pattern->data.call.func->data.identifier.name) {
            name = pattern->data.call.func->data.identifier.name;
        }

        if (name && strcmp(name, "_") == 0) {
            has_wildcard = 1;
            break;
        }
        if (name && pattern_count < 64) {
            pattern_names[pattern_count++] = name;
        }
    }

    if (has_wildcard || pattern_count == 0) return;

    int def_idx = -1;
    for (int i = 0; i < pattern_count; i++) {
        int di = -1;
        find_adt_ctor_index(pattern_names[i], &di);
        if (di >= 0) {
            if (def_idx < 0) {
                def_idx = di;
            } else if (def_idx != di) {
                return;
            }
        }
    }

    if (def_idx < 0) return;

    int ctor_count = g_adt_defs[def_idx].ctor_count;
    if (ctor_count <= 0) return;

    char missing[512];
    int pos = 0;
    int missing_count = 0;

    for (int c = 0; c < ctor_count; c++) {
        const char* ctor_name = g_adt_defs[def_idx].ctors[c].name;
        if (!ctor_name) continue;

        int found = 0;
        for (int p = 0; p < pattern_count; p++) {
            if (strcmp(pattern_names[p], ctor_name) == 0) {
                found = 1;
                break;
            }
        }

        if (!found) {
            if (missing_count > 0 && pos < (int)sizeof(missing) - 2) {
                missing[pos++] = ',';
                missing[pos++] = ' ';
            }
            int len = (int)strlen(ctor_name);
            if (pos + len + 2 < (int)sizeof(missing)) {
                missing[pos++] = '\'';
                memcpy(missing + pos, ctor_name, (size_t)len);
                pos += len;
                missing[pos++] = '\'';
            }
            missing_count++;
        }
    }
    missing[pos] = '\0';

    if (missing_count > 0) {
        char msg[640];
        snprintf(msg, sizeof(msg), "non-exhaustive match: missing %s", missing);

        int line = scrutinee ? scrutinee->location.first_line : yylineno;
        int col = scrutinee ? scrutinee->location.first_column : 1;
        set_location_with_column(current_input_filename ? current_input_filename : "unknown", line, col);
        report_simple_error(ERROR_LEVEL_ERROR, ERROR_SEMANTIC, msg);
    }
}

static ASTNode* create_array_length_expr(ASTNode* scrutinee) {
    ASTNode* target = clone_match_scrutinee(scrutinee);
    if (!target) return NULL;
    return create_member_access_node(target, create_identifier_node("length"));
}

static ASTNode* create_array_pattern_index(ASTNode* scrutinee, int element_index,
                                           int element_count, int rest_index) {
    ASTNode* target = clone_match_scrutinee(scrutinee);
    if (!target) return NULL;

    ASTNode* index = NULL;
    if (rest_index >= 0 && element_index >= rest_index) {
        int suffix_offset = element_count - element_index;
        index = create_binop_node(OP_SUB, create_array_length_expr(scrutinee),
                                  create_num_int_node(suffix_offset));
    } else {
        index = create_num_int_node(element_index);
    }
    if (!index) return NULL;
    return create_index_node(target, index);
}

static ASTNode* build_array_pattern_arm(ASTNode* scrutinee, ASTNode* pattern,
                                        ASTNode* body, ASTNode* fallback) {
    if (!scrutinee || !pattern || pattern->type != AST_ARRAY_PATTERN || !body) {
        return fallback;
    }

    int count = pattern->data.array_pattern.element_count;
    int rest_index = pattern->data.array_pattern.rest_index;
    ASTNode* length = create_array_length_expr(scrutinee);
    ASTNode* condition = create_binop_node(
        rest_index >= 0 ? OP_GE : OP_EQ,
        length,
        create_num_int_node(count)
    );
    ASTNode* bindings = create_program_node();

    for (int i = 0; i < count; i++) {
        ASTNode* element = pattern->data.array_pattern.elements[i];
        if (!element) continue;

        ASTNode* indexed = create_array_pattern_index(scrutinee, i, count, rest_index);
        if (!indexed) continue;

        if (element->type == AST_IDENTIFIER && element->data.identifier.name) {
            if (strcmp(element->data.identifier.name, "_") != 0) {
                ASTNode* binding = create_assign_node(
                    create_identifier_node(element->data.identifier.name), indexed);
                if (binding) {
                    binding->data.assign.is_declaration = 1;
                    add_statement_to_program(bindings, binding);
                }
            }
            continue;
        }

        ASTNode* expected = clone_match_scrutinee(element);
        if (!expected) continue;
        ASTNode* element_condition = create_binop_node(OP_EQ, indexed, expected);
        condition = create_binop_node(OP_AND, condition, element_condition);
    }

    body = prepend_bindings_to_match_body(body, bindings);
    return create_if_node(condition, body, fallback);
}

static ASTNode* build_match_desugared(ASTNode* scrutinee, ASTNode* arms) {
    if (!scrutinee || !arms || arms->type != AST_EXPRESSION_LIST) return NULL;

    check_match_exhaustiveness(scrutinee, arms);

    ASTNode* scrutinee_ref = NULL;
    ASTNode* prelude = materialize_match_scrutinee(scrutinee, &scrutinee_ref);
    if (!scrutinee_ref) return NULL;

    int count = arms->data.expression_list.expression_count;
    ASTNode* chain = NULL;

    for (int i = count - 1; i >= 0; i--) {
        ASTNode* arm = arms->data.expression_list.expressions[i];
        if (!arm || arm->type != AST_ASSIGN || !arm->data.assign.left || !arm->data.assign.right) continue;

        ASTNode* pattern = arm->data.assign.left;
        ASTNode* body = arm->data.assign.right;
        int is_multi = arm->data.assign.is_multi_pattern;

        if (!is_multi && pattern->type == AST_ARRAY_PATTERN) {
            chain = build_array_pattern_arm(scrutinee_ref, pattern, body, chain);
            continue;
        }

        // Handle wildcard pattern
        if (!is_multi && pattern && pattern->type == AST_IDENTIFIER && pattern->data.identifier.name &&
            strcmp(pattern->data.identifier.name, "_") == 0) {
            if (i != count - 1) {
                int line = pattern->location.first_line > 0 ? pattern->location.first_line : yylineno;
                int col = pattern->location.first_column > 0 ? pattern->location.first_column : 1;
                set_location_with_column(current_input_filename ? current_input_filename : "unknown", line, col);
                report_simple_error(ERROR_LEVEL_WARNING, ERROR_WARNING,
                    "'_' match arm should be the last arm!");
            }
            chain = body;
            continue;
        }

        // Handle multi-pattern arms (e.g., "int" | "bool" | "string" -> ...)
        if (is_multi && pattern && pattern->type == AST_EXPRESSION_LIST) {
            int pat_count = pattern->data.expression_list.expression_count;
            ASTNode* combined_cond = NULL;

            for (int j = 0; j < pat_count; j++) {
                ASTNode* single_pattern = pattern->data.expression_list.expressions[j];
                if (!single_pattern) continue;

                ASTNode* cond_left = clone_match_scrutinee(scrutinee_ref);
                ASTNode* cond_right = clone_match_scrutinee(single_pattern);
                if (!cond_left || !cond_right) continue;

                ASTNode* single_cond = create_binop_node(OP_EQ, cond_left, cond_right);
                if (!single_cond) continue;

                if (!combined_cond) {
                    combined_cond = single_cond;
                } else {
                    combined_cond = create_binop_node(OP_OR, combined_cond, single_cond);
                }
            }

            if (combined_cond) {
                chain = create_if_node(combined_cond, body, chain);
            }
            continue;
        }

        ASTNode* cond = NULL;

        if (pattern->type == AST_CALL && pattern->data.call.func &&
            pattern->data.call.func->type == AST_IDENTIFIER &&
            pattern->data.call.func->data.identifier.name) {
            const char* ctor_name = pattern->data.call.func->data.identifier.name;
            ASTNode* cond_left = clone_match_scrutinee(scrutinee_ref);
            if (!cond_left) continue;

            if (is_builtin_union_ctor_name(ctor_name)) {
                if (strcmp(ctor_name, "None") == 0) {
                    ASTNode* tag_access = create_member_access_node(
                        clone_match_scrutinee(scrutinee_ref), create_identifier_node("0"));
                    cond = create_binop_node(OP_EQ, tag_access, create_num_int_node(1));
                } else if (strcmp(ctor_name, "Some") == 0) {
                    ASTNode* tag_access = create_member_access_node(
                        clone_match_scrutinee(scrutinee_ref), create_identifier_node("0"));
                    cond = create_binop_node(OP_EQ, tag_access, create_num_int_node(0));
                }
            }

            int is_adt_ctor = is_builtin_union_ctor_name(ctor_name) &&
                             (strcmp(ctor_name, "Ok") == 0 || strcmp(ctor_name, "Err") == 0 ||
                              strcmp(ctor_name, "Some") == 0);
            int is_custom_adt_ctor = !is_adt_ctor && vix_adt_ctor_index(ctor_name) >= 0;
            int adt_ctor_tag = is_custom_adt_ctor ? vix_adt_ctor_index(ctor_name) : -1;

            if (pattern->data.call.args && pattern->data.call.args->type == AST_EXPRESSION_LIST &&
                pattern->data.call.args->data.expression_list.expression_count == 1) {
                ASTNode* bind_arg = pattern->data.call.args->data.expression_list.expressions[0];
                if (bind_arg && bind_arg->type == AST_IDENTIFIER && bind_arg->data.identifier.name) {
                    if (is_adt_ctor || is_custom_adt_ctor) {
                        ASTNode* payload_access = create_member_access_node(
                            clone_match_scrutinee(scrutinee_ref), create_identifier_node("1"));
                        ASTNode* bind_left = create_identifier_node(bind_arg->data.identifier.name);
                        ASTNode* bind_decl = create_assign_node(bind_left, payload_access);
                        if (bind_decl) bind_decl->data.assign.is_declaration = 1;

                        ASTNode* wrapped = create_program_node();
                        add_statement_to_program(wrapped, bind_decl);
                        if (body->type == AST_PROGRAM) {
                            for (int j = 0; j < body->data.program.statement_count; j++) {
                                add_statement_to_program(wrapped, body->data.program.statements[j]);
                            }
                        } else {
                            add_statement_to_program(wrapped, body);
                        }
                        body = wrapped;
                    } else {
                        body = prepend_binding_to_match_body(body, bind_arg->data.identifier.name, scrutinee_ref);
                    }
                }
            }

            if (!cond) {
                if (is_adt_ctor || is_custom_adt_ctor) {
                    ASTNode* tag_access = create_member_access_node(
                        clone_match_scrutinee(scrutinee_ref), create_identifier_node("0"));
                    if (is_custom_adt_ctor) {
                        cond = create_binop_node(OP_EQ, tag_access, create_identifier_node(ctor_name));
                    } else {
                        ASTNode* cond_right = create_identifier_node(ctor_name);
                        cond = create_binop_node(OP_EQ, tag_access, cond_right);
                    }
                } else {
                    ASTNode* cond_right = create_identifier_node(ctor_name);
                    if (is_builtin_union_ctor_name(ctor_name) && strcmp(ctor_name, "None") == 0) {
                        cond_right = create_nil_node();
                    }
                    cond = create_binop_node(OP_EQ, cond_left, cond_right);
                }
            }
        } else {
            ASTNode* cond_left = clone_match_scrutinee(scrutinee_ref);//克隆 scrutinee 以构建条件表达式，确保不修改原始 scrutinee
            ASTNode* cond_right = clone_match_scrutinee(pattern);//克隆模式以构建条件表达式，确保不修改原始模式

            // Check for builtin ctor names (None/Some) BEFORE falling through
            if (pattern->type == AST_IDENTIFIER && pattern->data.identifier.name &&
                is_builtin_union_ctor_name(pattern->data.identifier.name)) {
                const char* pname = pattern->data.identifier.name;
                if (strcmp(pname, "None") == 0) {
                    ASTNode* tag_access = create_member_access_node(
                        clone_match_scrutinee(scrutinee_ref), create_identifier_node("0"));
                    cond = create_binop_node(OP_EQ, tag_access, create_num_int_node(1));
                } else if (strcmp(pname, "Some") == 0) {
                    ASTNode* tag_access = create_member_access_node(
                        clone_match_scrutinee(scrutinee_ref), create_identifier_node("0"));
                    cond = create_binop_node(OP_EQ, tag_access, create_num_int_node(0));
                }
            }
            // Check for custom ADT constructors (simple identifier, no payload)
            if (!cond && pattern->type == AST_IDENTIFIER && pattern->data.identifier.name &&
                vix_adt_ctor_index(pattern->data.identifier.name) >= 0) {
                ASTNode* tag_access = create_member_access_node(
                    clone_match_scrutinee(scrutinee_ref), create_identifier_node("0"));
                cond = create_binop_node(OP_EQ, tag_access, create_identifier_node(pattern->data.identifier.name));
            }

            if (!cond) {
                if (!cond_right && pattern->type == AST_IDENTIFIER && pattern->data.identifier.name) {
                    cond_right = create_identifier_node(pattern->data.identifier.name);
                }
                if (!cond_left || !cond_right) {
                    continue;
                }
                cond = create_binop_node(OP_EQ, cond_left, cond_right);//构建条件表达式：scrutinee == pattern
            }
        }

        if (!cond) continue;
        chain = create_if_node(cond, body, chain);//构建 if-else 链：if (xxxx == xxxxx) { body } else do_something_e
    }

    if (!prelude) {
        return chain;
    }

    if (chain) {
        add_statement_to_program(prelude, chain);
    }
    return prelude;
}
/*
build_type_alias_enum：将枚举类型转换为常量定义
clone_match_scrutinee：克隆 match 语句的 scrutinee，确保在生成条件表达式时不会修改原始 scrutinee
build_match_desugared：将 match 表达式转换为嵌套的 ifelse 表达式
*/
%}

%union {
    long long num_int;
    double num_float;
    char* str;
    struct ASTNode* node;
}

%define lr.type ielr

%token <str> IDENTIFIER STRING
%token STRUCT COLON
%token TYPE_KW MATCH PIPE PIPE_GT
%token QUESTION
%token LET MUT REF_KW
%token IMPORT PUB IMPL
%token <num_int> NUMBER_INT CHAR_LITERAL BOOL_LITERAL
%token <num_float> NUMBER_FLOAT
%token PRINT INPUT TYPE_I32 TYPE_I64 TYPE_I8 TYPE_F32 TYPE_F64 TYPE_STR TYPE_PTR FN ARROW RETURN TYPE_VOID NIL EXTERN DOTDOTDOT
%token AND OR
%token AT AMPERSAND BANG
%token IF ELSE ELIF WHILE FOR BREAK CONTINUE IN
%token ASSIGN PLUS_ASSIGN MINUS_ASSIGN MULTIPLY_ASSIGN DIVIDE_ASSIGN MODULO_ASSIGN
%token PLUS MINUS MULTIPLY DIVIDE MODULO POWER
%token EQ NE LT LE GT GE
%token DOT DOTDOT LBRACKET RBRACKET
%token LPAREN RPAREN LBRACE RBRACE SEMICOLON COMMA
%token ERROR
%left OR
%left AND
%nonassoc EQ NE LT LE GT GE
%left PLUS MINUS
%left MULTIPLY DIVIDE MODULO
%right POWER

%type <node> program statement_list statement 
%type <node> extern_block extern_decl extern_decl_list
%type <node> struct_fields struct_field struct_init_fields struct_init_field
%type <node> type param_list function_definition pub_function_definition function_return_type
%type <node> print_statement assignment_statement compound_assignment_statement
%type <node> if_statement while_statement for_statement
%type <node> expression pipeline_expression logical_expression comparison_expression additive_expression term factor power factor_unary
%type <node> literal identifier input_expression
%type <node> block_statement if_rest expression_list

%type <node> type_definition enum_variant_list match_statement match_arms match_arm match_arm_body match_target match_arm_pattern match_arm_patterns
%type <node> match_scalar_pattern match_array_pattern match_array_pattern_items match_array_pattern_element
%type <node> generic_param_list generic_type_args enum_variant
%type <node> type_list
%type <node> impl_block impl_method_list

%nonassoc IF
%nonassoc ELSE

%start program

%%

program
    : statement_list { root = $$ = $1; }
    ;

statement_list
    : statement                  { 
                                   $$ = create_program_node_with_yyltype((YYLTYPE*) &@$);
                                   add_statement_to_program($$, $1);
                               }
    | statement_list SEMICOLON statement   {
                                   add_statement_to_program($1, $3);
                                   $$ = $1;
                               }
    | statement_list statement   {
                                   add_statement_to_program($1, $2);
                                   $$ = $1;
                               }
    ;

statement
    : IMPORT STRING SEMICOLON { $$ = create_import_node_with_yyltype($2, (YYLTYPE*) &@$); }
    | IMPORT STRING { $$ = create_import_node_with_yyltype($2, (YYLTYPE*) &@$); }
    | while_statement               { $$ = $1; }
    | for_statement               { $$ = $1; }
    | print_statement               { $$ = $1; }
    | assignment_statement          { $$ = $1; }
    | compound_assignment_statement { $$ = $1; }
    | LET identifier ASSIGN expression {
        $$ = create_assign_node_with_yyltype($2, $4, (YYLTYPE*) &@$);
        $$->data.assign.is_declaration = 1;
    }
    | LET identifier ASSIGN expression COLON type {
        $$ = create_assign_node_with_yyltype($2, $4, (YYLTYPE*) &@$);
        $$->data.assign.is_declaration = 1;
        $$->data.assign.declared_type = $6;
    }
    | LET MUT identifier ASSIGN expression {
        $$ = create_assign_node_with_yyltype($3, $5, (YYLTYPE*) &@$);
        $3->mutability = MUTABILITY_MUTABLE;
        $$->data.assign.is_declaration = 1;
    }
    | LET identifier COLON type ASSIGN expression {
        $$ = create_assign_node_with_yyltype($2, $6, (YYLTYPE*) &@$);
        $$->data.assign.is_declaration = 1;
        $$->data.assign.declared_type = $4;
    }
    | LET identifier COLON type {
        ASTNode* init = create_default_value_for_type($4, (YYLTYPE*) &@$);
        $$ = create_assign_node_with_yyltype($2, init, (YYLTYPE*) &@$);
        $$->data.assign.is_declaration = 1;
        $$->data.assign.declared_type = $4;
    }
    | LET MUT identifier COLON type ASSIGN expression {
        $$ = create_assign_node_with_yyltype($3, $7, (YYLTYPE*) &@$);
        $3->mutability = MUTABILITY_MUTABLE;
        $$->data.assign.is_declaration = 1;
        $$->data.assign.declared_type = $5;
    }
    | LET MUT identifier COLON type {
        ASTNode* init = create_default_value_for_type($5, (YYLTYPE*) &@$);
        $$ = create_assign_node_with_yyltype($3, init, (YYLTYPE*) &@$);
        $3->mutability = MUTABILITY_MUTABLE;
        $$->data.assign.is_declaration = 1;
        $$->data.assign.declared_type = $5;
    }
    | identifier COLON expression { $$ = create_assign_node_with_yyltype($1, $3, (YYLTYPE*) &@$); }
    | identifier COLON type ASSIGN expression { 
        $$ = create_assign_node_with_yyltype($1, $5, (YYLTYPE*) &@$); 
    }
    | pub_function_definition           { $$ = $1; }
    | function_definition           { $$ = $1; }
    | extern_block                 { $$ = $1; }
    | STRUCT IDENTIFIER LBRACE struct_fields RBRACE {
        register_generic_arity($2, GENERIC_KIND_STRUCT, 0);
        {
            int line = @2.first_line > 0 ? @2.first_line : yylineno;
            int col = @2.first_column > 0 ? @2.first_column : 1;
            set_location_with_column(current_input_filename ? current_input_filename : "unknown", line, col);
            report_simple_error(ERROR_LEVEL_WARNING, ERROR_WARNING,
                "deprecated syntax: use 'type NAME = struct {...}' instead of 'struct NAME {...}'");
        }
        $$ = create_struct_def_node_with_yyltype($2, $4, (YYLTYPE*) &@$);
    }
    | PUB STRUCT IDENTIFIER LBRACE struct_fields RBRACE {
        register_generic_arity($3, GENERIC_KIND_STRUCT, 0);
        {
            int line = @3.first_line > 0 ? @3.first_line : yylineno;
            int col = @3.first_column > 0 ? @3.first_column : 1;
            set_location_with_column(current_input_filename ? current_input_filename : "unknown", line, col);
            report_simple_error(ERROR_LEVEL_WARNING, ERROR_WARNING,
                "deprecated syntax: use 'pub type NAME = struct {...}' instead of 'pub struct NAME {...}'");
        }
        $$ = create_struct_def_node_with_yyltype($3, $5, (YYLTYPE*) &@$);
        $$->data.struct_def.is_public = 1;
    }
    | STRUCT IDENTIFIER COLON LBRACKET generic_param_list RBRACKET LBRACE struct_fields RBRACE {
        register_generic_arity($2, GENERIC_KIND_STRUCT, node_list_count($5));
        {
            int line = @2.first_line > 0 ? @2.first_line : yylineno;
            int col = @2.first_column > 0 ? @2.first_column : 1;
            set_location_with_column(current_input_filename ? current_input_filename : "unknown", line, col);
            report_simple_error(ERROR_LEVEL_WARNING, ERROR_WARNING,
                "deprecated syntax: use 'type NAME:[T] = struct {...}' instead of 'struct NAME:[T] {...}'");
        }
        $$ = create_struct_def_node_with_yyltype($2, $8, (YYLTYPE*) &@$);
        $$->data.struct_def.generic_params = $5;
    }
    | PUB STRUCT IDENTIFIER COLON LBRACKET generic_param_list RBRACKET LBRACE struct_fields RBRACE {
        register_generic_arity($3, GENERIC_KIND_STRUCT, node_list_count($6));
        {
            int line = @3.first_line > 0 ? @3.first_line : yylineno;
            int col = @3.first_column > 0 ? @3.first_column : 1;
            set_location_with_column(current_input_filename ? current_input_filename : "unknown", line, col);
            report_simple_error(ERROR_LEVEL_WARNING, ERROR_WARNING,
                "deprecated syntax: use 'pub type NAME:[T] = struct {...}' instead of 'pub struct NAME:[T] {...}'");
        }
        $$ = create_struct_def_node_with_yyltype($3, $9, (YYLTYPE*) &@$);
        $$->data.struct_def.generic_params = $6;
        $$->data.struct_def.is_public = 1;
    }
    | type_definition              { $$ = $1; }
    | impl_block                   { $$ = $1; }
    | match_statement              { $$ = $1; }
    | RETURN expression            { $$ = create_return_node_with_yyltype($2, (YYLTYPE*) &@$); }
    | RETURN                       { $$ = create_return_node_with_yyltype(NULL, (YYLTYPE*) &@$); }
    | expression                   { $$ = $1; }
    | MUT identifier ASSIGN expression { 
        $$ = create_assign_node_with_yyltype($2, $4, (YYLTYPE*) &@$); 
        $2->mutability = MUTABILITY_MUTABLE;
    }
    | MUT identifier COLON type ASSIGN expression { 
        $$ = create_assign_node_with_yyltype($2, $6, (YYLTYPE*) &@$); 
        $2->mutability = MUTABILITY_MUTABLE;
    }
    | BREAK                         { $$ = create_break_node_with_yyltype((YYLTYPE*) &@$); }
    | CONTINUE                      { $$ = create_continue_node_with_yyltype((YYLTYPE*) &@$); }
    ;

type_definition
    : TYPE_KW IDENTIFIER ASSIGN enum_variant_list {
        register_generic_arity($2, GENERIC_KIND_TYPE, 0);
        register_adt_definition($2, 0, $4);
        $$ = build_type_alias_enum($2, $4);
    }
    | PUB TYPE_KW IDENTIFIER ASSIGN enum_variant_list {
        register_generic_arity($3, GENERIC_KIND_TYPE, 0);
        register_adt_definition($3, 0, $5);
        $$ = mark_type_alias_public(build_type_alias_enum($3, $5));
    }
    | TYPE_KW IDENTIFIER ASSIGN STRUCT LBRACE struct_fields RBRACE {
        register_generic_arity($2, GENERIC_KIND_STRUCT, 0);
        $$ = create_struct_def_node_with_yyltype($2, $6, (YYLTYPE*) &@$);
    }
    | PUB TYPE_KW IDENTIFIER ASSIGN STRUCT LBRACE struct_fields RBRACE {
        register_generic_arity($3, GENERIC_KIND_STRUCT, 0);
        $$ = create_struct_def_node_with_yyltype($3, $7, (YYLTYPE*) &@$);
        $$->data.struct_def.is_public = 1;
    }
    | TYPE_KW IDENTIFIER COLON LBRACKET generic_param_list RBRACKET ASSIGN enum_variant_list {
        register_generic_arity($2, GENERIC_KIND_TYPE, node_list_count($5));
        register_adt_definition($2, node_list_count($5), $8);
        $$ = build_type_alias_enum($2, $8);
    }
    | PUB TYPE_KW IDENTIFIER COLON LBRACKET generic_param_list RBRACKET ASSIGN enum_variant_list {
        register_generic_arity($3, GENERIC_KIND_TYPE, node_list_count($6));
        register_adt_definition($3, node_list_count($6), $9);
        $$ = mark_type_alias_public(build_type_alias_enum($3, $9));
    }
    | TYPE_KW IDENTIFIER COLON LBRACKET generic_param_list RBRACKET ASSIGN STRUCT LBRACE struct_fields RBRACE {
        register_generic_arity($2, GENERIC_KIND_STRUCT, node_list_count($5));
        $$ = create_struct_def_node_with_yyltype($2, $10, (YYLTYPE*) &@$);
        $$->data.struct_def.generic_params = $5;
    }
    | PUB TYPE_KW IDENTIFIER COLON LBRACKET generic_param_list RBRACKET ASSIGN STRUCT LBRACE struct_fields RBRACE {
        register_generic_arity($3, GENERIC_KIND_STRUCT, node_list_count($6));
        $$ = create_struct_def_node_with_yyltype($3, $11, (YYLTYPE*) &@$);
        $$->data.struct_def.generic_params = $6;
        $$->data.struct_def.is_public = 1;
    }

    ;

impl_block
    : IMPL IDENTIFIER LBRACE impl_method_list RBRACE {
        ASTNode* prog = create_program_node_with_yyltype((YYLTYPE*) &@$);
        ASTNode* methods = $4;
        /* Normalize type name: "String" -> "string", "I32" -> "i32", etc. */
        char* normalized_type = strdup($2);
        if (normalized_type) {
            for (char* p = normalized_type; *p; p++) {
                *p = tolower((unsigned char)*p);
            }
        }
        const char* reg_type = normalized_type ? normalized_type : $2;
        if (methods && methods->type == AST_EXPRESSION_LIST) {
            int cnt = methods->data.expression_list.expression_count;
            for (int i = 0; i < cnt; i++) {
                ASTNode* fn = methods->data.expression_list.expressions[i];
                if (fn && fn->type == AST_FUNCTION && fn->data.function.name) {
                    char mangled[512];
                    snprintf(mangled, sizeof(mangled), "%s.%s", $2, fn->data.function.name);
                    register_impl_method(reg_type, fn->data.function.name, mangled);
                    free(fn->data.function.name);
                    fn->data.function.name = strdup(mangled);
                    add_statement_to_program(prog, fn);
                }
            }
        }
        free(normalized_type);
        free($2);
        $$ = prog;
    }
    | IMPL TYPE_STR LBRACE impl_method_list RBRACE {
        ASTNode* prog = create_program_node_with_yyltype((YYLTYPE*) &@$);
        ASTNode* methods = $4;
        if (methods && methods->type == AST_EXPRESSION_LIST) {
            int cnt = methods->data.expression_list.expression_count;
            for (int i = 0; i < cnt; i++) {
                ASTNode* fn = methods->data.expression_list.expressions[i];
                if (fn && fn->type == AST_FUNCTION && fn->data.function.name) {
                    char mangled[512];
                    snprintf(mangled, sizeof(mangled), "string.%s", fn->data.function.name);
                    register_impl_method("string", fn->data.function.name, mangled);
                    free(fn->data.function.name);
                    fn->data.function.name = strdup(mangled);
                    add_statement_to_program(prog, fn);
                }
            }
        }
        $$ = prog;
    }
    | IMPL TYPE_I32 LBRACE impl_method_list RBRACE {
        ASTNode* prog = create_program_node_with_yyltype((YYLTYPE*) &@$);
        ASTNode* methods = $4;
        if (methods && methods->type == AST_EXPRESSION_LIST) {
            int cnt = methods->data.expression_list.expression_count;
            for (int i = 0; i < cnt; i++) {
                ASTNode* fn = methods->data.expression_list.expressions[i];
                if (fn && fn->type == AST_FUNCTION && fn->data.function.name) {
                    char mangled[512];
                    snprintf(mangled, sizeof(mangled), "i32.%s", fn->data.function.name);
                    register_impl_method("i32", fn->data.function.name, mangled);
                    free(fn->data.function.name);
                    fn->data.function.name = strdup(mangled);
                    add_statement_to_program(prog, fn);
                }
            }
        }
        $$ = prog;
    }

    | IMPL IDENTIFIER COLON LBRACKET generic_param_list RBRACKET LBRACE impl_method_list RBRACE {
        ASTNode* prog = create_program_node_with_yyltype((YYLTYPE*) &@$);
        ASTNode* methods = $8;
        int generic_arity = node_list_count($5);
        /* Normalize type name */
        char* normalized_type = strdup($2);
        if (normalized_type) {
            for (char* p = normalized_type; *p; p++) {
                *p = tolower((unsigned char)*p);
            }
        }
        const char* reg_type = normalized_type ? normalized_type : $2;
        if (methods && methods->type == AST_EXPRESSION_LIST) {
            int cnt = methods->data.expression_list.expression_count;
            for (int i = 0; i < cnt; i++) {
                ASTNode* fn = methods->data.expression_list.expressions[i];
                if (fn && fn->type == AST_FUNCTION && fn->data.function.name) {
                    char mangled[512];
                    snprintf(mangled, sizeof(mangled), "%s.%s", $2, fn->data.function.name);
                    register_impl_method(reg_type, fn->data.function.name, mangled);
                    free(fn->data.function.name);
                    fn->data.function.name = strdup(mangled);
                    /* Propagate generic params to the method if not already set */
                    if (!fn->data.function.generic_params || fn->data.function.generic_params->data.expression_list.expression_count == 0) {
                        fn->data.function.generic_params = $5;
                    }
                    add_statement_to_program(prog, fn);
                }
            }
        }
        register_generic_arity($2, GENERIC_KIND_STRUCT, generic_arity);
        free(normalized_type);
        free($2);
        $$ = prog;
    }

    ;

impl_method_list
    : /* empty */ { $$ = create_expression_list_node_with_yyltype((YYLTYPE*) &@$); }
    | impl_method_list function_definition {
        add_expression_to_list($1, $2);
        $$ = $1;
    }
    | impl_method_list pub_function_definition {
        add_expression_to_list($1, $2);
        $$ = $1;
    }
    ;

enum_variant_list
    : enum_variant {
        ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        add_expression_to_list(list, $1);
        $$ = list;
    }
    | PIPE enum_variant {
        ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        add_expression_to_list(list, $2);
        $$ = list;
    }
    | enum_variant_list PIPE enum_variant {
        add_expression_to_list($1, $3);
        $$ = $1;
    }
    ;

enum_variant
    : IDENTIFIER {
        $$ = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$);
        g_adt_payload_types[g_adt_payload_type_count++] = NULL;
    }
    | IDENTIFIER LPAREN type RPAREN {
        $$ = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$);
        $$->mutability = (MutabilityType)1;
        g_adt_payload_types[g_adt_payload_type_count++] = $3;
    }
    ;

generic_param_list
    : IDENTIFIER {
        ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        add_expression_to_list(list, create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$));
        $$ = list;
    }
    | generic_param_list COMMA IDENTIFIER {
        add_expression_to_list($1, create_identifier_node_with_yyltype($3, (YYLTYPE*) &@$));
        $$ = $1;
    }
    ;

generic_type_args
    : type {
        ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        add_expression_to_list(list, $1);
        $$ = list;
    }
    | generic_type_args COMMA type {
        add_expression_to_list($1, $3);
        $$ = $1;
    }
    ;

match_statement
    : MATCH match_target LBRACE match_arms RBRACE {
        $$ = build_match_desugared($2, $4);
    }
    ;

match_target
    : identifier { $$ = $1; }
    | identifier DOT IDENTIFIER {
        $$ = create_member_access_node_with_yyltype($1, create_identifier_node_with_yyltype($3, (YYLTYPE*) &@$), (YYLTYPE*) &@$);
    }
    | literal { $$ = $1; }
    | LPAREN expression RPAREN { $$ = $2; }
    | IDENTIFIER LPAREN RPAREN {
        ASTNode* id = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$);
        $$ = create_call_node_with_yyltype(id, NULL, (YYLTYPE*) &@$);
    }
    | IDENTIFIER LPAREN expression_list RPAREN {
        ASTNode* id = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$);
        $$ = create_call_node_with_yyltype(id, $3, (YYLTYPE*) &@$);
    }
    ;

match_arms
    : match_arm {
        ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        add_expression_to_list(list, $1);
        $$ = list;
    }
    | match_arms match_arm {
        add_expression_to_list($1, $2);
        $$ = $1;
    }
    ;

match_arm
    : match_arm_pattern ARROW match_arm_body {
        $$ = create_assign_node_with_yyltype($1, $3, (YYLTYPE*) &@$);
    }
    | match_arm_patterns PIPE match_arm_pattern ARROW match_arm_body {
        // Multiple patterns: duplicate body for each pattern
        // Store all patterns in $1, add $3
        add_expression_to_list($1, $3);
        $$ = create_assign_node_with_yyltype($1, $5, (YYLTYPE*) &@$);
        // Mark this as a multi-pattern arm
        $$->data.assign.is_multi_pattern = 1;
    }
    ;

match_arm_patterns
    : match_arm_pattern {
        ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        add_expression_to_list(list, $1);
        $$ = list;
    }
    | match_arm_patterns PIPE match_arm_pattern {
        add_expression_to_list($1, $3);
        $$ = $1;
    }
    ;

match_arm_pattern
    : identifier { $$ = $1; }
    | match_scalar_pattern { $$ = $1; }
    | match_array_pattern { $$ = $1; }
    | IDENTIFIER LPAREN IDENTIFIER RPAREN {
        ASTNode* ctor = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$);
        ASTNode* args = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        ASTNode* bind_id = create_identifier_node_with_yyltype($3, (YYLTYPE*) &@$);
        add_expression_to_list(args, bind_id);
        $$ = create_call_node_with_yyltype(ctor, args, (YYLTYPE*) &@$);
    }
    ;

match_scalar_pattern
    : NUMBER_INT { $$ = create_num_int_node_with_yyltype($1, (YYLTYPE*) &@$); }
    | BOOL_LITERAL { $$ = create_bool_node_with_yyltype($1, (YYLTYPE*) &@$); }
    | NUMBER_FLOAT { $$ = create_num_float_node_with_yyltype($1, (YYLTYPE*) &@$); }
    | STRING { $$ = create_string_node_with_yyltype($1, (YYLTYPE*) &@$); }
    | CHAR_LITERAL { $$ = create_char_node_with_yyltype((char)$1, (YYLTYPE*) &@$); }
    | NIL { $$ = create_nil_node_with_yyltype((YYLTYPE*) &@$); }
    ;

match_array_pattern
    : LBRACKET RBRACKET {
        $$ = create_array_pattern_node_with_yyltype((YYLTYPE*) &@$);
    }
    | LBRACKET match_array_pattern_items RBRACKET { $$ = $2; }
    ;

match_array_pattern_items
    : match_array_pattern_element {
        $$ = create_array_pattern_node_with_yyltype((YYLTYPE*) &@$);
        add_array_pattern_element($$, $1);
    }
    | DOTDOT {
        $$ = create_array_pattern_node_with_yyltype((YYLTYPE*) &@$);
        set_array_pattern_rest($$);
    }
    | match_array_pattern_items COMMA match_array_pattern_element {
        add_array_pattern_element($1, $3);
        $$ = $1;
    }
    | match_array_pattern_items COMMA DOTDOT {
        if (!set_array_pattern_rest($1)) {
            yyerror("array pattern may contain at most one '..'");
        }
        $$ = $1;
    }
    ;

match_array_pattern_element
    : identifier { $$ = $1; }
    | match_scalar_pattern { $$ = $1; }
    ;

match_arm_body
    : block_statement { $$ = $1; }
    | print_statement { $$ = $1; }
    | RETURN expression { $$ = create_return_node_with_yyltype($2, (YYLTYPE*) &@$); }
    | RETURN { $$ = create_return_node_with_yyltype(NULL, (YYLTYPE*) &@$); }
    | expression { $$ = $1; }
    ;

input_expression
    : INPUT LPAREN STRING RPAREN { 
        ASTNode* prompt = create_string_node_with_yyltype($3, (YYLTYPE*) &@$);
        $$ = create_input_node_with_yyltype(prompt, (YYLTYPE*) &@$); 
    }
    | INPUT LPAREN RPAREN { 
        ASTNode* prompt = create_string_node_with_yyltype("", (YYLTYPE*) &@$);
        $$ = create_input_node_with_yyltype(prompt, (YYLTYPE*) &@$); 
    }
    ;
struct_field
    : IDENTIFIER COLON type {
        $$ = create_assign_node_with_yyltype(create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$), $3, (YYLTYPE*) &@$);
        $$->data.assign.is_declaration = 2;
    }
    ;

struct_fields
    : /* empty */ { $$ = create_expression_list_node_with_yyltype((YYLTYPE*) &@$); }
    | struct_field { ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$); add_expression_to_list(list, $1); $$ = list; }
    | struct_fields COMMA struct_field { add_expression_to_list($1, $3); $$ = $1; }
    | struct_fields struct_field { add_expression_to_list($1, $2); $$ = $1; }
    ;
struct_init_field
    : IDENTIFIER COLON expression { $$ = create_assign_node_with_yyltype(create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$), $3, (YYLTYPE*) &@$); }
    ;

struct_init_fields
    : /* empty */ { $$ = create_expression_list_node_with_yyltype((YYLTYPE*) &@$); }
    | struct_init_field { ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$); add_expression_to_list(list, $1); $$ = list; }
    | struct_init_fields COMMA struct_init_field { add_expression_to_list($1, $3); $$ = $1; }
    | struct_init_fields struct_init_field { add_expression_to_list($1, $2); $$ = $1; }
    ;

type_list
    : type {
        ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        add_expression_to_list(list, $1);
        $$ = list;
    }
    | type_list COMMA type {
        add_expression_to_list($1, $3);
        $$ = $1;
    }
    ;

type
    : TYPE_I32 { $$ = create_type_node(AST_TYPE_INT32); }
    | TYPE_I64 { $$ = create_type_node(AST_TYPE_INT64); }
    | TYPE_I8 { $$ = create_type_node(AST_TYPE_INT8); }
    | TYPE_F32 { $$ = create_type_node(AST_TYPE_FLOAT32); }
    | TYPE_F64 { $$ = create_type_node(AST_TYPE_FLOAT64); }
    | TYPE_STR { $$ = create_type_node(AST_TYPE_STRING); }
    | TYPE_VOID { $$ = create_type_node(AST_TYPE_VOID); }
        | TYPE_PTR LPAREN type RPAREN {
                $$ = create_type_node(AST_TYPE_POINTER);
                $$->data.pointer_type.element_type = $3;
            }
        | REF_KW type {
                $$ = create_type_node(AST_TYPE_POINTER);
                $$->data.pointer_type.element_type = $2;
            }
        | AMPERSAND type {
                $$ = create_type_node(AST_TYPE_POINTER);
                $$->data.pointer_type.element_type = $2;
            }
    | LBRACKET type MULTIPLY NUMBER_INT RBRACKET { $$ = create_fixed_size_list_type_node($2, $4); }
    | LBRACKET type RBRACKET { $$ = create_list_type_node($2); }
        | QUESTION type {
                ASTNode* option_ctor = create_identifier_node_with_yyltype(strdup("Option"), (YYLTYPE*) &@$);
                ASTNode* args = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
                add_expression_to_list(args, $2);
                $$ = create_type_app_node_with_yyltype(option_ctor, args, (YYLTYPE*) &@$);
            }
    | FN LPAREN RPAREN COLON type { $$ = create_type_node(AST_TYPE_POINTER); }
    | FN LPAREN type_list RPAREN COLON type { $$ = create_type_node(AST_TYPE_POINTER); }
    | LPAREN RPAREN { $$ = create_type_node(AST_TYPE_VOID); }
    | LPAREN type_list RPAREN {
        if ($2 && $2->type == AST_EXPRESSION_LIST && $2->data.expression_list.expression_count > 1) {
            $$ = $2;
        } else if ($2 && $2->type == AST_EXPRESSION_LIST && $2->data.expression_list.expression_count == 1) {
            $$ = $2->data.expression_list.expressions[0];
        } else {
            $$ = create_type_node(AST_TYPE_POINTER);
        }
    }
    | IDENTIFIER {
        $$ = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$); 
    }

    | IDENTIFIER COLON LBRACKET generic_type_args RBRACKET {
        check_generic_arity_usage($1, GENERIC_KIND_STRUCT, $4, (YYLTYPE*) &@$);
        check_generic_arity_usage($1, GENERIC_KIND_TYPE, $4, (YYLTYPE*) &@$);
        ASTNode* ctor = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$);
        $$ = create_type_app_node_with_yyltype(ctor, $4, (YYLTYPE*) &@$);
    }

    ;

param_list
    : identifier { ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$); add_expression_to_list(list, $1); $$ = list; }
    | identifier COLON type {
        ASTNode* id_node = create_identifier_node_with_yyltype($1->data.identifier.name, (YYLTYPE*) &@$);
        ASTNode* annotated_param = create_assign_node_with_yyltype(id_node, $3, (YYLTYPE*) &@$);
        annotated_param->data.assign.is_declaration = 2;
        ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        add_expression_to_list(list, annotated_param);
        $$ = list;
    }
    | MUT identifier {
        ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        ASTNode* id_node = create_identifier_node_with_yyltype($2->data.identifier.name, (YYLTYPE*) &@$);
        add_expression_to_list(list, id_node);
        $$ = list;
    }
    | MUT identifier COLON type {
        ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        ASTNode* id_node = create_identifier_node_with_yyltype($2->data.identifier.name, (YYLTYPE*) &@$);
        ASTNode* annotated_param = create_assign_node_with_mutability(id_node, $4, MUTABILITY_MUTABLE);
        annotated_param->data.assign.is_declaration = 2;
        add_expression_to_list(list, annotated_param);
        $$ = list;
    }
    | param_list COMMA identifier { add_expression_to_list($1, $3); $$ = $1; }
    | param_list COMMA identifier COLON type {
        ASTNode* id_node = create_identifier_node_with_yyltype($3->data.identifier.name, (YYLTYPE*) &@$);
        ASTNode* annotated_param = create_assign_node_with_yyltype(id_node, $5, (YYLTYPE*) &@$);
        annotated_param->data.assign.is_declaration = 2;
        add_expression_to_list($1, annotated_param);
        $$ = $1;
    }
    | param_list COMMA MUT identifier {
        ASTNode* id_node = create_identifier_node_with_yyltype($4->data.identifier.name, (YYLTYPE*) &@$);
        add_expression_to_list($1, id_node);
        $$ = $1;
    }
    | param_list COMMA MUT identifier COLON type {
        ASTNode* id_node = create_identifier_node_with_yyltype($4->data.identifier.name, (YYLTYPE*) &@$);
        ASTNode* annotated_param = create_assign_node_with_mutability(id_node, $6, MUTABILITY_MUTABLE);
        annotated_param->data.assign.is_declaration = 2;
        add_expression_to_list($1, annotated_param);
        $$ = $1;
    }
    ;

function_return_type
    : ARROW type {
        int line = @1.first_line > 0 ? @1.first_line : yylineno;
        int col = @1.first_column > 0 ? @1.first_column : 1;
        set_location_with_column(current_input_filename ? current_input_filename : "unknown", line, col);
        report_simple_error(ERROR_LEVEL_ERROR, ERROR_SYNTAX,
            "invalid function return syntax: use ': type' instead of '-> type'");
        $$ = $2;
    }
    | COLON type { $$ = $2; }
    ;

pub_function_definition
    : PUB FN IDENTIFIER LPAREN RPAREN function_return_type LBRACE statement_list RBRACE {
        register_generic_arity($3, GENERIC_KIND_FUNCTION, 0);
        $$ = create_public_function_node($3, NULL, $6, $8);
    }
    | PUB FN IDENTIFIER LPAREN param_list RPAREN function_return_type LBRACE statement_list RBRACE {
        register_generic_arity($3, GENERIC_KIND_FUNCTION, 0);
        $$ = create_public_function_node($3, $5, $7, $9);
    }
    | PUB FN IDENTIFIER LPAREN RPAREN function_return_type ASSIGN expression {
        register_generic_arity($3, GENERIC_KIND_FUNCTION, 0);
        ASTNode* body = create_program_node_with_yyltype((YYLTYPE*) &@$);
        ASTNode* ret = create_return_node_with_yyltype($8, (YYLTYPE*) &@$);
        add_statement_to_program(body, ret);
        $$ = create_public_function_node($3, NULL, $6, body);
    }
    | PUB FN IDENTIFIER LPAREN param_list RPAREN function_return_type ASSIGN expression {
        register_generic_arity($3, GENERIC_KIND_FUNCTION, 0);
        ASTNode* body = create_program_node_with_yyltype((YYLTYPE*) &@$);
        ASTNode* ret = create_return_node_with_yyltype($9, (YYLTYPE*) &@$);
        add_statement_to_program(body, ret);
        $$ = create_public_function_node($3, $5, $7, body);
    }
    | PUB FN IDENTIFIER LPAREN RPAREN LBRACE statement_list RBRACE {
        ASTNode* void_type = create_type_node(AST_TYPE_VOID);
        register_generic_arity($3, GENERIC_KIND_FUNCTION, 0);
        $$ = create_public_function_node($3, NULL, void_type, $7);
    }
    | PUB FN IDENTIFIER LPAREN param_list RPAREN LBRACE statement_list RBRACE {
        ASTNode* void_type = create_type_node(AST_TYPE_VOID);
        register_generic_arity($3, GENERIC_KIND_FUNCTION, 0);
        $$ = create_public_function_node($3, $5, void_type, $8);
    }

    | PUB FN IDENTIFIER COLON LBRACKET generic_param_list RBRACKET LPAREN RPAREN function_return_type LBRACE statement_list RBRACE {
        register_generic_arity($3, GENERIC_KIND_FUNCTION, node_list_count($6));
        $$ = create_public_function_node($3, NULL, $10, $12);
        $$->data.function.generic_params = $6;
    }
    | PUB FN IDENTIFIER COLON LBRACKET generic_param_list RBRACKET LPAREN param_list RPAREN function_return_type LBRACE statement_list RBRACE {
        register_generic_arity($3, GENERIC_KIND_FUNCTION, node_list_count($6));
        $$ = create_public_function_node($3, $9, $11, $13);
        $$->data.function.generic_params = $6;
    }
    | PUB FN IDENTIFIER COLON LBRACKET generic_param_list RBRACKET LPAREN RPAREN LBRACE statement_list RBRACE {
        ASTNode* void_type = create_type_node(AST_TYPE_VOID);
        register_generic_arity($3, GENERIC_KIND_FUNCTION, node_list_count($6));
        $$ = create_public_function_node($3, NULL, void_type, $11);
        $$->data.function.generic_params = $6;
    }
    | PUB FN IDENTIFIER COLON LBRACKET generic_param_list RBRACKET LPAREN param_list RPAREN LBRACE statement_list RBRACE {
        ASTNode* void_type = create_type_node(AST_TYPE_VOID);
        register_generic_arity($3, GENERIC_KIND_FUNCTION, node_list_count($6));
        $$ = create_public_function_node($3, $9, void_type, $12);
        $$->data.function.generic_params = $6;
    }

    ;

function_definition
    : FN IDENTIFIER LPAREN RPAREN function_return_type LBRACE statement_list RBRACE {
        register_generic_arity($2, GENERIC_KIND_FUNCTION, 0);
        $$ = create_function_node($2, NULL, $5, $7);
        $$->data.function.is_public = 0;
    }
    | FN IDENTIFIER LPAREN param_list RPAREN function_return_type LBRACE statement_list RBRACE {
        register_generic_arity($2, GENERIC_KIND_FUNCTION, 0);
        $$ = create_function_node($2, $4, $6, $8);
        $$->data.function.is_public = 0;
    }
    | FN IDENTIFIER LPAREN RPAREN function_return_type ASSIGN expression {
        register_generic_arity($2, GENERIC_KIND_FUNCTION, 0);
        ASTNode* body = create_program_node_with_yyltype((YYLTYPE*) &@$);
        ASTNode* ret = create_return_node_with_yyltype($7, (YYLTYPE*) &@$);
        add_statement_to_program(body, ret);
        $$ = create_function_node($2, NULL, $5, body);
        $$->data.function.is_public = 0;
    }
    | FN IDENTIFIER LPAREN param_list RPAREN function_return_type ASSIGN expression {
        register_generic_arity($2, GENERIC_KIND_FUNCTION, 0);
        ASTNode* body = create_program_node_with_yyltype((YYLTYPE*) &@$);
        ASTNode* ret = create_return_node_with_yyltype($8, (YYLTYPE*) &@$);
        add_statement_to_program(body, ret);
        $$ = create_function_node($2, $4, $6, body);
        $$->data.function.is_public = 0;
    }
    | FN IDENTIFIER LPAREN RPAREN LBRACE statement_list RBRACE {
        ASTNode* void_type = create_type_node(AST_TYPE_VOID);
        register_generic_arity($2, GENERIC_KIND_FUNCTION, 0);
        $$ = create_function_node($2, NULL, void_type, $6);
        $$->data.function.is_public = 0;
    }
    | FN IDENTIFIER LPAREN param_list RPAREN LBRACE statement_list RBRACE {
        ASTNode* void_type = create_type_node(AST_TYPE_VOID);
        register_generic_arity($2, GENERIC_KIND_FUNCTION, 0);
        $$ = create_function_node($2, $4, void_type, $7);
        $$->data.function.is_public = 0;
    }


    | FN IDENTIFIER COLON LBRACKET generic_param_list RBRACKET LPAREN RPAREN function_return_type LBRACE statement_list RBRACE {
        register_generic_arity($2, GENERIC_KIND_FUNCTION, node_list_count($5));
        $$ = create_function_node($2, NULL, $9, $11);
        $$->data.function.generic_params = $5;
        $$->data.function.is_public = 0;
    }
    | FN IDENTIFIER COLON LBRACKET generic_param_list RBRACKET LPAREN param_list RPAREN function_return_type LBRACE statement_list RBRACE {
        register_generic_arity($2, GENERIC_KIND_FUNCTION, node_list_count($5));
        $$ = create_function_node($2, $8, $10, $12);
        $$->data.function.generic_params = $5;
        $$->data.function.is_public = 0;
    }
    | FN IDENTIFIER COLON LBRACKET generic_param_list RBRACKET LPAREN RPAREN LBRACE statement_list RBRACE {
        ASTNode* void_type = create_type_node(AST_TYPE_VOID);
        register_generic_arity($2, GENERIC_KIND_FUNCTION, node_list_count($5));
        $$ = create_function_node($2, NULL, void_type, $10);
        $$->data.function.generic_params = $5;
        $$->data.function.is_public = 0;
    }
    | FN IDENTIFIER COLON LBRACKET generic_param_list RBRACKET LPAREN param_list RPAREN LBRACE statement_list RBRACE {
        ASTNode* void_type = create_type_node(AST_TYPE_VOID);
        register_generic_arity($2, GENERIC_KIND_FUNCTION, node_list_count($5));
        $$ = create_function_node($2, $8, void_type, $11);
        $$->data.function.generic_params = $5;
        $$->data.function.is_public = 0;
    }

    ;

extern_decl
    : FN IDENTIFIER LPAREN RPAREN function_return_type {
        $$ = create_extern_function_node($2, NULL, $5, NULL);
    }
    | FN IDENTIFIER LPAREN RPAREN {
        $$ = create_extern_function_node($2, NULL, NULL, NULL);
    }
    | FN IDENTIFIER LPAREN param_list RPAREN function_return_type {
        $$ = create_extern_function_node($2, $4, $6, NULL);
    }
    | FN IDENTIFIER LPAREN param_list RPAREN {
        $$ = create_extern_function_node($2, $4, NULL, NULL);
    }
    | FN IDENTIFIER LPAREN param_list COMMA DOTDOTDOT RPAREN function_return_type {
        ASTNode* fn = create_extern_function_node($2, $4, $8, NULL);
        fn->data.function.vararg = 1;
        $$ = fn;
    }
    | FN IDENTIFIER LPAREN param_list COMMA DOTDOTDOT RPAREN {
        ASTNode* fn = create_extern_function_node($2, $4, NULL, NULL);
        fn->data.function.vararg = 1;
        $$ = fn;
    }
    | FN IDENTIFIER LPAREN DOTDOTDOT RPAREN function_return_type {
        ASTNode* fn = create_extern_function_node($2, NULL, $6, NULL);
        fn->data.function.vararg = 1;
        $$ = fn;
    }
    | FN IDENTIFIER LPAREN DOTDOTDOT RPAREN {
        ASTNode* fn = create_extern_function_node($2, NULL, NULL, NULL);
        fn->data.function.vararg = 1;
        $$ = fn;
    }

extern_decl_list
    : /* empty */ { $$ = create_expression_list_node(); }
    | extern_decl { ASTNode* list = create_expression_list_node(); add_expression_to_list(list, $1); $$ = list; }
    | extern_decl_list extern_decl { add_expression_to_list($1, $2); $$ = $1; }

extern_block
    : EXTERN STRING LBRACE extern_decl_list RBRACE {
        ASTNode* prog = create_program_node();
        if ($4 && $4->type == AST_EXPRESSION_LIST) {
            int cnt = $4->data.expression_list.expression_count;
            for (int i = 0; i < cnt; i++) {
                ASTNode* fn = $4->data.expression_list.expressions[i];
                if (fn && fn->type == AST_FUNCTION) {
                    fn->data.function.is_extern = 1;
                    if ($2) {
                        if (fn->data.function.linkage) free(fn->data.function.linkage);
                        fn->data.function.linkage = malloc(strlen($2) + 1);
                        strcpy(fn->data.function.linkage, $2);
                    }
                    add_statement_to_program(prog, fn);
                }
            }
        }
        $$ = prog;
    }

print_statement
    : PRINT expression              { $$ = create_print_node_with_yyltype($2, (YYLTYPE*) &@$); }
    | PRINT LPAREN expression RPAREN { $$ = create_print_node_with_yyltype($3, (YYLTYPE*) &@$); }
    | PRINT LPAREN expression_list RPAREN { $$ = create_print_node_with_yyltype($3, (YYLTYPE*) &@$); }
    ;

assignment_statement
    : factor_unary ASSIGN expression  { $$ = create_assign_node_with_yyltype($1, $3, (YYLTYPE*) &@$); }
    ;

compound_assignment_statement
    : factor_unary PLUS_ASSIGN expression      {
                                               ASTNode* lhs = clone_lvalue($1);
                                               ASTNode* binop = create_binop_node_with_yyltype(OP_ADD, lhs ? lhs : create_num_int_node(0), $3, (YYLTYPE*) &@$);
                                               $$ = create_assign_node_with_yyltype($1, binop, (YYLTYPE*) &@$);
                                             }
    | factor_unary MINUS_ASSIGN expression     {
                                               ASTNode* lhs = clone_lvalue($1);
                                               ASTNode* binop = create_binop_node_with_yyltype(OP_SUB, lhs ? lhs : create_num_int_node(0), $3, (YYLTYPE*) &@$);
                                               $$ = create_assign_node_with_yyltype($1, binop, (YYLTYPE*) &@$);
                                             }
    | factor_unary MULTIPLY_ASSIGN expression  {
                                               ASTNode* lhs = clone_lvalue($1);
                                               ASTNode* binop = create_binop_node_with_yyltype(OP_MUL, lhs ? lhs : create_num_int_node(0), $3, (YYLTYPE*) &@$);
                                               $$ = create_assign_node_with_yyltype($1, binop, (YYLTYPE*) &@$);
                                             }
    | factor_unary DIVIDE_ASSIGN expression    {
                                               ASTNode* lhs = clone_lvalue($1);
                                               ASTNode* binop = create_binop_node_with_yyltype(OP_DIV, lhs ? lhs : create_num_int_node(0), $3, (YYLTYPE*) &@$);
                                               $$ = create_assign_node_with_yyltype($1, binop, (YYLTYPE*) &@$);
                                             }
    | factor_unary MODULO_ASSIGN expression    {
                                               ASTNode* lhs = clone_lvalue($1);
                                               ASTNode* binop = create_binop_node_with_yyltype(OP_MOD, lhs ? lhs : create_num_int_node(0), $3, (YYLTYPE*) &@$);
                                               $$ = create_assign_node_with_yyltype($1, binop, (YYLTYPE*) &@$);
                                             }
    ;

block_statement
    : LBRACE opt_semi_list statement_list opt_semi_list RBRACE { $$ = $3; }
    | LBRACE opt_semi_list RBRACE { $$ = create_program_node_with_yyltype((YYLTYPE*) &@$); }
    | statement                    { $$ = $1; }
    ;

opt_semi_list
    : /* empty */ { }
    | opt_semi_list SEMICOLON { }
    ;

if_statement
    : IF LPAREN expression RPAREN block_statement if_rest {
        /* $6 为 else/body（可能为 NULL），if_rest 构造最终 else_body */
        if ($6) {
            $$ = create_if_node_with_yyltype($3, $5, $6, (YYLTYPE*) &@$);
        } else {
            $$ = create_if_node_with_yyltype($3, $5, NULL, (YYLTYPE*) &@$);
        }
    }
    ;

if_rest
    : /* empty */ { $$ = NULL; }
    | ELSE block_statement { $$ = $2; }
    | ELIF LPAREN expression RPAREN block_statement if_rest {
        /* 将 ELIF 转换为 else: if(expr) then block else rest */
        ASTNode* nested_if = create_if_node($3, $5, $6);
        $$ = nested_if;
    }
    ;

while_statement
    : WHILE LPAREN expression RPAREN block_statement {
        $$ = create_while_node_with_yyltype($3, $5, (YYLTYPE*) &@$);
    }
    ;

for_statement
    : FOR LPAREN identifier SEMICOLON expression DOTDOT expression RPAREN block_statement {
        ASTNode* start = $5;
        ASTNode* end = $7;
        $$ = create_for_node_with_yyltype($3, start, end, $9, (YYLTYPE*) &@$);
    }
    | FOR LPAREN identifier IN expression DOTDOT expression RPAREN block_statement {
        ASTNode* start = $5;
        ASTNode* end = $7;
        $$ = create_for_node_with_yyltype($3, start, end, $9, (YYLTYPE*) &@$);
    }
    | FOR LPAREN identifier IN expression RPAREN block_statement {
        ASTNode* var = $3;
        ASTNode* iterable = $5;
        $$ = create_for_node_with_yyltype(var, iterable, NULL, $7, (YYLTYPE*) &@$);
    }
    | FOR LPAREN identifier SEMICOLON expression RPAREN block_statement {
        ASTNode* var = $3;
        ASTNode* iterable = $5;
        $$ = create_for_node_with_yyltype(var, iterable, NULL, $7, (YYLTYPE*) &@$);
    }
    | FOR LPAREN IDENTIFIER identifier SEMICOLON expression RPAREN block_statement {
        ASTNode* var = $4;
        ASTNode* iterable = $6;
        $$ = create_for_node_with_yyltype(var, iterable, NULL, $8, (YYLTYPE*) &@$);
    }
    ;

expression_list
    : expression                    { 
                                      ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
                                      add_expression_to_list(list, $1);
                                      $$ = list;
                                    }
    | expression COMMA expression   { 
                                      ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
                                      add_expression_to_list(list, $1);
                                      add_expression_to_list(list, $3);
                                      $$ = list;
                                    }
    | expression_list COMMA expression { 
                                      add_expression_to_list($1, $3);
                                      $$ = $1;
                                    }
    ;

logical_expression
    : comparison_expression                 { $$ = $1; }
    | logical_expression AND comparison_expression { $$ = create_binop_node_with_yyltype(OP_AND, $1, $3, (YYLTYPE*) &@$); }
    | logical_expression OR comparison_expression  { $$ = create_binop_node_with_yyltype(OP_OR, $1, $3, (YYLTYPE*) &@$); }
    ;

comparison_expression
    : additive_expression                                  { $$ = $1; }
    | additive_expression EQ additive_expression            { $$ = create_binop_node_with_yyltype(OP_EQ, $1, $3, (YYLTYPE*) &@$); }
    | additive_expression NE additive_expression            { $$ = create_binop_node_with_yyltype(OP_NE, $1, $3, (YYLTYPE*) &@$); }
    | additive_expression LT additive_expression            { $$ = create_binop_node_with_yyltype(OP_LT, $1, $3, (YYLTYPE*) &@$); }
    | additive_expression LE additive_expression            { $$ = create_binop_node_with_yyltype(OP_LE, $1, $3, (YYLTYPE*) &@$); }
    | additive_expression GT additive_expression            { $$ = create_binop_node_with_yyltype(OP_GT, $1, $3, (YYLTYPE*) &@$); }
    | additive_expression GE additive_expression            { $$ = create_binop_node_with_yyltype(OP_GE, $1, $3, (YYLTYPE*) &@$); }
    ;

expression
    : pipeline_expression                   { $$ = $1; }
    ;

pipeline_expression
    : logical_expression                    { $$ = $1; }
    | pipeline_expression PIPE_GT logical_expression { $$ = build_pipe_desugared($1, $3, (YYLTYPE*) &@$); }
    ;

additive_expression
    : term                                  { $$ = $1; }
    | additive_expression PLUS term         { $$ = create_binop_node_with_yyltype(OP_ADD, $1, $3, (YYLTYPE*) &@$); }
    | additive_expression MINUS term        { $$ = create_binop_node_with_yyltype(OP_SUB, $1, $3, (YYLTYPE*) &@$); }
    ;

term
    : factor                        { $$ = $1; }
    | term MULTIPLY factor          { $$ = create_binop_node_with_yyltype(OP_MUL, $1, $3, (YYLTYPE*) &@$); }
    | term DIVIDE factor            { $$ = create_binop_node_with_yyltype(OP_DIV, $1, $3, (YYLTYPE*) &@$); }
    | term MODULO factor            { $$ = create_binop_node_with_yyltype(OP_MOD, $1, $3, (YYLTYPE*) &@$); }
    ;

factor
    : power                         { $$ = $1; }
    ;

power
    : factor_unary                  { $$ = $1; }
    | factor_unary POWER factor     { $$ = create_binop_node_with_yyltype(OP_POW, $1, $3, (YYLTYPE*) &@$); }
    ;

factor_unary
    : IDENTIFIER LPAREN RPAREN      { 
        ASTNode* id = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$); 
        $$ = create_call_node_with_yyltype(id, NULL, (YYLTYPE*) &@$); 
    }
    | IDENTIFIER LPAREN expression COMMA FN LPAREN IDENTIFIER COLON type RPAREN COLON type LBRACE statement_list RBRACE RPAREN {
        static int lambda_counter = 0;
        char name_buf[64];
        snprintf(name_buf, sizeof(name_buf), "__lambda_%d", lambda_counter++);

        ASTNode* params = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        ASTNode* id_node = create_identifier_node_with_yyltype($7, (YYLTYPE*) &@$);
        ASTNode* annotated_param = create_assign_node_with_yyltype(id_node, $9, (YYLTYPE*) &@$);
        annotated_param->data.assign.is_declaration = 2;
        add_expression_to_list(params, annotated_param);

        ASTNode* lambda_fn = create_function_node(name_buf, params, $12, $14);
        lambda_fn->data.function.is_public = 0;

        ASTNode* args = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        add_expression_to_list(args, $3);
        add_expression_to_list(args, lambda_fn);

        ASTNode* id = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$);
        $$ = create_call_node_with_yyltype(id, args, (YYLTYPE*) &@$);
    }
    | IDENTIFIER LPAREN expression_list RPAREN { 
        ASTNode* id = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$); 
        $$ = create_call_node_with_yyltype(id, $3, (YYLTYPE*) &@$); 
    }
    | IDENTIFIER LBRACKET expression RBRACKET {
        ASTNode* id = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$);
        $$ = create_index_node_with_yyltype(id, $3, (YYLTYPE*) &@$);
    }
    | IDENTIFIER LBRACKET expression DOTDOT expression RBRACKET {
        ASTNode* id = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$);
        $$ = build_string_slice_desugared(id, $3, $5, (YYLTYPE*) &@$);
    }
    | IDENTIFIER LBRACE RBRACE { ASTNode* type_id = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$); ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$); $$ = create_struct_literal_node_with_yyltype(type_id, list, (YYLTYPE*) &@$); }
    | IDENTIFIER LBRACE struct_init_fields RBRACE { ASTNode* type_id = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$); $$ = create_struct_literal_node_with_yyltype(type_id, $3, (YYLTYPE*) &@$); }

    | IDENTIFIER COLON LBRACKET generic_type_args RBRACKET LPAREN RPAREN {
        check_generic_arity_usage($1, GENERIC_KIND_FUNCTION, $4, (YYLTYPE*) &@$);
        ASTNode* id = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$);
        $$ = create_call_node_with_yyltype(id, NULL, (YYLTYPE*) &@$);
        $$->data.call.type_args = $4;
    }
    | IDENTIFIER COLON LBRACKET generic_type_args RBRACKET LPAREN expression_list RPAREN {
        check_generic_arity_usage($1, GENERIC_KIND_FUNCTION, $4, (YYLTYPE*) &@$);
        ASTNode* id = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$);
        $$ = create_call_node_with_yyltype(id, $7, (YYLTYPE*) &@$);
        $$->data.call.type_args = $4;
    }
    | IDENTIFIER COLON LBRACKET generic_type_args RBRACKET LBRACE RBRACE {
        check_generic_arity_usage($1, GENERIC_KIND_STRUCT, $4, (YYLTYPE*) &@$);
        ASTNode* type_id = create_type_app_node_with_yyltype(
            create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$), $4, (YYLTYPE*) &@$);
        ASTNode* list = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        $$ = create_struct_literal_node_with_yyltype(type_id, list, (YYLTYPE*) &@$);
    }
    | IDENTIFIER COLON LBRACKET generic_type_args RBRACKET LBRACE struct_init_fields RBRACE {
        check_generic_arity_usage($1, GENERIC_KIND_STRUCT, $4, (YYLTYPE*) &@$);
        ASTNode* type_id = create_type_app_node_with_yyltype(
            create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$), $4, (YYLTYPE*) &@$);
        $$ = create_struct_literal_node_with_yyltype(type_id, $7, (YYLTYPE*) &@$);
    }

    | IDENTIFIER {
        $$ = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$);
    }
    | FN LPAREN IDENTIFIER COLON type RPAREN function_return_type block_statement {
        static int lambda_counter = 0;
        char name_buf[64];
        snprintf(name_buf, sizeof(name_buf), "__lambda_%d", lambda_counter++);
        ASTNode* params = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        ASTNode* id_node = create_identifier_node_with_yyltype($3, (YYLTYPE*) &@$);
        ASTNode* annotated_param = create_assign_node_with_yyltype(id_node, $5, (YYLTYPE*) &@$);
        annotated_param->data.assign.is_declaration = 2;
        add_expression_to_list(params, annotated_param);
        ASTNode* fn = create_function_node(name_buf, params, $7, $8);
        fn->data.function.is_public = 0;
        $$ = fn;
    }
    | FN LPAREN param_list RPAREN function_return_type block_statement {
        static int lambda_counter = 0;
        char name_buf[64];
        snprintf(name_buf, sizeof(name_buf), "__lambda_%d", lambda_counter++);
        ASTNode* fn = create_function_node(name_buf, $3, $5, $6);
        fn->data.function.is_public = 0;
        $$ = fn;
    }
    | FN LPAREN RPAREN function_return_type block_statement {
        static int lambda_counter = 0;
        char name_buf[64];
        snprintf(name_buf, sizeof(name_buf), "__lambda_%d", lambda_counter++);
        ASTNode* fn = create_function_node(name_buf, NULL, $4, $5);
        fn->data.function.is_public = 0;
        $$ = fn;
    }
    | literal                       { $$ = $1; }
    | input_expression              { $$ = $1; }
    | IF LPAREN expression RPAREN block_statement if_rest {
        if ($6) {
            $$ = create_if_node_with_yyltype($3, $5, $6, (YYLTYPE*) &@$);
        } else {
            $$ = create_if_node_with_yyltype($3, $5, NULL, (YYLTYPE*) &@$);
        }
    }
    | MATCH match_target LBRACE match_arms RBRACE {
        $$ = build_match_desugared($2, $4);
    }
    | PLUS factor_unary             { $$ = create_unaryop_node_with_yyltype(OP_PLUS, $2, (YYLTYPE*) &@$); }
    | MINUS factor_unary            { $$ = create_unaryop_node_with_yyltype(OP_MINUS, $2, (YYLTYPE*) &@$); }
    | MULTIPLY factor_unary         { $$ = create_unaryop_node_with_yyltype(OP_DEREF, $2, (YYLTYPE*) &@$); }
    | REF_KW factor_unary           { $$ = create_unaryop_node_with_yyltype(OP_ADDRESS, $2, (YYLTYPE*) &@$); }
    | MUT REF_KW factor_unary       {
        $$ = create_unaryop_node_with_yyltype(OP_ADDRESS, $3, (YYLTYPE*) &@$);
        $$->mutability = MUTABILITY_MUTABLE;
    }
    | AMPERSAND factor_unary        { $$ = create_unaryop_node_with_yyltype(OP_ADDRESS, $2, (YYLTYPE*) &@$); }
    | MUT AMPERSAND factor_unary   {
        $$ = create_unaryop_node_with_yyltype(OP_ADDRESS, $3, (YYLTYPE*) &@$);
        $$->mutability = MUTABILITY_MUTABLE;
    }
    | AT factor_unary               { $$ = create_unaryop_node_with_yyltype(OP_DEREF, $2, (YYLTYPE*) &@$); }
    | BANG factor_unary             { $$ = create_unaryop_node_with_yyltype(OP_NOT, $2, (YYLTYPE*) &@$); }
    | LPAREN expression RPAREN      { $$ = $2; }
    | LPAREN RPAREN                 { $$ = create_nil_node_with_yyltype((YYLTYPE*) &@$); }
    | LPAREN expression COMMA expression_list RPAREN {
        ASTNode* tuple = create_expression_list_node_with_yyltype((YYLTYPE*) &@$);
        add_expression_to_list(tuple, $2);
        if ($4 && $4->type == AST_EXPRESSION_LIST) {
            for (int i = 0; i < $4->data.expression_list.expression_count; i++) {
                add_expression_to_list(tuple, $4->data.expression_list.expressions[i]);
            }
        }
        $$ = tuple;
    }
    | factor_unary DOT IDENTIFIER { $$ = create_member_access_node_with_yyltype($1, create_identifier_node_with_yyltype($3, (YYLTYPE*) &@$), (YYLTYPE*) &@$); }
    | factor_unary DOT NUMBER_INT {
        char idx_name[32];
        snprintf(idx_name, sizeof(idx_name), "%lld", $3);
        $$ = create_member_access_node_with_yyltype($1, create_identifier_node_with_yyltype(idx_name, (YYLTYPE*) &@$), (YYLTYPE*) &@$);
    }
    | factor_unary DOT IDENTIFIER LPAREN RPAREN { 
        ASTNode* method = create_identifier_node_with_yyltype($3, (YYLTYPE*) &@$);
        ASTNode* mem = create_member_access_node_with_yyltype($1, method, (YYLTYPE*) &@$);
        $$ = create_call_node_with_yyltype(mem, NULL, (YYLTYPE*) &@$);
    }
    | factor_unary DOT IDENTIFIER LPAREN expression_list RPAREN { 
        ASTNode* method = create_identifier_node_with_yyltype($3, (YYLTYPE*) &@$);
        ASTNode* mem = create_member_access_node_with_yyltype($1, method, (YYLTYPE*) &@$);
        $$ = create_call_node_with_yyltype(mem, $5, (YYLTYPE*) &@$);
    }
    | factor_unary LBRACKET expression DOTDOT expression RBRACKET { $$ = build_string_slice_desugared($1, $3, $5, (YYLTYPE*) &@$); }
    | factor_unary LBRACKET expression RBRACKET { $$ = create_index_node_with_yyltype($1, $3, (YYLTYPE*) &@$); }
    ;

literal
    : NUMBER_INT                    { $$ = create_num_int_node_with_yyltype($1, (YYLTYPE*) &@$); }
    | BOOL_LITERAL                  { $$ = create_bool_node_with_yyltype($1, (YYLTYPE*) &@$); }
    | NUMBER_FLOAT                  { $$ = create_num_float_node_with_yyltype($1, (YYLTYPE*) &@$); }
    | STRING                        { $$ = create_string_node_with_yyltype($1, (YYLTYPE*) &@$); }
    | CHAR_LITERAL                  { $$ = create_char_node_with_yyltype((char)$1, (YYLTYPE*) &@$); }
    | NIL                           { $$ = create_nil_node_with_yyltype((YYLTYPE*) &@$); }
    | LBRACKET RBRACKET             { $$ = create_expression_list_node_with_yyltype((YYLTYPE*) &@$); }
    | LBRACKET expression_list RBRACKET { $$ = $2; }
    ;

identifier
    : IDENTIFIER                    { $$ = create_identifier_node_with_yyltype($1, (YYLTYPE*) &@$); }
    ;

%%

void yyerror(const char* s) {
    const char* filename = current_input_filename ? current_input_filename : "unknown";
    int col = (yylloc.first_column > 0) ? yylloc.first_column : 1;
    report_syntax_error_with_location_column(s, filename, yylineno, col);
}
