#define _POSIX_C_SOURCE 200809L

#include "macro.h"

#include <ctype.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define VIX_MACRO_MAX_EXPANSION_DEPTH 64

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} TextBuffer;

typedef struct {
    size_t start;
    size_t end;
} Slice;

typedef struct {
    char *name;
    char *fragment;
    int variadic;
} MacroParam;

typedef struct {
    char *name;
    char opener;
    char closer;
    MacroParam *params;
    size_t param_count;
    char *body;
    size_t declaration_start;
    size_t declaration_end;
    int line;
} MacroDefinition;

typedef struct {
    MacroDefinition *items;
    size_t count;
    size_t cap;
    const char *filename;
    int failed;
} MacroTable;

typedef struct {
    Slice *items;
    size_t count;
    size_t cap;
} SliceList;

typedef struct {
    const MacroParam *param;
    char **values;
    size_t value_count;
} MacroBinding;

static int buffer_reserve(TextBuffer *buffer, size_t extra) {
    if (extra > (size_t)-1 - buffer->len - 1) return 0;
    size_t needed = buffer->len + extra + 1;
    if (needed <= buffer->cap) return 1;

    size_t cap = buffer->cap ? buffer->cap : 256;
    while (cap < needed) {
        if (cap > (size_t)-1 / 2) {
            cap = needed;
            break;
        }
        cap *= 2;
    }
    char *resized = (char *)realloc(buffer->data, cap);
    if (!resized) return 0;
    buffer->data = resized;
    buffer->cap = cap;
    return 1;
}

static int buffer_append_n(TextBuffer *buffer, const char *text, size_t len) {
    if (!buffer_reserve(buffer, len)) return 0;
    if (len) memcpy(buffer->data + buffer->len, text, len);
    buffer->len += len;
    buffer->data[buffer->len] = '\0';
    return 1;
}

static int buffer_append(TextBuffer *buffer, const char *text) {
    return buffer_append_n(buffer, text, strlen(text));
}

static int buffer_append_char(TextBuffer *buffer, char c) {
    return buffer_append_n(buffer, &c, 1);
}

static char *buffer_take(TextBuffer *buffer) {
    if (!buffer->data) {
        buffer->data = (char *)malloc(1);
        if (!buffer->data) return NULL;
        buffer->data[0] = '\0';
    }
    char *result = buffer->data;
    buffer->data = NULL;
    buffer->len = 0;
    buffer->cap = 0;
    return result;
}

static char *copy_range(const char *source, size_t start, size_t end) {
    if (end < start) return NULL;
    size_t len = end - start;
    char *copy = (char *)malloc(len + 1);
    if (!copy) return NULL;
    if (len) memcpy(copy, source + start, len);
    copy[len] = '\0';
    return copy;
}

static void trim_range(const char *source, size_t *start, size_t *end) {
    while (*start < *end && isspace((unsigned char)source[*start])) (*start)++;
    while (*end > *start && isspace((unsigned char)source[*end - 1])) (*end)--;
}

static int is_ident_start(char c) {
    return isalpha((unsigned char)c) || c == '_';
}

static int is_ident_continue(char c) {
    return isalnum((unsigned char)c) || c == '_';
}

static size_t scan_identifier(const char *source, size_t len, size_t pos) {
    if (pos >= len || !is_ident_start(source[pos])) return pos;
    pos++;
    while (pos < len && is_ident_continue(source[pos])) pos++;
    return pos;
}

static int word_at(const char *source, size_t len, size_t pos, const char *word) {
    size_t word_len = strlen(word);
    if (pos + word_len > len || memcmp(source + pos, word, word_len) != 0) return 0;
    if (pos > 0 && is_ident_continue(source[pos - 1])) return 0;
    if (pos + word_len < len && is_ident_continue(source[pos + word_len])) return 0;
    return 1;
}

static int line_for_offset(const char *source, size_t offset) {
    int line = 1;
    for (size_t i = 0; i < offset; i++) {
        if (source[i] == '\n') line++;
    }
    return line;
}

static int column_for_offset(const char *source, size_t offset) {
    size_t line_start = offset;
    while (line_start > 0 && source[line_start - 1] != '\n') line_start--;
    return (int)(offset - line_start + 1);
}

static void macro_error(MacroTable *table, const char *source, size_t offset,
                        const char *format, ...) {
    if (table->failed) return;
    table->failed = 1;

    fprintf(stderr, "error [MacroError]: ");
    va_list args;
    va_start(args, format);
    vfprintf(stderr, format, args);
    va_end(args);
    fprintf(stderr, "\n  --> %s:%d:%d\n",
            table->filename ? table->filename : "<input>",
            line_for_offset(source, offset), column_for_offset(source, offset));
}

