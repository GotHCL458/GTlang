#ifndef CODE_GENERATOR_H
#define CODE_GENERATOR_H

#include "Scope.h"
#include "Types.h"
#include "Attrs.h"
#include "Debug.h"
#include "../../include/ast.h"
#include "../../include/parser.h"
#include <llvm/IR/IRBuilder.h>
#include <llvm/IR/Module.h>
#include <llvm/IR/LLVMContext.h>
#include <llvm/IR/Function.h>
#include <llvm/IR/Type.h>
#include <llvm/IR/Value.h>
#include <llvm/IR/BasicBlock.h>
#include <llvm/IR/Verifier.h>
#include <llvm/Support/raw_ostream.h>
#include <llvm/Support/TargetSelect.h>
#include <llvm/TargetParser/Host.h>
#include <llvm/MC/TargetRegistry.h>
#include <llvm/Target/TargetMachine.h>
#include <llvm/Target/TargetOptions.h>
#include <map>
#include <set>
#include <string>
#include <memory>
#include <functional>

extern "C" const char* current_input_filename;
extern "C" void report_semantic_error_with_location(const char* message, const char* filename, int line);

static std::string g_vix_target_triple;

static inline bool isBuiltinUnionCtorName(const std::string& name) {
    return name == "Some" || name == "None" || name == "Ok" || name == "Err";
}

static inline bool isRegisteredUnionCtorName(const std::string& name) {
    return vix_adt_ctor_base_name(name.c_str()) != nullptr;
}

static inline int32_t ctorTagValue(const std::string& name) {
    // FNV-1a 32-bit — stable across platforms and runs
    uint32_t hash = 2166136261u;
    for (char c : name) {
        hash ^= static_cast<uint8_t>(c);
        hash *= 16777619u;
    }
    return static_cast<int32_t>(hash & 0x7fffffff);
}

static inline void initTarget() {
    llvm::InitializeAllTargetInfos();
    llvm::InitializeAllTargets();
    llvm::InitializeAllTargetMCs();
    llvm::InitializeAllAsmParsers();
    llvm::InitializeAllAsmPrinters();
}

class LLVMCodeGenerator {
public:
    struct VisitResult {
        llvm::Value* value;
        ValueType type;
        llvm::StructType* structType;

        VisitResult() : value(nullptr), type(ValueType::VOID), structType(nullptr) {}
        VisitResult(llvm::Value* v, ValueType t) : value(v), type(t), structType(nullptr) {}
        VisitResult(llvm::Value* v, ValueType t, llvm::StructType* st) : value(v), type(t), structType(st) {}
    };

private:
    llvm::LLVMContext context;
    llvm::IRBuilder<> builder;
    std::unique_ptr<llvm::Module> module;
    ScopeManager scopeManager;
    TypeHelper typeHelper;
    llvm::Function* printfFunction;
    llvm::Function* strlenFunction;
    llvm::Function* strcpyFunction;
    llvm::Function* strcatFunction;
    bool isGlobalScope;
    bool mainFunctionCreated;
    SourceAttrInfo sourceAttrs;
    std::map<std::string, std::vector<int>> functionArrayParamPositions;
    std::map<std::string, llvm::StructType*> functionSRetResultTypesByName;
    std::map<llvm::Function*, llvm::StructType*> functionSRetResultTypesByFunc;
    std::map<std::string, llvm::Type*> functionArrayReturnElementTypes;
    std::map<std::string, ASTNode*> genericFunctionTemplates;
    std::map<std::string, int> genericFunctionArity;
    std::map<std::string, ASTNode*> allFunctionNodes;
    std::set<std::string> compiledFunctions;
    std::map<std::string, llvm::Type*> activeGenericTypeBindings;
    std::map<const llvm::Value*, llvm::Type*> pointerElementHints;
    std::map<std::string, llvm::StructType*> paramStructTypes;
    std::map<std::string, int> memberArrayLengthHints;
    std::map<std::string, int> memberNestedArrayLengthHints;
    std::vector<llvm::BasicBlock*> loopBreakTargets;
    std::vector<llvm::BasicBlock*> loopContinueTargets;

    static constexpr uint64_t ARRAY_HEADER_BYTES = 8;

    // Error reporting
    void reportCodegenSemanticError(ASTNode* node, const std::string& message);

    // SRet helpers
    bool usesStructSRet(llvm::Function* func) const;
    llvm::StructType* getStructSRetType(llvm::Function* func) const;
    void registerStructSRetFunction(const std::string& funcName, llvm::Function* func, llvm::StructType* structType);

    // Name mangling
    static std::string sanitizeTypeToken(const std::string& raw);
    std::string typeNodeToMangleToken(ASTNode* typeNode) const;
    std::string mangleGenericFunctionName(const std::string& baseName, ASTNode* typeArgs) const;
    bool bindGenericTypeArgs(ASTNode* fnNode, ASTNode* typeArgs, std::map<std::string, llvm::Type*>& outBindings);
    llvm::Function* instantiateGenericFunction(const std::string& baseName, ASTNode* typeArgs);
    static std::string llvmTypeToToken(llvm::Type* type);

    // Insert point / module helpers
    bool ensureValidInsertPoint();
    llvm::Value* safeCreateGlobalString(const std::string& str, const std::string& name);
    llvm::Type* getActualType(llvm::AllocaInst* alloc);

