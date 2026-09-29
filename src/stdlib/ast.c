/* ============================================================
 * ast.c —— 纯 C 实现的 AST 库（为 GTLang 自举准备）
 *
 * 节点：{ kind, sval, ival, nchildren, children[] }
 * 用 i64 句柄在 GTLang 侧表示（即指针地址）。
 *
 * 高性能：O(1) 建节点/加子节点；dump 用动态字符串缓冲。
 * ============================================================ */
#define _CRT_SECURE_NO_WARNINGS 1
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <stdint.h>

typedef struct AstNode {
    int64_t kind;
    char   *sval;
    int64_t ival;
    int64_t  nchildren;
    int64_t  cap;
    struct AstNode **children;
} AstNode;

__declspec(dllexport) AstNode *ast_node(int64_t kind, const char *sval, int64_t ival) {
    AstNode *n = (AstNode *)calloc(1, sizeof(AstNode));
    if (!n) return NULL;
    n->kind = kind;
    n->sval = sval ? _strdup(sval) : NULL;
    n->ival = ival;
    n->nchildren = 0;
    n->cap = 4;
    n->children = (AstNode **)malloc(sizeof(AstNode *) * (size_t)n->cap);
    return n;
}

__declspec(dllexport) void ast_add(AstNode *parent, AstNode *child) {
    if (!parent || !child) return;
    if (parent->nchildren >= parent->cap) {
        parent->cap *= 2;
        parent->children = (AstNode **)realloc(parent->children, sizeof(AstNode *) * (size_t)parent->cap);
    }
    parent->children[parent->nchildren++] = child;
}

__declspec(dllexport) int64_t ast_kind(AstNode *n) { return n ? n->kind : -1; }
__declspec(dllexport) int64_t ast_nchildren(AstNode *n) { return n ? n->nchildren : 0; }
__declspec(dllexport) AstNode *ast_child(AstNode *n, int64_t i) {
    if (!n || i < 0 || i >= n->nchildren) return NULL;
    return n->children[i];
}
__declspec(dllexport) const char *ast_sval(AstNode *n) { return (n && n->sval) ? n->sval : ""; }
__declspec(dllexport) int64_t ast_ival(AstNode *n) { return n ? n->ival : 0; }

/* dump：S-表达式 */
static void dump_rec(AstNode *n, char **buf, size_t *len, size_t *cap) {
    if (!n) { return; }
    char tmp[64];
    int k = snprintf(tmp, sizeof(tmp), "(k%d", (int)n->kind);
    if (*len + (size_t)k + 1 > *cap) { *cap = (*cap + (size_t)k) * 2; *buf = (char *)realloc(*buf, *cap); }
    memcpy(*buf + *len, tmp, (size_t)k); *len += (size_t)k;
    if (n->sval) {
        size_t sl = strlen(n->sval);
        if (*len + sl + 3 > *cap) { *cap = (*cap + sl) * 2; *buf = (char *)realloc(*buf, *cap); }
        (*buf)[(*len)++] = ' ';
        (*buf)[(*len)++] = '"';
        memcpy(*buf + *len, n->sval, sl); *len += sl;
        (*buf)[(*len)++] = '"';
    }
    for (int64_t i = 0; i < n->nchildren; i++) {
        (*buf)[(*len)++] = ' ';
        dump_rec(n->children[i], buf, len, cap);
    }
    if (*len + 2 > *cap) { *cap += 2; *buf = (char *)realloc(*buf, *cap); }
    (*buf)[(*len)++] = ')';
    (*buf)[(*len)] = '\0';
}

__declspec(dllexport) char *ast_dump(AstNode *n) {
    size_t cap = 256, len = 0;
    char *buf = (char *)malloc(cap);
    if (!buf) return NULL;
    buf[0] = '\0';
    dump_rec(n, &buf, &len, &cap);
    return buf;
}

__declspec(dllexport) void ast_free(AstNode *n) {
    if (!n) return;
    for (int64_t i = 0; i < n->nchildren; i++) { ast_free(n->children[i]); }
    if (n->children) free(n->children);
    if (n->sval) free(n->sval);
    free(n);
}

/* 节点类型名（供 GTLang 侧打印） */
__declspec(dllexport) const char *ast_kind_name(int64_t k) {
    switch (k) {
        case 0:  return "Program";
        case 1:  return "Fn";
        case 2:  return "Block";
        case 3:  return "Let";
        case 4:  return "Assign";
        case 5:  return "If";
        case 6:  return "While";
        case 7:  return "For";
        case 8:  return "Return";
        case 9:  return "Call";
        case 10: return "BinOp";
        case 11: return "UnOp";
        case 12: return "Lit";
        case 13: return "Ident";
        case 14: return "Struct";
        case 15: return "Enum";
        case 16: return "Match";
        default: return "Unknown";
    }
}