static size_t skip_quoted(const char *source, size_t len, size_t pos) {
    char quote = source[pos++];
    while (pos < len) {
        if (source[pos] == '\\' && pos + 1 < len) {
            pos += 2;
        } else if (source[pos++] == quote) {
            break;
        }
    }
    return pos;
}

static size_t skip_line_comment(const char *source, size_t len, size_t pos) {
    while (pos < len && source[pos] != '\n') pos++;
    return pos;
}

static size_t skip_block_comment(const char *source, size_t len, size_t pos) {
    pos += 2;
    while (pos + 1 < len) {
        if (source[pos] == '*' && source[pos + 1] == '/') return pos + 2;
        pos++;
    }
    return len;
}

static size_t skip_trivia(const char *source, size_t len, size_t pos) {
    for (;;) {
        while (pos < len && isspace((unsigned char)source[pos])) pos++;
        if (pos + 1 < len && source[pos] == '/' && source[pos + 1] == '/') {
            pos = skip_line_comment(source, len, pos + 2);
        } else if (pos < len && source[pos] == '#') {
            pos = skip_line_comment(source, len, pos + 1);
        } else if (pos + 1 < len && source[pos] == '/' && source[pos + 1] == '*') {
            pos = skip_block_comment(source, len, pos);
        } else {
            return pos;
        }
    }
}

static char matching_closer(char opener) {
    switch (opener) {
        case '(': return ')';
        case '[': return ']';
        case '{': return '}';
        default: return '\0';
    }
}

static int find_matching(const char *source, size_t len, size_t open_pos,
                         size_t *close_pos) {
    char stack[256];
    size_t depth = 0;
    char first_closer = matching_closer(source[open_pos]);
    if (!first_closer) return 0;
    stack[depth++] = first_closer;

    size_t pos = open_pos + 1;
    while (pos < len) {
        char c = source[pos];
        if (c == '"' || c == '\'') {
            pos = skip_quoted(source, len, pos);
            continue;
        }
        if (c == '#') {
            pos = skip_line_comment(source, len, pos + 1);
            continue;
        }
        if (c == '/' && pos + 1 < len && source[pos + 1] == '/') {
            pos = skip_line_comment(source, len, pos + 2);
            continue;
        }
        if (c == '/' && pos + 1 < len && source[pos + 1] == '*') {
            pos = skip_block_comment(source, len, pos);
            continue;
        }

        char closer = matching_closer(c);
        if (closer) {
            if (depth >= sizeof(stack)) return 0;
            stack[depth++] = closer;
            pos++;
            continue;
        }
        if (depth > 0 && c == stack[depth - 1]) {
            depth--;
            if (depth == 0) {
                *close_pos = pos;
                return 1;
            }
        }
        pos++;
    }
    return 0;
}

static int slice_list_add(SliceList *list, size_t start, size_t end) {
    if (list->count == list->cap) {
        size_t cap = list->cap ? list->cap * 2 : 8;
        Slice *items = (Slice *)realloc(list->items, cap * sizeof(Slice));
        if (!items) return 0;
        list->items = items;
        list->cap = cap;
    }
    list->items[list->count++] = (Slice){start, end};
    return 1;
}

static int split_top_level(const char *source, size_t start, size_t end,
                           SliceList *parts) {
    size_t part_start = start;
    size_t pos = start;
    char stack[256];
    size_t depth = 0;

    while (pos < end) {
        char c = source[pos];
        if (c == '"' || c == '\'') {
            pos = skip_quoted(source, end, pos);
            continue;
        }
        if (c == '#') {
            pos = skip_line_comment(source, end, pos + 1);
            continue;
        }
        if (c == '/' && pos + 1 < end && source[pos + 1] == '/') {
            pos = skip_line_comment(source, end, pos + 2);
            continue;
        }
        if (c == '/' && pos + 1 < end && source[pos + 1] == '*') {
            pos = skip_block_comment(source, end, pos);
            continue;
        }
        char closer = matching_closer(c);
        if (closer) {
            if (depth >= sizeof(stack)) return 0;
            stack[depth++] = closer;
        } else if (depth && c == stack[depth - 1]) {
            depth--;
        } else if (c == ',' && depth == 0) {
            if (!slice_list_add(parts, part_start, pos)) return 0;
            part_start = pos + 1;
        }
        pos++;
    }
    if (depth != 0) return 0;
    if (part_start < end || parts->count > 0) {
        if (!slice_list_add(parts, part_start, end)) return 0;
    }
    return 1;
}

