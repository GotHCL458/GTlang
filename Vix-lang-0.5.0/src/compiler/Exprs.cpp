#include "Codegen.h"
#include <iostream>

using namespace llvm;

    LLVMCodeGenerator::VisitResult LLVMCodeGenerator::visitNumInt(ASTNode* node) {
        int64_t val = node->data.num_int.value;
        ValueType type;
        Value* value;
        
        if (node->inferred_type && node->inferred_type->kind == TYPEINFO_BOOL) {
            type = ValueType::BOOL;
            value = ConstantInt::get(Type::getInt1Ty(context), val, true);
        } else if (val >= -2147483648LL && val <= 2147483647LL) {
            type = ValueType::INT32;
            value = ConstantInt::get(Type::getInt32Ty(context), val, true);
        } else {
            type = ValueType::INT64;
            value = ConstantInt::get(Type::getInt64Ty(context), val, true);
        }
        return VisitResult(value, type);
    }
    
    LLVMCodeGenerator::VisitResult LLVMCodeGenerator::visitNumFloat(ASTNode* node) {
        double val = node->data.num_float.value;
        Value* value = ConstantFP::get(Type::getDoubleTy(context), val);
        return VisitResult(value, ValueType::FLOAT64);
    }
    
    LLVMCodeGenerator::VisitResult LLVMCodeGenerator::visitString(ASTNode* node) {
        const char* str = node ? node->data.string.value : NULL;
        if (!str) str = "";

        // At module level (no active insert block), create global string directly
        if (!builder.GetInsertBlock()) {
            Constant* strConst = ConstantDataArray::getString(context, str, true);
            createGlobalVariable(
                *module, strConst->getType(), true,
                GlobalValue::PrivateLinkage, strConst, ".str");
            // Create a pointer to the first element (i8*)
            std::vector<Constant*> indices = {
                ConstantInt::get(Type::getInt64Ty(context), 0),
                ConstantInt::get(Type::getInt64Ty(context), 0)
            };
            Constant* ptr = ConstantExpr::getGetElementPtr(
                strConst->getType(), strConst, indices, true);
            return VisitResult(ptr, ValueType::STRING);
        }

        Value* value = safeCreateGlobalString(str, "str_lit");
        return VisitResult(value, ValueType::STRING);
    }

    LLVMCodeGenerator::VisitResult LLVMCodeGenerator::visitChar(ASTNode* node) {
        if (!node) {
            Value* value = ConstantInt::get(Type::getInt8Ty(context), 0);
            return VisitResult(value, ValueType::INT8);
        }

        char ch = node->data.character.value;
        Value* value = ConstantInt::get(Type::getInt8Ty(context), static_cast<int8_t>(ch));
        return VisitResult(value, ValueType::INT8);
    }

    LLVMCodeGenerator::VisitResult LLVMCodeGenerator::visitNil(ASTNode* node) {
        (void)node;
        Type* nilPtrType = PointerType::get(context, 0);
        Value* nilValue = ConstantPointerNull::get(cast<PointerType>(nilPtrType));
        return VisitResult(nilValue, ValueType::POINTER);
    }

