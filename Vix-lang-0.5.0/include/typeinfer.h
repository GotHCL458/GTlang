#ifndef TYPE_INFERENCE_H
#define TYPE_INFERENCE_H
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

typedef struct ASTNode ASTNode;

typedef struct StructTypeInfo StructTypeInfo;

typedef enum {
    TYPE_UNKNOWN,
    TYPE_INT,
    TYPE_FLOAT,
    TYPE_STRING,
    TYPE_LIST,
    TYPE_POINTER, //指针类型
    TYPE_STRUCT,//结构体类型
    TYPE_INT8,
    TYPE_BOOL
} InferredType;
typedef struct {
    char* name;// 字段名称
    InferredType type;// 字段类型
    int offset;// 字段偏移量
    StructTypeInfo* struct_type;
} StructField;
typedef struct StructTypeInfo {
    char* name;
    StructField* fields;
    int field_count;
    int total_size;
} StructTypeInfo;

typedef struct {
    char* name;
    InferredType type;
    InferredType element_type;
    InferredType pointer_target_type;
    StructTypeInfo* struct_type;
    int array_length;
} VariableInfo;

typedef struct {
    VariableInfo* variables;
    int count;
    int capacity;
} TypeInferenceContext;

TypeInferenceContext* create_type_inference_context();
void free_type_inference_context(TypeInferenceContext* ctx);
InferredType infer_type(TypeInferenceContext* ctx, ASTNode* node);
InferredType get_variable_type(TypeInferenceContext* ctx, const char* var_name);
void set_variable_type(TypeInferenceContext* ctx, const char* var_name, InferredType type);
void set_variable_list_type(TypeInferenceContext* ctx, const char* var_name, InferredType list_type, InferredType element_type);
void set_variable_pointer_type(TypeInferenceContext* ctx, const char* var_name, InferredType target_type);//设置指针类型
InferredType get_variable_pointer_target_type(TypeInferenceContext* ctx, const char* var_name);//获取指针指向的类型
void set_variable_struct_type(TypeInferenceContext* ctx, const char* var_name, StructTypeInfo* struct_type);//设置结构体类型
StructTypeInfo* create_struct_type(const char* name);
void free_struct_type(StructTypeInfo* struct_type);
StructTypeInfo* get_struct_type(TypeInferenceContext* ctx, const char* struct_name);
InferredType get_struct_field_type(TypeInferenceContext* ctx, const char* struct_name, const char* field_name);
void process_struct_definition(TypeInferenceContext* ctx, ASTNode* struct_def_node);//处理结构体定义
const char* type_to_cpp_string(InferredType type);
int has_variable(TypeInferenceContext* ctx, const char* var_name);
#endif/*TYPE_INFERENCE_H*/