static void free_definition(MacroDefinition *definition) {
    free(definition->name);
    free(definition->body);
    for (size_t i = 0; i < definition->param_count; i++) {
        free(definition->params[i].name);
        free(definition->params[i].fragment);
    }
    free(definition->params);
}

static void free_table(MacroTable *table) {
    for (size_t i = 0; i < table->count; i++) free_definition(&table->items[i]);
    free(table->items);
}

static MacroDefinition *find_macro(MacroTable *table, const char *name,
                                   size_t name_len) {
    for (size_t i = 0; i < table->count; i++) {
        if (strlen(table->items[i].name) == name_len &&
            memcmp(table->items[i].name, name, name_len) == 0) {
            return &table->items[i];
        }
    }
    return NULL;
}

static int table_add(MacroTable *table, MacroDefinition *definition) {
    if (table->count == table->cap) {
        size_t cap = table->cap ? table->cap * 2 : 8;
        MacroDefinition *items = (MacroDefinition *)realloc(
            table->items, cap * sizeof(MacroDefinition));
        if (!items) return 0;
        table->items = items;
        table->cap = cap;
    }
    table->items[table->count++] = *definition;
    memset(definition, 0, sizeof(*definition));
    return 1;
}

static int definition_add_param(MacroDefinition *definition, MacroParam *param) {
    size_t count = definition->param_count + 1;
    MacroParam *params = (MacroParam *)realloc(
        definition->params, count * sizeof(MacroParam));
    if (!params) return 0;
    definition->params = params;
    definition->params[definition->param_count++] = *param;
    memset(param, 0, sizeof(*param));
    return 1;
}

static int valid_fragment_name(const char *name) {
    return strcmp(name, "ident") == 0 || strcmp(name, "expr") == 0 ||
           strcmp(name, "stmt") == 0 || strcmp(name, "type") == 0 ||
           strcmp(name, "pat") == 0 || strcmp(name, "tt") == 0;
}

static int parse_parameters(MacroTable *table, const char *source,
                            size_t start, size_t end,
                            MacroDefinition *definition) {
    SliceList parts = {0};
    if (!split_top_level(source, start, end, &parts)) {
        macro_error(table, source, start, "invalid macro parameter list");
        free(parts.items);
        return 0;
    }

    for (size_t i = 0; i < parts.count; i++) {
        size_t part_start = parts.items[i].start;
        size_t part_end = parts.items[i].end;
        trim_range(source, &part_start, &part_end);
        if (part_start == part_end) {
            macro_error(table, source, part_start, "empty macro parameter");
            free(parts.items);
            return 0;
        }

        size_t pos = part_start;
        if (source[pos] == '$') pos = skip_trivia(source, part_end, pos + 1);
        size_t name_end = scan_identifier(source, part_end, pos);
        if (name_end == pos) {
            macro_error(table, source, pos, "expected a macro parameter name");
            free(parts.items);
            return 0;
        }
        char *name = copy_range(source, pos, name_end);
        pos = skip_trivia(source, part_end, name_end);
        if (pos >= part_end || source[pos] != ':') {
            macro_error(table, source, pos, "expected ':' after macro parameter '%s'", name);
            free(name);
            free(parts.items);
            return 0;
        }
        pos = skip_trivia(source, part_end, pos + 1);
        size_t fragment_end = scan_identifier(source, part_end, pos);
        if (fragment_end == pos) {
            macro_error(table, source, pos, "expected a fragment type for macro parameter '%s'", name);
            free(name);
            free(parts.items);
            return 0;
        }
        char *fragment = copy_range(source, pos, fragment_end);
        pos = skip_trivia(source, part_end, fragment_end);
        int variadic = 0;
        if (pos < part_end && source[pos] == '*') {
            variadic = 1;
            pos = skip_trivia(source, part_end, pos + 1);
        }
        if (pos != part_end) {
            macro_error(table, source, pos, "unexpected text in macro parameter '%s'", name);
            free(name);
            free(fragment);
            free(parts.items);
            return 0;
        }
        if (!valid_fragment_name(fragment)) {
            macro_error(table, source, part_start, "unknown macro fragment type '%s'", fragment);
            free(name);
            free(fragment);
            free(parts.items);
            return 0;
        }
        if (variadic && i + 1 != parts.count) {
            macro_error(table, source, part_start, "variadic macro parameter must be last");
            free(name);
            free(fragment);
            free(parts.items);
            return 0;
        }
        for (size_t previous = 0; previous < definition->param_count; previous++) {
            if (strcmp(definition->params[previous].name, name) == 0) {
                macro_error(table, source, part_start, "duplicate macro parameter '%s'", name);
                free(name);
                free(fragment);
                free(parts.items);
                return 0;
            }
        }

        MacroParam param = {name, fragment, variadic};
        if (!definition_add_param(definition, &param)) {
            free(name);
            free(fragment);
            free(parts.items);
            return 0;
        }
    }

    free(parts.items);
    return 1;
}

