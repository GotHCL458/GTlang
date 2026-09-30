#ifndef AST_H
#define AST_H
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stddef.h>

#define VIX_STRING_SLICE_INTRINSIC "__vix_string_slice"

typedef struct {
    int first_line;
    int first_column;
    int last_line;
    int last_column;
} Location;
#ifndef YYLTYPE_IS_DECLARED
#define YYLTYPE_IS_DECLARED
typedef struct YYLTYPE {
    int first_line;
    int first_column;
    int last_line;
    int last_column;
} YYLTYPE;
#endif

typedef struct ASTNode ASTNode;

typedef enum {
    TYPEINFO_VOID,
    TYPEINFO_I8,
    TYPEINFO_I32,
    TYPEINFO_I64,
    TYPEINFO_F32,
    TYPEINFO_F64,
    TYPEINFO_BOOL,
    TYPEINFO_STRING,
    TYPEINFO_PTR,
    TYPEINFO_STRUCT,
    TYPEINFO_ARRAY,
    TYPEINFO_FIXED_ARRAY,
    TYPEINFO_FN,
    TYPEINFO_APP,
    TYPEINFO_VAR
} TypeInfoKind;

typedef struct TypeInfo {
    TypeInfoKind kind;
    struct TypeInfo* element;
    size_t size;
    char* name;
    struct TypeInfo** params;
    int param_count;
    struct TypeInfo* ret;
    struct TypeInfo* app_ctor;
    struct TypeInfo** app_args;
    int app_arg_count;
} TypeInfo;

typedef enum {
    AST_PROGRAM,
    AST_PRINT,
    AST_ASSIGN,
    AST_CONST,
    AST_GLOBAL,  // 全局变量声明节点
    AST_IMPORT,  // 添加模块导入节点
    AST_BINOP,
    AST_UNARYOP,
    AST_NUM_INT,
    AST_NUM_FLOAT,
    AST_STRING,
    AST_CHAR,  // 添加字符字面量节点类型
    AST_IDENTIFIER,
    AST_TYPE_INT32,
    AST_TYPE_INT64,
    AST_TYPE_INT8,
    AST_TYPE_FLOAT32,
    AST_TYPE_FLOAT64,
    AST_TYPE_STRING,
    AST_TYPE_VOID,
    AST_TYPE_POINTER,
    AST_TYPE_LIST,
    AST_TYPE_FIXED_SIZE_LIST,  // 新增：固定大小列表类型
    AST_TYPE_APP,
    AST_EXPRESSION_LIST,
    AST_INDEX,// 数组/列表索引访问
    AST_MEMBER_ACCESS,//结构体字段访问
    AST_INPUT,
    AST_TOINT,
    AST_TOFLOAT,
    AST_IF,
    AST_WHILE,
    AST_BREAK,
    AST_CONTINUE,
    AST_FOR,
    AST_FUNCTION,
    AST_RETURN,
    AST_CALL,
    AST_STRUCT_DEF,
    AST_STRUCT_LITERAL,
    AST_NIL,
    AST_ARRAY_PATTERN
} NodeType;

typedef enum {
    OP_ADD,  // +
    OP_SUB, // -
    OP_MUL, // *
    OP_DIV,     // /
    OP_MOD, // %
    OP_POW,     // **
    OP_CONCAT,   // +
    OP_REPEAT,   // *
    OP_EQ,  // ==
    OP_NE,  // !=
    OP_LT,  // <
    OP_LE,  // <=
    OP_GT,  // >
    OP_GE,  // >=
    OP_AND, // and
    OP_OR   // or
} BinOpType;

typedef enum {
    OP_MINUS,    // -
    OP_PLUS      // +
    , OP_ADDRESS  // & 取地址
    , OP_DEREF    // @ 解引用
    , OP_NOT 
} UnaryOpType;
typedef enum {
    MUTABILITY_IMMUTABLE = 0,  // 不可变
    MUTABILITY_MUTABLE         // 可变
} MutabilityType;

