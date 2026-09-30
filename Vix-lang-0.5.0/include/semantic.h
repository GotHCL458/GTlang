#ifndef SEMANTIC_H
#define SEMANTIC_H
#include "ast.h"
#include "typeinfer.h"

typedef enum {
    SYMBOL_VARIABLE,
    SYMBOL_CONSTANT,
    SYMBOL_FUNCTION
} SymbolType;

typedef struct Symbol {
    char* name;
    SymbolType type;
    InferredType inferred_type;
    int is_mutable_pointer;  // 是否是可变指针
    struct Symbol* next;
} Symbol;

typedef struct SymbolTable {
    Symbol* head;
    struct SymbolTable* parent;
} SymbolTable;

SymbolTable* create_symbol_table(SymbolTable* parent);
void destroy_symbol_table(SymbolTable* symbol_table);
int add_symbol(SymbolTable* table, const char* name, SymbolType type, InferredType inferred_type);
int add_symbol_with_mutability(SymbolTable* table, const char* name, SymbolType type, InferredType inferred_type, int is_mutable_pointer);
Symbol* lookup_symbol(SymbolTable* table, const char* name);
int check_undefined_symbols(ASTNode* node);
int check_undefined_symbols_in_node(ASTNode* node, SymbolTable* table);
int check_unused_variables(ASTNode* node, SymbolTable* table);
int is_variable_used_in_node(ASTNode* node, const char* var_name);
struct VariableUsage;
int check_unused_variables_with_usage(ASTNode* node, SymbolTable* table, struct VariableUsage** usage_list);
void report_undefined_identifier_with_location_and_column(const char* identifier, const char* filename, int line, int column);
void report_undefined_function_with_location_and_column(const char* identifier, const char* filename, int line, int column);

#endif /*SEMANTIC_H*/