static int parse_definition(MacroTable *table, const char *source, size_t len,
                            size_t start, size_t *end_out) {
    MacroDefinition definition = {0};
    definition.declaration_start = start;
    definition.line = line_for_offset(source, start);

    size_t pos = skip_trivia(source, len, start + strlen("macro"));
    if (pos >= len || source[pos] != '$') {
        macro_error(table, source, pos, "expected '$' before macro name");
        return 0;
    }
    pos++;
    size_t name_start = pos;
    size_t name_end = scan_identifier(source, len, pos);
    if (name_end == name_start) {
        macro_error(table, source, pos, "expected a macro name");
        return 0;
    }
    definition.name = copy_range(source, name_start, name_end);
    if (!definition.name) return 0;
    if (find_macro(table, definition.name, strlen(definition.name))) {
        macro_error(table, source, name_start, "duplicate macro definition '$%s'", definition.name);
        free_definition(&definition);
        return 0;
    }

    pos = skip_trivia(source, len, name_end);
    if (pos >= len || (source[pos] != '(' && source[pos] != '[')) {
        macro_error(table, source, pos, "expected '(' or '[' after macro name '$%s'", definition.name);
        free_definition(&definition);
        return 0;
    }
    definition.opener = source[pos];
    definition.closer = matching_closer(definition.opener);
    size_t header_close = 0;
    if (!find_matching(source, len, pos, &header_close)) {
        macro_error(table, source, pos, "unclosed parameter list for macro '$%s'", definition.name);
        free_definition(&definition);
        return 0;
    }
    if (!parse_parameters(table, source, pos + 1, header_close, &definition)) {
        free_definition(&definition);
        return 0;
    }

    pos = skip_trivia(source, len, header_close + 1);
    if (pos >= len || source[pos] != '{') {
        macro_error(table, source, pos, "expected '{' before body of macro '$%s'", definition.name);
        free_definition(&definition);
        return 0;
    }
    size_t body_close = 0;
    if (!find_matching(source, len, pos, &body_close)) {
        macro_error(table, source, pos, "unclosed body for macro '$%s'", definition.name);
        free_definition(&definition);
        return 0;
    }
    definition.body = copy_range(source, pos + 1, body_close);
    if (!definition.body) {
        free_definition(&definition);
        return 0;
    }
    definition.declaration_end = body_close + 1;
    *end_out = definition.declaration_end;
    if (!table_add(table, &definition)) {
        free_definition(&definition);
        return 0;
    }
    return 1;
}

static int collect_definitions(MacroTable *table, const char *source, size_t len) {
    size_t pos = 0;
    int brace_depth = 0;
    while (pos < len) {
        char c = source[pos];
        if (c == '"' || c == '\'') {
            pos = skip_quoted(source, len, pos);
        } else if (c == '#') {
            pos = skip_line_comment(source, len, pos + 1);
        } else if (c == '/' && pos + 1 < len && source[pos + 1] == '/') {
            pos = skip_line_comment(source, len, pos + 2);
        } else if (c == '/' && pos + 1 < len && source[pos + 1] == '*') {
            pos = skip_block_comment(source, len, pos);
        } else if (c == '{') {
            brace_depth++;
            pos++;
        } else if (c == '}') {
            if (brace_depth > 0) brace_depth--;
            pos++;
        } else if (brace_depth == 0 && word_at(source, len, pos, "macro")) {
            size_t definition_end = pos;
            if (!parse_definition(table, source, len, pos, &definition_end)) return 0;
            pos = definition_end;
        } else {
            pos++;
        }
    }
    return !table->failed;
}

static int in_definition(const MacroTable *table, size_t pos, size_t *end) {
    for (size_t i = 0; i < table->count; i++) {
        if (pos >= table->items[i].declaration_start &&
            pos < table->items[i].declaration_end) {
            if (end) *end = table->items[i].declaration_end;
            return 1;
        }
    }
    return 0;
}