typedef struct ASTNode {
    NodeType type;
    Location location;// 位置信息
    const char* source_file;
    MutabilityType mutability; // 可变性标记
    TypeInfo* inferred_type;
    union {
        struct {
            struct ASTNode** statements;
            int statement_count;
        } program;
        struct {
            struct ASTNode* expr;
        } print;
        struct {
            struct ASTNode** expressions;
            int expression_count;
            int precomputed_length;/*length长度*/
        } expression_list;
        struct {
            struct ASTNode** elements;
            int element_count;
            int rest_index;
        } array_pattern;
        struct {
            struct ASTNode* target;
            struct ASTNode* index;
        } index;
        struct {
            struct ASTNode* object;
            struct ASTNode* field;
        } member_access;
        struct {
            struct ASTNode* left;
            struct ASTNode* right;
            MutabilityType mutability;
            int is_declaration;
            int is_public;
            int is_multi_pattern;
            struct ASTNode* declared_type;
        } assign;
        struct {
            struct ASTNode* left;
            struct ASTNode* right;
            BinOpType op;
        } binop;
        struct {
            struct ASTNode* expr;
            UnaryOpType op;
        } unaryop;
        struct {
            long long value;
        } num_int;
        struct {
            double value;
        } num_float;
        struct {
            char value;  // 字符值
        } character;
        struct {
            char* value;
        } string;
        struct {
            char* name;
        } identifier;
        struct {
            struct ASTNode* element_type;
        } list_type;
        struct {
            struct ASTNode* element_type;
        } pointer_type;
        struct {
            struct ASTNode* element_type;
            long long size;
        } fixed_size_list_type;
        struct {
            struct ASTNode* ctor;
            struct ASTNode* args;
        } type_app;
        struct {
            struct ASTNode* prompt;
        } input;
        struct {
            struct ASTNode* expr;
        } toint;
        struct {
            struct ASTNode* expr;
        } tofloat;
        struct {
            struct ASTNode* condition;
            struct ASTNode* then_body;
            struct ASTNode* else_body;  // 可以为NULL
        } if_stmt;
        struct {
            struct ASTNode* condition;
            struct ASTNode* body;
        } while_stmt;
        struct {
            struct ASTNode* var;
            struct ASTNode* start;
            struct ASTNode* end;
            struct ASTNode* body;
        } for_stmt;
        struct {
            char* name;
            struct ASTNode* params;
            struct ASTNode* generic_params;
            struct ASTNode* return_type;
            struct ASTNode* body;
            int is_extern;
            char* linkage;
            int vararg;
            int is_public;  // 添加公共函数标记
        } function;
        struct {
            struct ASTNode* func;
            struct ASTNode* args;
            struct ASTNode* type_args;
        } call;
        struct {
            char* name;
            struct ASTNode* fields;
            struct ASTNode* generic_params;
            int is_public;
        } struct_def;
        struct {
            struct ASTNode* type_name;
            struct ASTNode* fields;
        } struct_literal;
        struct {
            struct ASTNode* expr;
        } return_stmt;
        struct {
            struct ASTNode* identifier;
            struct ASTNode* type;//可能为NULL
            struct ASTNode* initializer;
            int is_public;
        } global_decl;
        struct {//import的数据节点
            char* module_path;
        } import;
    } data;
} ASTNode;

