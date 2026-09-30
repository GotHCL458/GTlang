#ifndef TYPE_HELPER_H
#define TYPE_HELPER_H

#include <llvm/IR/LLVMContext.h>
#include <llvm/IR/Type.h>
#include <llvm/IR/DerivedTypes.h>
#include <llvm/IR/IRBuilder.h>
#include <map>
#include <unordered_map>
#include <unordered_set>
#include <string>
#include <vector>
#include "../../include/ast.h"

enum class ValueType {
    VOID,
    INT32,
    INT64,
    INT8,
    FLOAT32,
    FLOAT64,
    BOOL,
    POINTER,
    STRING,
    ARRAY
};

class TypeHelper {
private:
    llvm::LLVMContext& context;
    std::map<std::string, llvm::StructType*> structTypes;
    std::map<std::string, std::vector<std::pair<std::string, llvm::Type*>>> structFields;
    std::map<std::string, ASTNode*> structTemplates;
    std::map<std::string, std::pair<llvm::Type*, int>> arrayTypes;
    std::map<std::string, int> variableArraySizes;
    std::map<std::string, bool> stringVariables;
    std::map<std::string, llvm::Type*> arrayFieldElementTypes;
    std::map<std::string, llvm::Type*> genericTypeBindings;
    std::unordered_set<std::string> instantiating;
    // Field-index cache: structName → { fieldName → index }
    std::unordered_map<std::string, std::unordered_map<std::string, int>> fieldIndexCache;

    int getTypeRank(ValueType type);

public:
    TypeHelper(llvm::LLVMContext& ctx) : context(ctx) {}

    void setGenericTypeBindings(const std::map<std::string, llvm::Type*>& bindings);
    void clearGenericTypeBindings();
    void registerStringVariable(const std::string& name);
    bool isStringVariable(const std::string& name);

    void registerArrayType(const std::string& name, llvm::Type* elementType, int elementCount);
    std::pair<llvm::Type*, int>* getArrayTypeInfo(const std::string& name);
    void registerVariableArraySize(const std::string& varName, int size);
    int getVariableArraySize(const std::string& varName);

    llvm::Type* createArrayType(llvm::Type* elementType, int elementCount);
    llvm::Type* getArrayElementTypeFromNode(ASTNode* node);
    int getArrayElementCountFromNode(ASTNode* node);

    void registerStructType(const std::string& name, llvm::StructType* type,
                            std::vector<std::pair<std::string, llvm::Type*>> fields);
    void registerStructTemplate(const std::string& name, ASTNode* structDef);
    ASTNode* getStructTemplate(const std::string& name);
    llvm::StructType* getStructType(const std::string& name);
    std::vector<std::pair<std::string, llvm::Type*>>* getStructFields(const std::string& name);
    int getFieldIndex(const std::string& structName, const std::string& fieldName);
    void registerFieldArrayElementType(const std::string& structName, const std::string& fieldName, llvm::Type* elemType);
    llvm::Type* getFieldArrayElementType(const std::string& structName, const std::string& fieldName);

    std::string typeNodeToToken(ASTNode* typeNode) const;
    std::string mangleStructInstanceName(const std::string& baseName, ASTNode* typeArgs) const;
    llvm::Type* instantiateStructType(const std::string& baseName, ASTNode* typeArgs);
    llvm::StructType* inferStructTypeByFieldName(const std::string& fieldName, std::string* outStructName = nullptr);

    llvm::Type* getLLVMType(ValueType type);
    ValueType fromLLVMType(llvm::Type* type);
    llvm::Type* getTypeFromTypeNode(ASTNode* node);
    llvm::Type* typeNodeToLLVM(ASTNode* node);  // unified lookup, shared with Funcs.cpp
    ValueType fromTypeNode(ASTNode* node);
    ValueType getValueTypeFromType(llvm::Type* type);
    std::pair<ValueType, ValueType> promoteTypes(ValueType left, ValueType right);
    ValueType promoteTo(ValueType from, ValueType to);
    llvm::Value* castValue(llvm::IRBuilder<>& builder, llvm::Value* val, ValueType from, ValueType to);
};

#endif