static const MacroBinding *find_binding(const MacroBinding *bindings,
                                        size_t binding_count,
                                        const char *name, size_t name_len) {
    for (size_t i = 0; i < binding_count; i++) {
        if (strlen(bindings[i].param->name) == name_len &&
            memcmp(bindings[i].param->name, name, name_len) == 0) {
            return &bindings[i];
        }
    }
    return NULL;
}

static int is_single_identifier(const char *value) {
    size_t len = strlen(value);
    size_t start = 0;
    size_t end = len;
    trim_range(value, &start, &end);
    if (start == end || !is_ident_start(value[start])) return 0;
    size_t ident_end = scan_identifier(value, end, start);
    return ident_end == end;
}

static void free_bindings(MacroBinding *bindings, size_t count) {
    if (!bindings) return;
    for (size_t i = 0; i < count; i++) {
        if (bindings[i].values) {
            for (size_t j = 0; j < bindings[i].value_count; j++) {
                free(bindings[i].values[j]);
            }
        }
        free(bindings[i].values);
    }
    free(bindings);
}

static int bind_arguments(MacroTable *table, const char *whole_source,
                          size_t invocation_offset,
                          const MacroDefinition *definition,
                          const char *args_source, size_t args_len,
                          MacroBinding **bindings_out) {
    SliceList args = {0};
    if (!split_top_level(args_source, 0, args_len, &args)) {
        macro_error(table, whole_source, invocation_offset,
                    "invalid arguments for macro '$%s'", definition->name);
        free(args.items);
        return 0;
    }
    if (args.count == 1) {
        size_t start = args.items[0].start;
        size_t end = args.items[0].end;
        trim_range(args_source, &start, &end);
        if (start == end) args.count = 0;
    }

    size_t fixed_count = definition->param_count;
    int variadic = 0;
    if (fixed_count && definition->params[fixed_count - 1].variadic) {
        fixed_count--;
        variadic = 1;
    }
    if ((!variadic && args.count != fixed_count) ||
        (variadic && args.count < fixed_count)) {
        if (variadic) {
            macro_error(table, whole_source, invocation_offset,
                        "macro '$%s' expects at least %zu argument(s), got %zu",
                        definition->name, fixed_count, args.count);
        } else {
            macro_error(table, whole_source, invocation_offset,
                        "macro '$%s' expects %zu argument(s), got %zu",
                        definition->name, fixed_count, args.count);
        }
        free(args.items);
        return 0;
    }

    MacroBinding *bindings = (MacroBinding *)calloc(
        definition->param_count ? definition->param_count : 1,
        sizeof(MacroBinding));
    if (!bindings) {
        free(args.items);
        return 0;
    }
    for (size_t i = 0; i < definition->param_count; i++) {
        bindings[i].param = &definition->params[i];
        size_t value_count = definition->params[i].variadic ? args.count - fixed_count : 1;
        bindings[i].value_count = value_count;
        if (value_count) {
            bindings[i].values = (char **)calloc(value_count, sizeof(char *));
            if (!bindings[i].values) {
                free_bindings(bindings, definition->param_count);
                free(args.items);
                return 0;
            }
        }
        for (size_t j = 0; j < value_count; j++) {
            size_t arg_index = definition->params[i].variadic ? fixed_count + j : i;
            if (!args.items || arg_index >= args.count) {
                macro_error(table, whole_source, invocation_offset,
                            "invalid argument binding for macro '$%s'",
                            definition->name);
                free_bindings(bindings, definition->param_count);
                free(args.items);
                return 0;
            }
            size_t start = args.items[arg_index].start;
            size_t end = args.items[arg_index].end;
            trim_range(args_source, &start, &end);
            bindings[i].values[j] = copy_range(args_source, start, end);
            if (!bindings[i].values[j]) {
                free_bindings(bindings, definition->param_count);
                free(args.items);
                return 0;
            }
            if (strcmp(definition->params[i].fragment, "ident") == 0 &&
                !is_single_identifier(bindings[i].values[j])) {
                macro_error(table, whole_source, invocation_offset,
                            "argument %zu of macro '$%s' must be an identifier",
                            arg_index + 1, definition->name);
                free_bindings(bindings, definition->param_count);
                free(args.items);
                return 0;
            }
            if (bindings[i].values[j][0] == '\0') {
                macro_error(table, whole_source, invocation_offset,
                            "argument %zu of macro '$%s' is empty",
                            arg_index + 1, definition->name);
                free_bindings(bindings, definition->param_count);
                free(args.items);
                return 0;
            }
        }
    }

    free(args.items);
    *bindings_out = bindings;
    return 1;
}