    // TypeInfo helpers
    llvm::Type* getLLVMTypeFromTypeInfo(const TypeInfo* info);
    ValueType getValueTypeFromTypeInfo(const TypeInfo* info);
    llvm::Type* getInferredLLVMType(ASTNode* node);
    llvm::Type* getInferredArrayElementType(ASTNode* node);
    llvm::Type* getInferredPointerElementType(ASTNode* node);

    // Variable lookup
    llvm::AllocaInst* findVariableInMain(const std::string& name);
    llvm::GlobalVariable* findGlobalVariable(const std::string& name);
    void invalidateGlobalVarCache();
    // Wrapper for GlobalVariable creation that auto-invalidates the local cache
    llvm::GlobalVariable* createGlobalVariable(
        llvm::Module& M, llvm::Type* Ty, bool isConstant,
        llvm::GlobalValue::LinkageTypes Linkage, llvm::Constant* InitVal,
        const std::string& Name,
        llvm::GlobalVariable* InsertBefore = nullptr,
        llvm::GlobalValue::ThreadLocalMode TLMode = llvm::GlobalValue::NotThreadLocal,
        unsigned AddressSpace = 0,
        bool isExternallyInitialized = false);

    // C library init helpers
    void initStrlen();
    llvm::Function* getOrCreateStrcpyFunction();
    llvm::Function* getOrCreateStrcatFunction();
    llvm::Function* getOrCreateReallocFunction();
    llvm::Function* getOrCreateMemcpyFunction();

    // Pointer element type inference
    llvm::Type* getPointerElementTypeSafely(llvm::PointerType* ptrType, const std::string& varName);

    // Array param helpers
    bool isArrayParamPosition(const std::string& funcName, int userParamIndex);
    llvm::Value* getRuntimeArrayLengthValue(const std::string& varName);
    llvm::Value* inferArrayLengthFromArgument(ASTNode* argNode);
    llvm::AllocaInst* findRuntimeArrayLengthSlot(const std::string& varName);
    llvm::AllocaInst* ensureRuntimeArrayLengthSlot(const std::string& varName, int initialLen);

    // String concat
    llvm::Value* emitStringConcat(llvm::Value* left, llvm::Value* right);
    llvm::Value* emitStringSlice(llvm::Value* source, llvm::Value* start, llvm::Value* end);

    // Array length loading
    llvm::Value* emitLoadArrayLength(llvm::Value* dataPtr, const llvm::Twine& name = "arr_len");

    // Function pointer call
    VisitResult emitFunctionPointerCall(llvm::Value* rawCalleePtr, ASTNode* argsNode, llvm::Type* expectedReturnTypeHint = nullptr);

    // ADT helpers
    llvm::Value* getBuiltinUnionCtorTagValue(const std::string& ctorName);
    VisitResult emitAdtAlloc(int32_t tagVal, llvm::StructType* adtStructTy);

    // Constant expression evaluation
    llvm::Constant* evaluateConstExpr(ASTNode* node, ValueType* outType = nullptr);

    // Visit dispatch
    VisitResult visit(ASTNode* node);

    // Expression visitors
    VisitResult visitNumInt(ASTNode* node);
    VisitResult visitNumFloat(ASTNode* node);
    VisitResult visitString(ASTNode* node);
    VisitResult visitChar(ASTNode* node);
    VisitResult visitNil(ASTNode* node);
    VisitResult visitIdentifier(ASTNode* node);
    VisitResult visitBinOp(ASTNode* node);
    VisitResult visitUnaryOp(ASTNode* node);

    // Statement visitors
    VisitResult visitProgram(ASTNode* node);
    VisitResult visitAssign(ASTNode* node);
    VisitResult visitIf(ASTNode* node);
    VisitResult visitWhile(ASTNode* node);
    VisitResult visitFor(ASTNode* node);
    VisitResult visitBreak(ASTNode* node);
    VisitResult visitContinue(ASTNode* node);
    VisitResult visitReturn(ASTNode* node);
    VisitResult visitPrint(ASTNode* node);
    VisitResult visitInput(ASTNode* node);
    VisitResult visitConst(ASTNode* node);
    VisitResult visitGlobal(ASTNode* node);

    // Function visitors
    void preDeclareFunction(ASTNode* node);
    VisitResult visitFunction(ASTNode* node, const std::string* overrideName = nullptr);
    VisitResult visitCall(ASTNode* node);

    // Struct visitors
    VisitResult visitStructDef(ASTNode* node);
    VisitResult visitStructLiteral(ASTNode* node);
    VisitResult visitMemberAccess(ASTNode* node);
    VisitResult visitStructAssign(ASTNode* node);
    VisitResult visitMemberAssign(ASTNode* node);
    void initStructLiteral(llvm::AllocaInst* structAlloc, ASTNode* node);

    // Array visitors
    VisitResult visitIndex(ASTNode* node);
    VisitResult visitIndexAssign(ASTNode* node);
    VisitResult computeIndexPtr(ASTNode* node);
    VisitResult visitArrayLiteral(ASTNode* node);
    VisitResult handleArrayLength(ASTNode* object);

    // Local cache for GlobalVariable lookups (avoids O(n) LLVM linked-list walks)
    std::unordered_map<std::string, llvm::GlobalVariable*> global_var_cache;

public:
    LLVMCodeGenerator();

    std::unique_ptr<llvm::Module> generate(ASTNode* ast_root);
    void initPrintf();
    void createDefaultMain();
    llvm::Function* getCurrentFunction();
};

#endif