// Shared helper: heap-allocate an ADT tagged struct (tag + nullable payload pointer)
LLVMCodeGenerator::VisitResult LLVMCodeGenerator::emitAdtAlloc(int32_t tagVal, StructType *adtStructTy) {
    Function *reallocFn = getOrCreateReallocFunction();
    uint64_t allocSize = module->getDataLayout().getTypeAllocSize(adtStructTy);
    Value *heapBytes = ConstantInt::get(Type::getInt64Ty(context), allocSize);
    Value *heapI8 = builder.CreateCall(reallocFn,
        {ConstantPointerNull::get(PointerType::get(context, 0)), heapBytes}, "adt_heap");
    Value *adtPtr = builder.CreateBitCast(heapI8, PointerType::get(context, 0), "adt_ptr");
    Value *tagPtr = builder.CreateStructGEP(adtStructTy, adtPtr, 0, "tag_ptr");
    builder.CreateStore(ConstantInt::get(Type::getInt32Ty(context), tagVal), tagPtr);
    Value *payloadPtr = builder.CreateStructGEP(adtStructTy, adtPtr, 1, "payload_ptr");
    builder.CreateStore(ConstantPointerNull::get(cast<PointerType>(Type::getInt8Ty(context)->getPointerTo())), payloadPtr);
    pointerElementHints[adtPtr] = adtStructTy;
    return VisitResult(adtPtr, ValueType::POINTER, adtStructTy);
}

    LLVMCodeGenerator::VisitResult LLVMCodeGenerator::visitIdentifier(ASTNode* node) {
        if (!node || !node->data.identifier.name) return VisitResult();
        
        std::string name(node->data.identifier.name);
        Type* i32Ty = Type::getInt32Ty(context);
        StructType* adtStructTy = StructType::get(context, {i32Ty, PointerType::get(context, 0)});

        if (name == "None") {
            return emitAdtAlloc(1, adtStructTy);
        }
        if (name == "Some" || name == "Ok" || name == "Err") {
            return VisitResult(getBuiltinUnionCtorTagValue(name), ValueType::INT32);
        }
        if (isRegisteredUnionCtorName(name)) {
            GlobalVariable* gv = module->getGlobalVariable(name, true);
            int32_t tagVal = 0;
            if (gv && gv->hasInitializer()) {
                if (auto* intInit = dyn_cast<ConstantInt>(gv->getInitializer())) {
                    tagVal = static_cast<int32_t>(intInit->getSExtValue());
                }
            } else {
                int ctor_idx = vix_adt_ctor_index(name.c_str());
                tagVal = (ctor_idx >= 0) ? static_cast<int32_t>(ctor_idx) : ctorTagValue(name);
            }
            /* Create tagged struct for no-payload constructor */
            return emitAdtAlloc(tagVal, adtStructTy);
        }
        AllocaInst* alloc = scopeManager.findVariable(name);
        Function* curFnForScope = getCurrentFunction();
        if (alloc && curFnForScope && alloc->getFunction() != curFnForScope) {
            reportCodegenSemanticError(node, "capturing local variables from outer functions is not supported yet");
            alloc = nullptr;
        }
        
        if (!alloc) {
            if (Function* fn = module->getFunction(name)) {
                return VisitResult(fn, ValueType::POINTER);
            }

            Function* curFn = getCurrentFunction();
            bool inMain = (curFn && curFn->getName() == "main");
            alloc = inMain ? findVariableInMain(name) : nullptr;
            if (alloc) {
                scopeManager.defineVariable(name, alloc);
            } else {
                if (!inMain && findVariableInMain(name)) {
                    reportCodegenSemanticError(node, "capturing local variables from outer functions is not supported yet");
                    return VisitResult(ConstantInt::get(Type::getInt32Ty(context), 0), ValueType::INT32);
                }
                GlobalVariable* globalVar = findGlobalVariable(name);
                if (globalVar) {
                    Type* globalType = globalVar->getValueType();
                    if (globalType && globalType->isStructTy()) {
                        StructType* st = cast<StructType>(globalType);
                        return VisitResult(globalVar, ValueType::POINTER, st);
                    }
                    Value* loadedValue = builder.CreateLoad(globalType, globalVar, name);
                    ValueType valueType = typeHelper.getValueTypeFromType(globalType);
                    return VisitResult(loadedValue, valueType);
                } else {
                    llvm::errs() << "Warning: Use of undeclared variable '" << name << "'\n";
                    return VisitResult(ConstantInt::get(Type::getInt32Ty(context), 0), ValueType::INT32);
                }
            }
        }
        
        Type* allocatedType = getActualType(alloc);
        
        // 检查是否是字符串变量
        bool isStringVar = typeHelper.isStringVariable(name);
        
        if (allocatedType && (allocatedType->isStructTy() || allocatedType->isArrayTy())) {
            if (allocatedType->isStructTy()) {
                StructType* st = cast<StructType>(allocatedType);
                return VisitResult(alloc, ValueType::POINTER, st);
            }
            return VisitResult(alloc, ValueType::POINTER);
        }
        
        // 如果是字符串变量，需要正确处理
        if (isStringVar) {
            VIX_DEBUG_LOG << "[DEBUG] String variable '" << name << "' loading value\n";
            Value* val = builder.CreateLoad(allocatedType, alloc, name);
            // 对于字符串，返回的是指向字符数组或字符指针的值
            ValueType vt = typeHelper.getValueTypeFromType(allocatedType);
            return VisitResult(val, vt);
        }

        Value* val = builder.CreateLoad(allocatedType, alloc, name);
        ValueType type = typeHelper.getValueTypeFromType(allocatedType);
        if (node->inferred_type && node->inferred_type->kind == TYPEINFO_PTR) {
            Type* elemType = getLLVMTypeFromTypeInfo(node->inferred_type->element);
            if (elemType) {
                pointerElementHints[val] = elemType;
            }
        }
        if (node->inferred_type && node->inferred_type->kind == TYPEINFO_STRUCT && node->inferred_type->name) {
            if (vix_is_adt_definition(node->inferred_type->name)) {
                StructType* adtStructTy = StructType::get(context, {Type::getInt32Ty(context), PointerType::get(context, 0)});
                pointerElementHints[val] = adtStructTy;
            } else {
                StructType* st = typeHelper.getStructType(node->inferred_type->name);
                if (st) {
                    pointerElementHints[val] = st;
                }
            }
        }
        if (node->inferred_type && node->inferred_type->kind == TYPEINFO_APP) {
            if (node->inferred_type->app_ctor && node->inferred_type->app_ctor->kind == TYPEINFO_STRUCT &&
                node->inferred_type->app_ctor->name) {
                if (vix_is_adt_definition(node->inferred_type->app_ctor->name)) {
                    StructType* adtStructTy = StructType::get(context, {Type::getInt32Ty(context), PointerType::get(context, 0)});
                    pointerElementHints[val] = adtStructTy;
                } else {
                    StructType* st = typeHelper.getStructType(node->inferred_type->app_ctor->name);
                    if (st) {
                        pointerElementHints[val] = st;
                    }
                }
            }
        }
        // Propagate pointerElementHints from the alloca to the loaded value
        // This ensures ADT struct types (Option/Result/Custom) are tracked through variables
        auto allocHintIt = pointerElementHints.find(alloc);
        if (allocHintIt != pointerElementHints.end() && allocHintIt->second) {
            pointerElementHints[val] = allocHintIt->second;
        }
        return VisitResult(val, type);
    }
    
    // ==================== 修复：visitBinOp - 确保比较操作类型一致 ====================
    LLVMCodeGenerator::VisitResult LLVMCodeGenerator::visitBinOp(ASTNode* node) {
        if (!node || !node->data.binop.left || !node->data.binop.right)
            return VisitResult();
        
        /* For comparisons involving ADT constructors, use tag value (i32) instead of pointer */
        BinOpType op = node->data.binop.op;
        if (op == OP_EQ || op == OP_NE) {
            ASTNode* left_node = node->data.binop.left;
            ASTNode* right_node = node->data.binop.right;
            auto getAdtTagValue = [&](ASTNode* n) -> Value* {
                if (!n || n->type != AST_IDENTIFIER || !n->data.identifier.name) return nullptr;
                std::string name(n->data.identifier.name);
                if (!isRegisteredUnionCtorName(name)) return nullptr;
                GlobalVariable* gv = module->getGlobalVariable(name, true);
                int32_t tagVal = 0;
                if (gv && gv->hasInitializer()) {
                    if (auto* intInit = dyn_cast<ConstantInt>(gv->getInitializer())) {
                        tagVal = static_cast<int32_t>(intInit->getSExtValue());
                    }
                } else {
                    int ctor_idx = vix_adt_ctor_index(name.c_str());
                    tagVal = (ctor_idx >= 0) ? static_cast<int32_t>(ctor_idx) : ctorTagValue(name);
                }
                return ConstantInt::get(Type::getInt32Ty(context), tagVal);
            };
            Value* leftAdtTag = getAdtTagValue(left_node);
            Value* rightAdtTag = getAdtTagValue(right_node);
            if (leftAdtTag || rightAdtTag) {
                VisitResult leftRes = leftAdtTag ? VisitResult(leftAdtTag, ValueType::INT32) : visit(left_node);
                VisitResult rightRes = rightAdtTag ? VisitResult(rightAdtTag, ValueType::INT32) : visit(right_node);
                if (!leftRes.value || !rightRes.value) return VisitResult();
                Value* lv = leftRes.value;
                Value* rv = rightRes.value;
                if (lv->getType() != rv->getType()) {
                    if (lv->getType()->isIntegerTy() && rv->getType()->isIntegerTy()) {
                        unsigned maxBits = std::max(lv->getType()->getIntegerBitWidth(),
                                                    rv->getType()->getIntegerBitWidth());
                        lv = builder.CreateIntCast(lv, Type::getIntNTy(context, maxBits), true);
                        rv = builder.CreateIntCast(rv, Type::getIntNTy(context, maxBits), true);
                    }
                }
                if (op == OP_EQ) return VisitResult(builder.CreateICmpEQ(lv, rv, "eqtmp"), ValueType::BOOL);
                return VisitResult(builder.CreateICmpNE(lv, rv, "netmp"), ValueType::BOOL);
            }
        }

        auto toBoolValue = [&](Value* v, ValueType vt, const char* name) -> Value* {
            if (vt == ValueType::BOOL && v->getType()->isIntegerTy(1)) {
                return v;
            }
            Type* ty = v->getType();
            if (ty->isIntegerTy()) {
                return builder.CreateICmpNE(v, ConstantInt::get(ty, 0), name);
            }
            if (ty->isFloatingPointTy()) {
                return builder.CreateFCmpONE(v, ConstantFP::get(ty, 0.0), name);
            }
            if (ty->isPointerTy()) {
                return builder.CreateICmpNE(v, ConstantPointerNull::get(cast<PointerType>(ty)), name);
            }
            return ConstantInt::getFalse(context);
        };

        /* Short-circuit evaluation for AND/OR: only evaluate right operand if needed */
        if (op == OP_AND || op == OP_OR) {
            VisitResult leftRes = visit(node->data.binop.left);
            if (!leftRes.value) return VisitResult();
            Value* leftBool = toBoolValue(leftRes.value, leftRes.type, op == OP_AND ? "and_lhs" : "or_lhs");

            Function* curFn = builder.GetInsertBlock()->getParent();
            BasicBlock* evalRightBB = BasicBlock::Create(context, op == OP_AND ? "and.rhs" : "or.rhs", curFn);
            BasicBlock* mergeBB = BasicBlock::Create(context, op == OP_AND ? "and.merge" : "or.merge", curFn);
            BasicBlock* shortcutBB = BasicBlock::Create(context, op == OP_AND ? "and.false" : "or.true", curFn);

            if (op == OP_AND) {
                builder.CreateCondBr(leftBool, evalRightBB, shortcutBB);
            } else {
                builder.CreateCondBr(leftBool, shortcutBB, evalRightBB);
            }

            builder.SetInsertPoint(evalRightBB);
            VisitResult rightRes = visit(node->data.binop.right);
            BasicBlock* rightEndBB = builder.GetInsertBlock();
            if (!rightRes.value) rightRes = VisitResult(ConstantInt::get(Type::getInt32Ty(context), 0), ValueType::INT32);
            Value* rightBool = toBoolValue(rightRes.value, rightRes.type, op == OP_AND ? "and_rhs" : "or_rhs");
            bool rightReachedMerge = !rightEndBB->getTerminator();
            if (rightReachedMerge)
                builder.CreateBr(mergeBB);

            builder.SetInsertPoint(shortcutBB);
            Value* shortcutVal = op == OP_AND ? ConstantInt::getFalse(context) : ConstantInt::getTrue(context);
            builder.CreateBr(mergeBB);

            builder.SetInsertPoint(mergeBB);
            if (rightReachedMerge) {
                PHINode* phi = builder.CreatePHI(Type::getInt1Ty(context), 2, op == OP_AND ? "andtmp" : "ortmp");
                if (op == OP_AND) {
                    phi->addIncoming(rightBool, rightEndBB);
                    phi->addIncoming(shortcutVal, shortcutBB);
                } else {
                    phi->addIncoming(shortcutVal, shortcutBB);
                    phi->addIncoming(rightBool, rightEndBB);
                }
                return VisitResult(phi, ValueType::BOOL);
            } else {
                return VisitResult(shortcutVal, ValueType::BOOL);
            }
        }

        VisitResult leftRes = visit(node->data.binop.left);
        VisitResult rightRes = visit(node->data.binop.right);
        if (!leftRes.value || !rightRes.value) {
            return VisitResult(ConstantInt::get(Type::getInt32Ty(context), 0), ValueType::INT32);
        }
        
        // 检查操作数类型并确保它们兼容
        // 特别处理 i8 类型的比较操作
        if ((leftRes.type == ValueType::INT8 || rightRes.type == ValueType::INT8) &&
            leftRes.value->getType()->isIntegerTy() && rightRes.value->getType()->isIntegerTy() &&
            (node->data.binop.op == OP_EQ || node->data.binop.op == OP_NE ||
             node->data.binop.op == OP_LT || node->data.binop.op == OP_LE ||
             node->data.binop.op == OP_GT || node->data.binop.op == OP_GE)) {
            
            // 对于比较操作，将两个操作数都提升到 i32 以确保类型一致
            Value* leftVal = leftRes.value;
            Value* rightVal = rightRes.value;
            
            // 确保两个操作数都是整数类型
            if (!leftVal->getType()->isIntegerTy() || !rightVal->getType()->isIntegerTy()) {
                llvm::errs() << "Error: Comparison operands must be integers\n";
                return VisitResult();
            }
            
            // 将两个操作数都扩展到 i32（如果它们不是 i32）
            if (leftVal->getType() != Type::getInt32Ty(context)) {
                if (leftVal->getType()->isIntegerTy(1)) {
                    leftVal = builder.CreateZExt(leftVal, Type::getInt32Ty(context), "left_zext");
                } else if (leftVal->getType()->getIntegerBitWidth() < 32) {
                    leftVal = builder.CreateSExt(leftVal, Type::getInt32Ty(context), "left_sext");
                }
            }
            
            if (rightVal->getType() != Type::getInt32Ty(context)) {
                if (rightVal->getType()->isIntegerTy(1)) {
                    rightVal = builder.CreateZExt(rightVal, Type::getInt32Ty(context), "right_zext");
                } else if (rightVal->getType()->getIntegerBitWidth() < 32) {
                    rightVal = builder.CreateSExt(rightVal, Type::getInt32Ty(context), "right_sext");
                }
            }
            
            // 执行比较操作
            switch (node->data.binop.op) {
                case OP_EQ:
                    return VisitResult(builder.CreateICmpEQ(leftVal, rightVal, "eqtmp"), ValueType::BOOL);
                case OP_NE:
                    return VisitResult(builder.CreateICmpNE(leftVal, rightVal, "netmp"), ValueType::BOOL);
                case OP_LT:
                    return VisitResult(builder.CreateICmpSLT(leftVal, rightVal, "lttmp"), ValueType::BOOL);
                case OP_LE:
                    return VisitResult(builder.CreateICmpSLE(leftVal, rightVal, "letmp"), ValueType::BOOL);
                case OP_GT:
                    return VisitResult(builder.CreateICmpSGT(leftVal, rightVal, "gttmp"), ValueType::BOOL);
                case OP_GE:
                    return VisitResult(builder.CreateICmpSGE(leftVal, rightVal, "getmp"), ValueType::BOOL);
                default:
                    return VisitResult();
            }
        }

        
        if (node->data.binop.op == OP_ADD || node->data.binop.op == OP_SUB) {//ptr +/- int, int + ptr
            bool leftIsPtr = leftRes.value->getType()->isPointerTy();
            bool rightIsPtr = rightRes.value->getType()->isPointerTy();
            bool leftIsInt = leftRes.value->getType()->isIntegerTy();
            bool rightIsInt = rightRes.value->getType()->isIntegerTy();
            if ((leftIsPtr && rightIsInt) ||
                (node->data.binop.op == OP_ADD && rightIsPtr && leftIsInt)) {
                Value* ptrVal = leftIsPtr ? leftRes.value : rightRes.value;
                Value* idxVal = leftIsPtr ? rightRes.value : leftRes.value;
                if (!idxVal->getType()->isIntegerTy(64)) {
                    idxVal = builder.CreateIntCast(idxVal, Type::getInt64Ty(context), true, "ptr_idx_cast");
                }
                if (node->data.binop.op == OP_SUB) {
                    idxVal = builder.CreateNeg(idxVal, "ptr_idx_neg");
                }
                std::string ptrName;
                ASTNode* ptrExpr = leftIsPtr ? node->data.binop.left : node->data.binop.right;
                if (ptrExpr && ptrExpr->type == AST_IDENTIFIER && ptrExpr->data.identifier.name) {
                    ptrName = std::string(ptrExpr->data.identifier.name);
                }
                Type* elemType = getPointerElementTypeSafely(
                    dyn_cast<PointerType>(ptrVal->getType()), ptrName);
                if (!elemType) {
                    elemType = Type::getInt32Ty(context);
                }
                Type* expectPtrType = PointerType::get(context, 0);
                if (ptrVal->getType() != expectPtrType) {
                    ptrVal = builder.CreateBitCast(ptrVal, expectPtrType, "ptr_arith_cast");
                }
                Value* gep = builder.CreateInBoundsGEP(elemType, ptrVal, idxVal, "ptr_arith");
                return VisitResult(gep, ValueType::POINTER);
            }
        }

        if (node->data.binop.op == OP_ADD) {
            bool leftIsString = (leftRes.type == ValueType::STRING) ||
                                (node->data.binop.left && node->data.binop.left->type == AST_STRING);
            bool rightIsString = (rightRes.type == ValueType::STRING) ||
                                 (node->data.binop.right && node->data.binop.right->type == AST_STRING);
            if (leftIsString && rightIsString &&
                leftRes.value->getType()->isPointerTy() && rightRes.value->getType()->isPointerTy()) {
                Value* concat = emitStringConcat(leftRes.value, rightRes.value);
                if (concat) {
                    return VisitResult(concat, ValueType::STRING);
                }
            }
        }

        bool isCompareOp = (node->data.binop.op == OP_EQ || node->data.binop.op == OP_NE ||
                            node->data.binop.op == OP_LT || node->data.binop.op == OP_LE ||
                            node->data.binop.op == OP_GT || node->data.binop.op == OP_GE);
        // String comparison: use strcmp instead of pointer comparison
        if (isCompareOp && leftRes.value->getType()->isPointerTy() && rightRes.value->getType()->isPointerTy()) {
            bool leftIsString = (leftRes.type == ValueType::STRING) ||
                                (node->data.binop.left && node->data.binop.left->type == AST_STRING);
            bool rightIsString = (rightRes.type == ValueType::STRING) ||
                                 (node->data.binop.right && node->data.binop.right->type == AST_STRING);
            if (leftIsString && rightIsString) {
                // Declare strcmp if needed
                Function* strcmpFn = module->getFunction("strcmp");
                if (!strcmpFn) {
                    Type* i8Ptr = PointerType::get(context, 0);
                    FunctionType* strcmpTy = FunctionType::get(Type::getInt32Ty(context), {i8Ptr, i8Ptr}, false);
                    strcmpFn = Function::Create(strcmpTy, Function::ExternalLinkage, "strcmp", module.get());
                }
                Value* cmpResult = builder.CreateCall(strcmpFn, {leftRes.value, rightRes.value}, "strcmp_res");
                Value* zero = ConstantInt::get(Type::getInt32Ty(context), 0);
                switch (node->data.binop.op) {
                    case OP_EQ: return VisitResult(builder.CreateICmpEQ(cmpResult, zero, "streq"), ValueType::BOOL);
                    case OP_NE: return VisitResult(builder.CreateICmpNE(cmpResult, zero, "strneq"), ValueType::BOOL);
                    case OP_LT: return VisitResult(builder.CreateICmpSLT(cmpResult, zero, "strlt"), ValueType::BOOL);
                    case OP_LE: return VisitResult(builder.CreateICmpSLE(cmpResult, zero, "strle"), ValueType::BOOL);
                    case OP_GT: return VisitResult(builder.CreateICmpSGT(cmpResult, zero, "strgt"), ValueType::BOOL);
                    case OP_GE: return VisitResult(builder.CreateICmpSGE(cmpResult, zero, "strge"), ValueType::BOOL);
                    default: break;
                }
            }
        }
        if (isCompareOp && (leftRes.value->getType()->isPointerTy() || rightRes.value->getType()->isPointerTy())) {
            Value* leftVal = leftRes.value;
            Value* rightVal = rightRes.value;
            Type* leftTy = leftVal->getType();
            Type* rightTy = rightVal->getType();

            if (leftTy->isPointerTy() && rightTy->isPointerTy()) {
                if (leftTy != rightTy) {
                    rightVal = builder.CreateBitCast(rightVal, leftTy, "cmp_ptr_cast");
                }
            } else if (leftTy->isPointerTy() && rightTy->isIntegerTy()) {
                if (auto* ci = dyn_cast<ConstantInt>(rightVal); ci && ci->isZero()) {
                    rightVal = ConstantPointerNull::get(cast<PointerType>(leftTy));
                } else {
                    if (!rightTy->isIntegerTy(64)) {
                        rightVal = builder.CreateIntCast(rightVal, Type::getInt64Ty(context), false, "cmp_int_cast64");
                    }
                    rightVal = builder.CreateIntToPtr(rightVal, cast<PointerType>(leftTy), "cmp_int_to_ptr");
                }
            } else if (rightTy->isPointerTy() && leftTy->isIntegerTy()) {
                if (auto* ci = dyn_cast<ConstantInt>(leftVal); ci && ci->isZero()) {
                    leftVal = ConstantPointerNull::get(cast<PointerType>(rightTy));
                } else {
                    if (!leftTy->isIntegerTy(64)) {
                        leftVal = builder.CreateIntCast(leftVal, Type::getInt64Ty(context), false, "cmp_int_cast64");
                    }
                    leftVal = builder.CreateIntToPtr(leftVal, cast<PointerType>(rightTy), "cmp_int_to_ptr");
                }
            }

            switch (node->data.binop.op) {
                case OP_EQ:
                    return VisitResult(builder.CreateICmpEQ(leftVal, rightVal, "eqtmp"), ValueType::BOOL);
                case OP_NE:
                    return VisitResult(builder.CreateICmpNE(leftVal, rightVal, "netmp"), ValueType::BOOL);
                case OP_LT:
                    return VisitResult(builder.CreateICmpULT(leftVal, rightVal, "lttmp"), ValueType::BOOL);
                case OP_LE:
                    return VisitResult(builder.CreateICmpULE(leftVal, rightVal, "letmp"), ValueType::BOOL);
                case OP_GT:
                    return VisitResult(builder.CreateICmpUGT(leftVal, rightVal, "gttmp"), ValueType::BOOL);
                case OP_GE:
                    return VisitResult(builder.CreateICmpUGE(leftVal, rightVal, "getmp"), ValueType::BOOL);
                default:
                    return VisitResult();
            }
        }

        auto [promotedLeftType, promotedRightType] = typeHelper.promoteTypes(
            leftRes.type, rightRes.type);
        
        Value* leftVal = typeHelper.castValue(builder, leftRes.value, 
                                              leftRes.type, promotedLeftType);
        Value* rightVal = typeHelper.castValue(builder, rightRes.value, 
                                               rightRes.type, promotedRightType);
        
        ValueType resultType = (promotedLeftType > promotedRightType) 
                               ? promotedLeftType : promotedRightType;
        bool isFloat = (resultType == ValueType::FLOAT32 || resultType == ValueType::FLOAT64);

        bool isArithmeticOp = (node->data.binop.op == OP_ADD || node->data.binop.op == OP_SUB ||
                               node->data.binop.op == OP_MUL || node->data.binop.op == OP_DIV ||
                               node->data.binop.op == OP_MOD || node->data.binop.op == OP_POW);
        if (isArithmeticOp && !isFloat) {
            if (!leftVal->getType()->isIntegerTy() || !rightVal->getType()->isIntegerTy()) {
                reportCodegenSemanticError(node, "arithmetic operators require numeric operands");
                return VisitResult();
            }
        }
        
        if (!isFloat && leftVal->getType() != rightVal->getType()) {
            if (leftVal->getType()->isIntegerTy() && rightVal->getType()->isIntegerTy()) {
                if (leftVal->getType()->getIntegerBitWidth() > rightVal->getType()->getIntegerBitWidth()) {
                    rightVal = builder.CreateIntCast(rightVal, leftVal->getType(), true, "arith_cast");
                } else if (rightVal->getType()->getIntegerBitWidth() > leftVal->getType()->getIntegerBitWidth()) {
                    leftVal = builder.CreateIntCast(leftVal, rightVal->getType(), true, "arith_cast");
                }
            }
        }

        switch (node->data.binop.op) {
            case OP_ADD:
                if (isFloat) return VisitResult(builder.CreateFAdd(leftVal, rightVal, "addtmp"), resultType);
                else return VisitResult(builder.CreateAdd(leftVal, rightVal, "addtmp"), resultType);
            case OP_SUB:
                if (isFloat) return VisitResult(builder.CreateFSub(leftVal, rightVal, "subtmp"), resultType);
                else return VisitResult(builder.CreateSub(leftVal, rightVal, "subtmp"), resultType);
            case OP_MUL:
                if (isFloat) return VisitResult(builder.CreateFMul(leftVal, rightVal, "multmp"), resultType);
                else return VisitResult(builder.CreateMul(leftVal, rightVal, "multmp"), resultType);
            case OP_DIV:
                if (isFloat) return VisitResult(builder.CreateFDiv(leftVal, rightVal, "divtmp"), resultType);
                else return VisitResult(builder.CreateSDiv(leftVal, rightVal, "divtmp"), resultType);
            case OP_MOD:
                if (isFloat) {
                    Type* dblTy = Type::getDoubleTy(context);
                    Value* lhs = (resultType == ValueType::FLOAT32)
                        ? builder.CreateFPExt(leftVal, dblTy, "mod_lhs") : leftVal;
                    Value* rhs = (resultType == ValueType::FLOAT32)
                        ? builder.CreateFPExt(rightVal, dblTy, "mod_rhs") : rightVal;
                    Function* fmodFn = module->getFunction("fmod");
                    if (!fmodFn) {
                        FunctionType* fmodTy = FunctionType::get(dblTy, {dblTy, dblTy}, false);
                        fmodFn = Function::Create(fmodTy, Function::ExternalLinkage, "fmod", module.get());
                    }
                    Value* fmodResult = builder.CreateCall(fmodFn, {lhs, rhs}, "modtmp");
                    if (resultType == ValueType::FLOAT32) {
                        return VisitResult(builder.CreateFPTrunc(fmodResult, Type::getFloatTy(context), "mod_f32"), resultType);
                    }
                    return VisitResult(fmodResult, resultType);
                }
                else return VisitResult(builder.CreateSRem(leftVal, rightVal, "modtmp"), resultType);
            case OP_POW:
                {
                    if (isFloat) {
                        // Float power: call libc pow() via declaration
                        Type* dblTy = Type::getDoubleTy(context);
                        Value* baseD = (resultType == ValueType::FLOAT32)
                            ? builder.CreateFPExt(leftVal, dblTy, "pow_base") : leftVal;
                        Value* expD = (resultType == ValueType::FLOAT32)
                            ? builder.CreateFPExt(rightVal, dblTy, "pow_exp") : rightVal;
                        Function* powFn = module->getFunction("pow");
                        if (!powFn) {
                            FunctionType* powTy = FunctionType::get(dblTy, {dblTy, dblTy}, false);
                            powFn = Function::Create(powTy, Function::ExternalLinkage, "pow", module.get());
                        }
                        Value* powResult = builder.CreateCall(powFn, {baseD, expD}, "powtmp");
                        if (resultType == ValueType::FLOAT32) {
                            return VisitResult(builder.CreateFPTrunc(powResult, Type::getFloatTy(context), "pow_f32"), resultType);
                        }
                        return VisitResult(powResult, resultType);
                    } else {
                        // Integer power: loop-based implementation
                        Function* curFn = builder.GetInsertBlock()->getParent();
                        Type* valTy = leftVal->getType();
                        Value* resultAlloca = builder.CreateAlloca(valTy, nullptr, "pow_result");
                        Value* baseAlloca = builder.CreateAlloca(valTy, nullptr, "pow_base");
                        Value* expAlloca = builder.CreateAlloca(valTy, nullptr, "pow_exp");
                        Value* one = ConstantInt::get(valTy, 1);
                        Value* zero = ConstantInt::get(valTy, 0);
                        builder.CreateStore(one, resultAlloca);
                        builder.CreateStore(leftVal, baseAlloca);
                        builder.CreateStore(rightVal, expAlloca);

                        BasicBlock* condBB = BasicBlock::Create(context, "pow_cond", curFn);
                        BasicBlock* bodyBB = BasicBlock::Create(context, "pow_body", curFn);
                        BasicBlock* afterBB = BasicBlock::Create(context, "pow_after", curFn);

                        builder.CreateBr(condBB);
                        builder.SetInsertPoint(condBB);
                        Value* curExp = builder.CreateLoad(valTy, expAlloca, "cur_exp");
                        Value* cond = builder.CreateICmpSGT(curExp, zero, "pow_cmp");
                        builder.CreateCondBr(cond, bodyBB, afterBB);

                        builder.SetInsertPoint(bodyBB);
                        Value* curBase = builder.CreateLoad(valTy, baseAlloca, "cur_base");
                        Value* curResult = builder.CreateLoad(valTy, resultAlloca, "cur_res");
                        Value* bit = builder.CreateAnd(curExp, one, "pow_bit");
                        Value* needMul = builder.CreateICmpNE(bit, zero, "pow_needmul");
                        Value* newResult = builder.CreateSelect(needMul,
                            builder.CreateMul(curResult, curBase, "pow_mul"), curResult, "pow_sel");
                        builder.CreateStore(newResult, resultAlloca);
                        Value* newBase = builder.CreateMul(curBase, curBase, "pow_sq");
                        builder.CreateStore(newBase, baseAlloca);
                        Value* newExp = builder.CreateAShr(curExp, one, "pow_shr");
                        builder.CreateStore(newExp, expAlloca);
                        builder.CreateBr(condBB);

                        builder.SetInsertPoint(afterBB);
                        Value* finalResult = builder.CreateLoad(valTy, resultAlloca, "pow_final");
                        return VisitResult(finalResult, resultType);
                    }
                }
            case OP_EQ:
                if (isFloat) return VisitResult(builder.CreateFCmpOEQ(leftVal, rightVal, "eqtmp"), ValueType::BOOL);
                else return VisitResult(builder.CreateICmpEQ(leftVal, rightVal, "eqtmp"), ValueType::BOOL);
            case OP_NE:
                if (isFloat) return VisitResult(builder.CreateFCmpONE(leftVal, rightVal, "netmp"), ValueType::BOOL);
                else return VisitResult(builder.CreateICmpNE(leftVal, rightVal, "netmp"), ValueType::BOOL);
            case OP_LT:
                if (isFloat) return VisitResult(builder.CreateFCmpOLT(leftVal, rightVal, "lttmp"), ValueType::BOOL);
                else return VisitResult(builder.CreateICmpSLT(leftVal, rightVal, "lttmp"), ValueType::BOOL);
            case OP_LE:
                if (isFloat) return VisitResult(builder.CreateFCmpOLE(leftVal, rightVal, "letmp"), ValueType::BOOL);
                else return VisitResult(builder.CreateICmpSLE(leftVal, rightVal, "letmp"), ValueType::BOOL);
            case OP_GT:
                if (isFloat) return VisitResult(builder.CreateFCmpOGT(leftVal, rightVal, "gttmp"), ValueType::BOOL);
                else return VisitResult(builder.CreateICmpSGT(leftVal, rightVal, "gttmp"), ValueType::BOOL);
            case OP_GE:
                if (isFloat) return VisitResult(builder.CreateFCmpOGE(leftVal, rightVal, "getmp"), ValueType::BOOL);
                else return VisitResult(builder.CreateICmpSGE(leftVal, rightVal, "getmp"), ValueType::BOOL);
            case OP_AND:
            case OP_OR:
                return VisitResult();
            default: return VisitResult();
        }
    }

    LLVMCodeGenerator::VisitResult LLVMCodeGenerator::visitUnaryOp(ASTNode* node) {
        if (!node || !node->data.unaryop.expr) return VisitResult();

        if (node->data.unaryop.op == OP_ADDRESS) {
            ASTNode* expr = node->data.unaryop.expr;
            if (expr->type == AST_STRUCT_LITERAL) {
                VisitResult structRes = visit(expr);
                if (structRes.value) {
                    return structRes;
                }
                return VisitResult();
            }
            if (expr->type == AST_IDENTIFIER && expr->data.identifier.name) {
                std::string name(expr->data.identifier.name);
                AllocaInst* alloc = scopeManager.findVariable(name);
                if (!alloc) alloc = findVariableInMain(name);
                if (alloc) {
                    return VisitResult(alloc, ValueType::POINTER);
                }

                GlobalVariable* gvar = findGlobalVariable(name);
                if (gvar) {
                    return VisitResult(gvar, ValueType::POINTER);
                }

                std::cerr << "Warning: Address-of operator applied to undefined variable '"
                          << name << "'" << std::endl;
            }

            if (expr->type == AST_MEMBER_ACCESS) {
                ASTNode* object = expr->data.member_access.object;
                ASTNode* field = expr->data.member_access.field;
                if (!object || !field || field->type != AST_IDENTIFIER) {
                    return VisitResult();
                }

                std::string fieldName(field->data.identifier.name);
                VisitResult objectRes = visit(object);
                if (!objectRes.value) {
                    return VisitResult();
                }

                StructType* structType = nullptr;
                Value* basePtr = nullptr;

                if (objectRes.structType) {
                    structType = objectRes.structType;
                    basePtr = objectRes.value;
                }

                if (!structType && object->type == AST_IDENTIFIER && object->data.identifier.name) {
                    std::string objName(object->data.identifier.name);
                    AllocaInst* alloc = scopeManager.findVariable(objName);
                    if (!alloc) alloc = findVariableInMain(objName);
                    if (alloc) {
                        Type* allocType = getActualType(alloc);
                        if (allocType && allocType->isStructTy()) {
                            structType = cast<StructType>(allocType);
                            basePtr = alloc;
                        }
                    }
                }

                if (!structType || !basePtr) {
                    return VisitResult();
                }

                std::string structName = structType->getName().str();
                int idx = typeHelper.getFieldIndex(structName, fieldName);
                if (idx < 0) {
                    return VisitResult();
                }

                Value* fieldPtr = builder.CreateStructGEP(structType, basePtr, idx, fieldName + "_addr");
                Type* fieldType = structType->getElementType(idx);
                if (fieldType->isStructTy()) {
                    return VisitResult(fieldPtr, ValueType::POINTER, cast<StructType>(fieldType));
                }
                return VisitResult(fieldPtr, ValueType::POINTER);
            }

            if (expr->type == AST_INDEX && expr->data.index.target && expr->data.index.index) {
                ASTNode* target = expr->data.index.target;
                ASTNode* indexExpr = expr->data.index.index;

                VisitResult idxRes = visit(indexExpr);
                if (!idxRes.value) return VisitResult();
                Value* idxVal = idxRes.value;
                if (!idxVal->getType()->isIntegerTy(32)) {
                    idxVal = builder.CreateIntCast(idxVal, Type::getInt32Ty(context), true, "addr_idx_cast");
                }

                AllocaInst* baseAlloc = nullptr;
                std::string varName;
                if (target->type == AST_IDENTIFIER && target->data.identifier.name) {
                    varName = std::string(target->data.identifier.name);
                    baseAlloc = scopeManager.findVariable(varName);
                    if (!baseAlloc) baseAlloc = findVariableInMain(varName);
                }

                if (baseAlloc) {
                    Type* allocatedType = getActualType(baseAlloc);
                    if (allocatedType && allocatedType->isArrayTy()) {
                        Value* gep = builder.CreateInBoundsGEP(
                            allocatedType,
                            baseAlloc,
                            {ConstantInt::get(Type::getInt32Ty(context), 0), idxVal},
                            "addr_arr_index_ptr");
                        return VisitResult(gep, ValueType::POINTER);
                    }

                    if (allocatedType && allocatedType->isPointerTy()) {
                        Value* arrayPtr = builder.CreateLoad(allocatedType, baseAlloc, "addr_array_ptr");
                        Type* elemType = getPointerElementTypeSafely(
                            dyn_cast<PointerType>(allocatedType), varName);
                        Value* gep = builder.CreateInBoundsGEP(elemType, arrayPtr, idxVal, "addr_ptr_index_ptr");
                        return VisitResult(gep, ValueType::POINTER);
                    }
                }

                VisitResult targetRes = visit(target);
                if (!targetRes.value) return VisitResult();
                if (targetRes.value->getType()->isPointerTy()) {
                    Type* elemType = getPointerElementTypeSafely(
                        dyn_cast<PointerType>(targetRes.value->getType()), varName);
                    Value* gep = builder.CreateInBoundsGEP(elemType, targetRes.value, idxVal, "addr_idx_ptr");
                    return VisitResult(gep, ValueType::POINTER);
                }
            }
            return VisitResult();//no valid addr target
        }

        VisitResult operand = visit(node->data.unaryop.expr);
        if (!operand.value) return VisitResult();

        switch (node->data.unaryop.op) {
            case OP_MINUS:
                if (operand.type == ValueType::FLOAT32 || operand.type == ValueType::FLOAT64)
                    return VisitResult(builder.CreateFNeg(operand.value, "negtmp"), operand.type);
                else
                    return VisitResult(builder.CreateNeg(operand.value, "negtmp"), operand.type);
            case OP_PLUS:
                return operand;
            case OP_NOT: {
                Value* boolVal = operand.value;
                if (!boolVal->getType()->isIntegerTy(1)) {
                    boolVal = builder.CreateICmpNE(
                        boolVal,
                        ConstantInt::get(boolVal->getType(), 0),
                        "not_cmp");
                }
                Value* result = builder.CreateNot(boolVal, "nottmp");
                return VisitResult(result, ValueType::BOOL);
            }
            case OP_DEREF: {
                Value* ptrVal = operand.value;
                if (!ptrVal->getType()->isPointerTy()) {
                    return VisitResult();
                }

                std::string varName;
                ASTNode* derefExpr = node->data.unaryop.expr;
                if (derefExpr &&
                    derefExpr->type == AST_IDENTIFIER &&
                    derefExpr->data.identifier.name) {
                    varName = std::string(derefExpr->data.identifier.name);
                } else if (derefExpr && derefExpr->type == AST_BINOP &&
                           (derefExpr->data.binop.op == OP_ADD || derefExpr->data.binop.op == OP_SUB)) {
                    ASTNode* left = derefExpr->data.binop.left;
                    ASTNode* right = derefExpr->data.binop.right;
                    if (left && left->type == AST_IDENTIFIER && left->data.identifier.name) {
                        varName = std::string(left->data.identifier.name);
                    } else if (right && right->type == AST_IDENTIFIER && right->data.identifier.name) {
                        varName = std::string(right->data.identifier.name);
                    }
                }

                Type* elemType = nullptr;
                auto hintIt = pointerElementHints.find(operand.value);
                if (hintIt != pointerElementHints.end() && hintIt->second) {
                    elemType = hintIt->second;
                }
                if (!elemType) {
                    AllocaInst* derefAlloc = varName.empty() ? nullptr : scopeManager.findVariable(varName);
                    if (!derefAlloc && !varName.empty()) derefAlloc = findVariableInMain(varName);
                    if (derefAlloc) {
                        auto allocHintIt = pointerElementHints.find(derefAlloc);
                        if (allocHintIt != pointerElementHints.end() && allocHintIt->second) {
                            elemType = allocHintIt->second;
                        }
                    }
                }
                if (!elemType) {
                    PointerType* ptrTy = dyn_cast<PointerType>(ptrVal->getType());
                    elemType = getPointerElementTypeSafely(ptrTy, varName);
                }
                if (!elemType) {
                    elemType = Type::getInt32Ty(context);
                }

                Type* expectPtrType = PointerType::get(context, 0);
                if (ptrVal->getType() != expectPtrType) {
                    ptrVal = builder.CreateBitCast(ptrVal, expectPtrType, "deref_ptrcast");
                }

                if (elemType->isStructTy()) {
                    return VisitResult(ptrVal, ValueType::POINTER, cast<StructType>(elemType));
                }

                Value* loaded = builder.CreateLoad(elemType, ptrVal, "deref_load");
                return VisitResult(loaded, typeHelper.getValueTypeFromType(elemType));
            }
            default:
                return VisitResult();
        }
    }
    
    LLVMCodeGenerator::VisitResult LLVMCodeGenerator::visitArrayLiteral(ASTNode* node) {
        if (!node || node->type != AST_EXPRESSION_LIST) return VisitResult();
        int count = node->data.expression_list.expression_count;
        if (count == 0) {
            return VisitResult(ConstantPointerNull::get(cast<PointerType>(typeHelper.getLLVMType(ValueType::POINTER))), ValueType::ARRAY);
        }

        std::vector<Value*> elems;
        elems.reserve(count);
        Type* elemType = nullptr;
        ValueType elemValueType = ValueType::INT32;

        for (int i = 0; i < count; i++) {
            ASTNode* e = node->data.expression_list.expressions[i];
            VisitResult r = visit(e);
            if (!r.value) return VisitResult();
            if (i == 0) {
                elemType = r.value->getType();
                elemValueType = typeHelper.getValueTypeFromType(elemType);
            }
            Value* v = r.value;
            if (typeHelper.getValueTypeFromType(v->getType()) != elemValueType) {
                v = typeHelper.castValue(builder, v, r.type, elemValueType);
            }
            elems.push_back(v);
        }

        if (!elemType) return VisitResult();

        uint64_t elemBytes = 4;
        if (elemType->isIntegerTy(8)) elemBytes = 1;
        else if (elemType->isIntegerTy(64) || elemType->isDoubleTy() || elemType->isPointerTy()) elemBytes = 8;
        else if (elemType->isFloatTy()) elemBytes = 4;

        Value* headerSizeVal = ConstantInt::get(Type::getInt64Ty(context), ARRAY_HEADER_BYTES);
        Value* dataBytes = ConstantInt::get(Type::getInt64Ty(context), (uint64_t)count * elemBytes);
        Value* totalBytes = builder.CreateAdd(headerSizeVal, dataBytes, "arr_total_bytes");
        Function* reallocFn = getOrCreateReallocFunction();
        Value* heapI8 = builder.CreateCall(
            reallocFn,
            {ConstantPointerNull::get(PointerType::get(context, 0)), totalBytes},
            "arr_heap"
        );
        Value* lenPtr = builder.CreateBitCast(heapI8, PointerType::get(context, 0), "arr_len_ptr");
        builder.CreateStore(ConstantInt::get(Type::getInt32Ty(context), count), lenPtr);
        Value* dataI8 = builder.CreateInBoundsGEP(Type::getInt8Ty(context), heapI8, headerSizeVal, "arr_data_i8");
        Value* arrayPtr = builder.CreateBitCast(dataI8, PointerType::get(context, 0), "arr_ptr");

        for (int i = 0; i < count; i++) {
            Value* idx = ConstantInt::get(Type::getInt32Ty(context), i);
            Value* gep = builder.CreateInBoundsGEP(elemType, arrayPtr, idx, "elem_ptr");
            builder.CreateStore(elems[i], gep);
        }

        pointerElementHints[arrayPtr] = elemType;
        return VisitResult(arrayPtr, ValueType::ARRAY);
    }