static int render_range(MacroTable *table, const char *whole_source,
                        size_t invocation_offset,
                        const char *template_source, size_t start, size_t end,
                        const MacroBinding *bindings, size_t binding_count,
                        const MacroBinding *repeated_binding, size_t repeat_index,
                        TextBuffer *output);

static const MacroBinding *repetition_binding(MacroTable *table,
                                               const char *whole_source,
                                               size_t invocation_offset,
                                               const char *source,
                                               size_t start, size_t end,
                                               const MacroBinding *bindings,
                                               size_t binding_count) {
    const MacroBinding *found = NULL;
    size_t pos = start;
    while (pos < end) {
        if (source[pos] == '"' || source[pos] == '\'') {
            pos = skip_quoted(source, end, pos);
            continue;
        }
        if (source[pos] == '$') {
            pos++;
            if (pos < end && source[pos] == '$') pos++;
        }
        size_t name_start = pos;
        size_t name_end = scan_identifier(source, end, pos);
        if (name_end > name_start) {
            const MacroBinding *binding = find_binding(
                bindings, binding_count, source + name_start, name_end - name_start);
            if (binding && binding->param->variadic) {
                if (found && found != binding) {
                    macro_error(table, whole_source, invocation_offset,
                                "a repetition may use only one variadic parameter");
                    return NULL;
                }
                found = binding;
            }
            pos = name_end;
        } else {
            pos++;
        }
    }
    if (!found) {
        macro_error(table, whole_source, invocation_offset,
                    "macro repetition does not reference a variadic parameter");
    }
    return found;
}

static int render_binding(TextBuffer *output, const MacroBinding *binding,
                          const MacroBinding *repeated_binding,
                          size_t repeat_index) {
    if (binding->param->variadic) {
        if (binding == repeated_binding) {
            if (repeat_index >= binding->value_count) return 0;
            return buffer_append(output, binding->values[repeat_index]);
        }
        for (size_t i = 0; i < binding->value_count; i++) {
            if (i && !buffer_append(output, ", ")) return 0;
            if (!buffer_append(output, binding->values[i])) return 0;
        }
        return 1;
    }
    return binding->value_count == 1 && buffer_append(output, binding->values[0]);
}