#ifdef __cplusplus
extern "C" {
#endif

void free_type_info(TypeInfo* info);

ASTNode* create_program_node();
ASTNode* create_program_node_with_location(Location location);
ASTNode* create_program_node_with_yyltype(void* yylloc);
void add_statement_to_program(ASTNode* program, ASTNode* statement);
ASTNode* create_print_node(ASTNode* expr);
ASTNode* create_print_node_with_location(ASTNode* expr, Location location);
ASTNode* create_print_node_with_yyltype(ASTNode* expr, void* yylloc);
ASTNode* create_input_node(ASTNode* prompt);
ASTNode* create_input_node_with_location(ASTNode* prompt, Location location);
ASTNode* create_input_node_with_yyltype(ASTNode* prompt, void* yylloc);
ASTNode* create_toint_node(ASTNode* expr);
ASTNode* create_toint_node_with_location(ASTNode* expr, Location location);
ASTNode* create_toint_node_with_yyltype(ASTNode* expr, void* yylloc);
ASTNode* create_tofloat_node(ASTNode* expr);
ASTNode* create_tofloat_node_with_location(ASTNode* expr, Location location);
ASTNode* create_tofloat_node_with_yyltype(ASTNode* expr, void* yylloc);
ASTNode* create_expression_list_node();
ASTNode* create_expression_list_node_with_location(Location location);
ASTNode* create_expression_list_node_with_yyltype(void* yylloc);
void add_expression_to_list(ASTNode* list, ASTNode* expr);
ASTNode* create_array_pattern_node_with_location(Location location);
ASTNode* create_array_pattern_node_with_yyltype(void* yylloc);
void add_array_pattern_element(ASTNode* pattern, ASTNode* element);
int set_array_pattern_rest(ASTNode* pattern);
ASTNode* create_assign_node(ASTNode* left, ASTNode* right);
ASTNode* create_assign_node_with_location(ASTNode* left, ASTNode* right, Location location);
ASTNode* create_assign_node_with_yyltype(ASTNode* left, ASTNode* right, void* yylloc);
ASTNode* create_assign_node_with_mutability(ASTNode* left, ASTNode* right, MutabilityType mutability);
ASTNode* create_const_node(ASTNode* left, ASTNode* right);
ASTNode* create_const_node_with_location(ASTNode* left, ASTNode* right, Location location);
ASTNode* create_const_node_with_yyltype(ASTNode* left, ASTNode* right, void* yylloc);
ASTNode* create_binop_node(BinOpType op, ASTNode* left, ASTNode* right);
ASTNode* create_binop_node_with_location(BinOpType op, ASTNode* left, ASTNode* right, Location location);
ASTNode* create_binop_node_with_yyltype(BinOpType op, ASTNode* left, ASTNode* right, void* yylloc);
ASTNode* create_unaryop_node(UnaryOpType op, ASTNode* expr);
ASTNode* create_unaryop_node_with_location(UnaryOpType op, ASTNode* expr, Location location);
ASTNode* create_unaryop_node_with_yyltype(UnaryOpType op, ASTNode* expr, void* yylloc);
ASTNode* create_num_int_node(long long value);
ASTNode* create_num_int_node_with_location(long long value, Location location);
ASTNode* create_num_int_node_with_yyltype(long long value, void* yylloc);
ASTNode* create_bool_node_with_yyltype(long long value, void* yylloc);
ASTNode* create_num_float_node(double value);
ASTNode* create_num_float_node_with_location(double value, Location location);
ASTNode* create_num_float_node_with_yyltype(double value, void* yylloc);
ASTNode* create_string_node(const char* value);
ASTNode* create_string_node_with_location(const char* value, Location location);
ASTNode* create_string_node_with_yyltype(const char* value, void* yylloc);
ASTNode* create_char_node(char value);
ASTNode* create_char_node_with_location(char value, Location location);
ASTNode* create_char_node_with_yyltype(char value, void* yylloc);
ASTNode* create_nil_node();
ASTNode* create_nil_node_with_location(Location location);
ASTNode* create_nil_node_with_yyltype(void* yylloc);
ASTNode* create_identifier_node(const char* name);
ASTNode* create_identifier_node_with_location(const char* name, Location location);
ASTNode* create_identifier_node_with_yyltype(const char* name, void* yylloc);
ASTNode* create_type_node(NodeType type);
ASTNode* create_type_node_with_location(NodeType type, Location location);
ASTNode* create_list_type_node_with_location(ASTNode* element_type, Location location);
ASTNode* create_list_type_node(ASTNode* element_type);
ASTNode* create_fixed_size_list_type_node_with_location(ASTNode* element_type, long long size, Location location);
ASTNode* create_fixed_size_list_type_node(ASTNode* element_type, long long size);
ASTNode* create_type_app_node_with_location(ASTNode* ctor, ASTNode* args, Location location);
ASTNode* create_type_app_node(ASTNode* ctor, ASTNode* args);
ASTNode* create_type_app_node_with_yyltype(ASTNode* ctor, ASTNode* args, void* yylloc);
ASTNode* create_if_node_with_location(ASTNode* condition, ASTNode* then_body, ASTNode* else_body, Location location);
ASTNode* create_if_node_with_yyltype(ASTNode* condition, ASTNode* then_body, ASTNode* else_body, void* yylloc);
ASTNode* create_if_node(ASTNode* condition, ASTNode* then_body, ASTNode* else_body);
ASTNode* create_while_node_with_location(ASTNode* condition, ASTNode* body, Location location);
ASTNode* create_while_node_with_yyltype(ASTNode* condition, ASTNode* body, void* yylloc);
ASTNode* create_while_node(ASTNode* condition, ASTNode* body);
ASTNode* create_for_node(ASTNode* var, ASTNode* start, ASTNode* end, ASTNode* body);
ASTNode* create_for_node_with_location(ASTNode* var, ASTNode* start, ASTNode* end, ASTNode* body, Location location);
ASTNode* create_for_node_with_yyltype(ASTNode* var, ASTNode* start, ASTNode* end, ASTNode* body, void* yylloc);
ASTNode* create_break_node();
ASTNode* create_break_node_with_location(Location location);
ASTNode* create_break_node_with_yyltype(void* yylloc);
ASTNode* create_continue_node();
ASTNode* create_continue_node_with_location(Location location);
ASTNode* create_continue_node_with_yyltype(void* yylloc);
ASTNode* create_function_node(const char* name, ASTNode* params, ASTNode* return_type, ASTNode* body);
ASTNode* create_function_node_with_location(const char* name, ASTNode* params, ASTNode* return_type, ASTNode* body, Location location);
ASTNode* create_public_function_node(const char* name, ASTNode* params, ASTNode* return_type, ASTNode* body);  // 添加创建公共函数的函数
ASTNode* create_public_function_node_with_location(const char* name, ASTNode* params, ASTNode* return_type, ASTNode* body, Location location);  // 添加创建公共函数的函数
ASTNode* create_extern_function_node(const char* name, ASTNode* params, ASTNode* return_type, const char* linkage);
ASTNode* create_extern_function_node_with_location(const char* name, ASTNode* params, ASTNode* return_type, const char* linkage, Location location);
ASTNode* create_return_node(ASTNode* expr);
ASTNode* create_return_node_with_location(ASTNode* expr, Location location);
ASTNode* create_return_node_with_yyltype(ASTNode* expr, void* yylloc);
ASTNode* create_call_node(ASTNode* func, ASTNode* args);
ASTNode* create_call_node_with_location(ASTNode* func, ASTNode* args, Location location);
ASTNode* create_call_node_with_yyltype(ASTNode* func, ASTNode* args, void* yylloc);
ASTNode* create_index_node(ASTNode* target, ASTNode* index);
ASTNode* create_index_node_with_location(ASTNode* target, ASTNode* index, Location location);
ASTNode* create_index_node_with_yyltype(ASTNode* target, ASTNode* index, void* yylloc);
ASTNode* create_member_access_node(ASTNode* object, ASTNode* field);
ASTNode* create_member_access_node_with_location(ASTNode* object, ASTNode* field, Location location);
ASTNode* create_member_access_node_with_yyltype(ASTNode* object, ASTNode* field, void* yylloc);
ASTNode* create_struct_def_node(const char* name, ASTNode* fields);
ASTNode* create_struct_def_node_with_location(const char* name, ASTNode* fields, Location location);
ASTNode* create_struct_def_node_with_yyltype(const char* name, ASTNode* fields, void* yylloc);
ASTNode* create_struct_literal_node(ASTNode* type_name, ASTNode* fields);
ASTNode* create_struct_literal_node_with_location(ASTNode* type_name, ASTNode* fields, Location location);
ASTNode* create_struct_literal_node_with_yyltype(ASTNode* type_name, ASTNode* fields, void* yylloc);
ASTNode* create_global_node_with_location(ASTNode* identifier, ASTNode* type, ASTNode* initializer, Location location);
ASTNode* create_global_node(ASTNode* identifier, ASTNode* type, ASTNode* initializer);
ASTNode* create_global_node_with_yyltype(ASTNode* identifier, ASTNode* type, ASTNode* initializer, void* yylloc);
ASTNode* create_import_node(const char* module_path);
ASTNode* create_import_node_with_location(const char* module_path, Location location);
ASTNode* create_import_node_with_yyltype(const char* module_path, void* yylloc);
void free_ast(ASTNode* node);
void print_ast(ASTNode* node, int indent);
int get_array_length(ASTNode* node);
// Inline imports: parse modules and inline their `pub` functions into the AST
int inline_imports(ASTNode* node);
// Resolve import path with VPM search priority.
// Returns 1 on success and writes resolved path to 'out', 0 on failure.
int vix_resolve_import_path(const char* current_file, const char* module_path, char* out, size_t out_size);

#ifdef __cplusplus
}
#endif

#endif/*AST_H*/