static int render_range(MacroTable *table, const char *whole_source,
                        size_t invocation_offset,
                        const char *template_source, size_t start, size_t end,
                        const MacroBinding *bindings, size_t binding_count,
                        const MacroBinding *repeated_binding, size_t repeat_index,
                        TextBuffer *output) {
    size_t pos = start;
    while (pos < end) {
        char c = template_source[pos];
        if (c == '"' || c == '\'') {
            size_t quoted_end = skip_quoted(template_source, end, pos);
            if (!buffer_append_n(output, template_source + pos, quoted_end - pos)) return 0;
            pos = quoted_end;
            continue;
        }
        if (c == '#') {
            size_t comment_end = skip_line_comment(template_source, end, pos + 1);
            if (!buffer_append_n(output, template_source + pos, comment_end - pos)) return 0;
            pos = comment_end;
            continue;
        }
        if (c == '/' && pos + 1 < end && template_source[pos + 1] == '/') {
            size_t comment_end = skip_line_comment(template_source, end, pos + 2);
            if (!buffer_append_n(output, template_source + pos, comment_end - pos)) return 0;
            pos = comment_end;
            continue;
        }
        if (c == '/' && pos + 1 < end && template_source[pos + 1] == '*') {
            size_t comment_end = skip_block_comment(template_source, end, pos);
            if (!buffer_append_n(output, template_source + pos, comment_end - pos)) return 0;
            pos = comment_end;
            continue;
        }
        if (c != '$' && repeated_binding && is_ident_start(c)) {
            size_t name_end = scan_identifier(template_source, end, pos);
            const MacroBinding *binding = find_binding(
                bindings, binding_count, template_source + pos, name_end - pos);
            if (binding == repeated_binding) {
                if (!render_binding(output, binding, repeated_binding, repeat_index)) return 0;
            } else if (!buffer_append_n(output, template_source + pos, name_end - pos)) {
                return 0;
            }
            pos = name_end;
            continue;
        }
        if (c != '$') {
            if (!buffer_append_char(output, c)) return 0;
            pos++;
            continue;
        }

        if (pos + 1 < end && template_source[pos + 1] == '(') {
            size_t close = 0;
            if (!find_matching(template_source, end, pos + 1, &close)) {
                macro_error(table, whole_source, invocation_offset,
                            "unclosed repetition in macro expansion");
                return 0;
            }
            size_t star = skip_trivia(template_source, end, close + 1);
            size_t separator_start = star;
            while (star < end && template_source[star] != '*' &&
                   template_source[star] != '\n' && template_source[star] != '\r') {
                star++;
            }
            if (star >= end || template_source[star] != '*') {
                macro_error(table, whole_source, invocation_offset,
                            "expected '*' after macro repetition");
                return 0;
            }
            size_t separator_end = star;
            while (separator_end > separator_start &&
                   isspace((unsigned char)template_source[separator_end - 1])) {
                separator_end--;
            }
            const MacroBinding *loop_binding = repetition_binding(
                table, whole_source, invocation_offset, template_source,
                pos + 2, close, bindings, binding_count);
            if (!loop_binding) return 0;
            for (size_t i = 0; i < loop_binding->value_count; i++) {
                if (i && !buffer_append_n(output, template_source + separator_start,
                                          separator_end - separator_start)) return 0;
                if (!render_range(table, whole_source, invocation_offset,
                                  template_source, pos + 2, close,
                                  bindings, binding_count, loop_binding, i,
                                  output)) return 0;
            }
            pos = star + 1;
            continue;
        }

        size_t name_start = pos + 1;
        size_t name_end = scan_identifier(template_source, end, name_start);
        if (name_end == name_start) {
            macro_error(table, whole_source, invocation_offset,
                        "expected a parameter name after '$' in macro body");
            return 0;
        }
        const MacroBinding *binding = find_binding(
            bindings, binding_count, template_source + name_start, name_end - name_start);
        if (!binding) {
            /* It may be a nested macro call; preserve it for the expansion pass. */
            if (!buffer_append_n(output, template_source + pos, name_end - pos)) return 0;
        } else if (!render_binding(output, binding, repeated_binding, repeat_index)) {
            return 0;
        }
        pos = name_end;
    }
    return 1;
}

static char *expand_text(MacroTable *table, const char *source,
                         size_t source_len, int depth, int strip_definitions);

static char *expand_invocation(MacroTable *table, const char *whole_source,
                               size_t invocation_offset,
                               const MacroDefinition *definition,
                               const char *args_source, size_t args_len,
                               int depth) {
    MacroBinding *bindings = NULL;
    if (!bind_arguments(table, whole_source, invocation_offset, definition,
                        args_source, args_len, &bindings)) return NULL;

    TextBuffer rendered = {0};
    int ok = render_range(table, whole_source, invocation_offset,
                          definition->body, 0, strlen(definition->body),
                          bindings, definition->param_count, NULL, 0,
                          &rendered);
    free_bindings(bindings, definition->param_count);
    if (!ok) {
        free(rendered.data);
        return NULL;
    }
    char *rendered_text = buffer_take(&rendered);
    if (!rendered_text) return NULL;
    char *expanded = expand_text(table, rendered_text, strlen(rendered_text),
                                 depth + 1, 0);
    free(rendered_text);
    return expanded;
}

static int append_definition_padding(TextBuffer *output, const char *source,
                                     size_t start, size_t end) {
    for (size_t i = start; i < end; i++) {
        if (!buffer_append_char(output, source[i] == '\n' ? '\n' : ' ')) return 0;
    }
    return 1;
}

static char *expand_text(MacroTable *table, const char *source,
                         size_t source_len, int depth, int strip_definitions) {
    if (depth > VIX_MACRO_MAX_EXPANSION_DEPTH) {
        macro_error(table, source, 0, "macro expansion exceeded the recursion limit (%d)",
                    VIX_MACRO_MAX_EXPANSION_DEPTH);
        return NULL;
    }

    TextBuffer output = {0};
    size_t pos = 0;
    while (pos < source_len) {
        size_t definition_end = 0;
        if (strip_definitions && in_definition(table, pos, &definition_end)) {
            if (!append_definition_padding(&output, source, pos, definition_end)) goto oom;
            pos = definition_end;
            continue;
        }

        char c = source[pos];
        if (c == '"' || c == '\'') {
            size_t end = skip_quoted(source, source_len, pos);
            if (!buffer_append_n(&output, source + pos, end - pos)) goto oom;
            pos = end;
            continue;
        }
        if (c == '#') {
            size_t end = skip_line_comment(source, source_len, pos + 1);
            if (!buffer_append_n(&output, source + pos, end - pos)) goto oom;
            pos = end;
            continue;
        }
        if (c == '/' && pos + 1 < source_len && source[pos + 1] == '/') {
            size_t end = skip_line_comment(source, source_len, pos + 2);
            if (!buffer_append_n(&output, source + pos, end - pos)) goto oom;
            pos = end;
            continue;
        }
        if (c == '/' && pos + 1 < source_len && source[pos + 1] == '*') {
            size_t end = skip_block_comment(source, source_len, pos);
            if (!buffer_append_n(&output, source + pos, end - pos)) goto oom;
            pos = end;
            continue;
        }
        if (c != '$') {
            if (!buffer_append_char(&output, c)) goto oom;
            pos++;
            continue;
        }

        size_t name_start = pos + 1;
        size_t name_end = scan_identifier(source, source_len, name_start);
        if (name_end == name_start) {
            macro_error(table, source, pos, "expected a macro name after '$'");
            free(output.data);
            return NULL;
        }
        MacroDefinition *definition = find_macro(
            table, source + name_start, name_end - name_start);
        if (!definition) {
            char *name = copy_range(source, name_start, name_end);
            macro_error(table, source, pos, "unknown macro '$%s'", name ? name : "<unknown>");
            free(name);
            free(output.data);
            return NULL;
        }
        size_t opener_pos = skip_trivia(source, source_len, name_end);
        if (opener_pos >= source_len ||
            (source[opener_pos] != '(' && source[opener_pos] != '[')) {
            macro_error(table, source, pos, "expected '(' or '[' after macro '$%s'",
                        definition->name);
            free(output.data);
            return NULL;
        }
        if (source[opener_pos] != definition->opener) {
            macro_error(table, source, opener_pos,
                        "macro '$%s' must be invoked with '%c...%c'",
                        definition->name, definition->opener, definition->closer);
            free(output.data);
            return NULL;
        }
        size_t close = 0;
        if (!find_matching(source, source_len, opener_pos, &close)) {
            macro_error(table, source, opener_pos, "unclosed invocation of macro '$%s'",
                        definition->name);
            free(output.data);
            return NULL;
        }
        char *expanded = expand_invocation(
            table, source, pos, definition,
            source + opener_pos + 1, close - opener_pos - 1, depth);
        if (!expanded) {
            free(output.data);
            return NULL;
        }
        if (!buffer_append(&output, expanded)) {
            free(expanded);
            goto oom;
        }
        free(expanded);
        pos = close + 1;
    }
    return buffer_take(&output);

oom:
    free(output.data);
    macro_error(table, source, pos, "out of memory while expanding macros");
    return NULL;
}

char *vix_expand_macros(const char *source, const char *filename) {
    if (!source) return NULL;
    MacroTable table = {0};
    table.filename = filename;
    size_t len = strlen(source);

    if (!collect_definitions(&table, source, len)) {
        free_table(&table);
        return NULL;
    }
    char *expanded = expand_text(&table, source, len, 0, 1);
    free_table(&table);
    return expanded;
}

static char *read_stream(FILE *input) {
    TextBuffer source = {0};
    char chunk[4096];
    size_t count;
    while ((count = fread(chunk, 1, sizeof(chunk), input)) != 0) {
        if (!buffer_append_n(&source, chunk, count)) {
            free(source.data);
            return NULL;
        }
    }
    if (ferror(input)) {
        free(source.data);
        return NULL;
    }
    return buffer_take(&source);
}

FILE *vix_preprocess_macros(FILE *input, const char *filename) {
    if (!input) return NULL;
    char *source = read_stream(input);
    if (!source) {
        fprintf(stderr, "error [MacroError]: failed to read %s\n",
                filename ? filename : "<input>");
        return NULL;
    }
    char *expanded = vix_expand_macros(source, filename);
    free(source);
    if (!expanded) return NULL;

    FILE *result = tmpfile();
    if (!result) {
        fprintf(stderr, "error [MacroError]: failed to create preprocessing stream for %s\n",
                filename ? filename : "<input>");
        free(expanded);
        return NULL;
    }
    size_t len = strlen(expanded);
    if (len && fwrite(expanded, 1, len, result) != len) {
        fprintf(stderr, "error [MacroError]: failed to write preprocessing stream for %s\n",
                filename ? filename : "<input>");
        free(expanded);
        fclose(result);
        return NULL;
    }
    free(expanded);
    rewind(result);
    return result;
}
