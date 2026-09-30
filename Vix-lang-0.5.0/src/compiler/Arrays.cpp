#include "Codegen.h"

using namespace llvm;

LLVMCodeGenerator::VisitResult LLVMCodeGenerator::computeIndexPtr(ASTNode* node) {
    if (!node || node->type != AST_INDEX) return VisitResult();
    ASTNode* target = node->data.index.target;
    ASTNode* indexNode = node->data.index.index;

    VisitResult idxRes = visit(indexNode);
    if (!idxRes.value) return VisitResult();
    Value* idxVal = idxRes.value;
    if (!idxVal->getType()->isIntegerTy(32)) {
        idxVal = builder.CreateIntCast(idxVal, Type::getInt32Ty(context), true, "idxcast");
    }

    AllocaInst* baseAlloc = nullptr;
    std::string varName;
    if (target->type == AST_IDENTIFIER) {
        varName = std::string(target->data.identifier.name);
        baseAlloc = scopeManager.findVariable(varName);
        if (!baseAlloc) baseAlloc = findVariableInMain(varName);
    }

    if (baseAlloc) {
        Type* allocatedType = getActualType(baseAlloc);
        if (allocatedType && allocatedType->isArrayTy()) {
            ArrayType* at = cast<ArrayType>(allocatedType);
            Type* elemType = at->getElementType();
            Value* gep = builder.CreateInBoundsGEP(allocatedType, baseAlloc,
                {ConstantInt::get(Type::getInt32Ty(context), 0), idxVal}, "lval_arr_gep");
            VisitResult res(gep, typeHelper.getValueTypeFromType(elemType));
            if (elemType->isPointerTy()) {
                pointerElementHints[gep] = Type::getInt32Ty(context);
            }
            return res;
        }
        if (allocatedType && allocatedType->isPointerTy()) {
            Value* arrayPtr = builder.CreateLoad(allocatedType, baseAlloc, "lval_array_ptr");
            Type* elemType = getPointerElementTypeSafely(dyn_cast<PointerType>(allocatedType), varName);
            Value* gep = builder.CreateInBoundsGEP(elemType, arrayPtr, idxVal, "lval_ptr_gep");
            VisitResult res(gep, typeHelper.getValueTypeFromType(elemType));
            if (elemType->isPointerTy()) {
                pointerElementHints[gep] = Type::getInt32Ty(context);
            }
            return res;
        }
    }

    if (target->type == AST_INDEX) {
        VisitResult innerPtr = computeIndexPtr(target);
        if (innerPtr.value && innerPtr.value->getType()->isPointerTy()) {
            Type* elemType = nullptr;
            auto hintIt = pointerElementHints.find(innerPtr.value);
            if (hintIt != pointerElementHints.end() && hintIt->second) {
                elemType = hintIt->second;
            }
            if (!elemType) {
                elemType = getPointerElementTypeSafely(dyn_cast<PointerType>(innerPtr.value->getType()), "");
            }
            if (!elemType) elemType = Type::getInt32Ty(context);
            Value* gep = builder.CreateInBoundsGEP(elemType, innerPtr.value, idxVal, "lval_chain_gep");
            Type* innerElem = elemType->isArrayTy() ? cast<ArrayType>(elemType)->getElementType() : elemType;
            VisitResult res(gep, typeHelper.getValueTypeFromType(innerElem));
            if (innerElem->isPointerTy()) {
                pointerElementHints[gep] = Type::getInt32Ty(context);
            }
            return res;
        }
    }

    VisitResult targRes = visit(target);
    if (!targRes.value || !targRes.value->getType()->isPointerTy()) return VisitResult();
    Type* elemType = nullptr;
    auto hintIt = pointerElementHints.find(targRes.value);
    if (hintIt != pointerElementHints.end() && hintIt->second) {
        elemType = hintIt->second;
    }
    if (!elemType) {
        elemType = getPointerElementTypeSafely(dyn_cast<PointerType>(targRes.value->getType()), varName);
    }
    if (!elemType) return VisitResult();
    Value* gep = builder.CreateInBoundsGEP(elemType, targRes.value, idxVal, "lval_generic_gep");
    VisitResult res(gep, typeHelper.getValueTypeFromType(elemType));
    if (elemType->isPointerTy()) {
        pointerElementHints[gep] = Type::getInt32Ty(context);
    }
    return res;
}

LLVMCodeGenerator::VisitResult LLVMCodeGenerator::visitIndexAssign(ASTNode* node) {
    ASTNode* indexNode = node->data.assign.left;
    ASTNode* target = indexNode->data.index.target;
    ASTNode* idxExpr = indexNode->data.index.index;

    VisitResult idxRes = visit(idxExpr);
    if (!idxRes.value) return VisitResult();
    Value* idxVal = idxRes.value;
    if (!idxVal->getType()->isIntegerTy(32)) {
        idxVal = builder.CreateIntCast(idxVal, Type::getInt32Ty(context), true, "idxcast");
    }

    AllocaInst* baseAlloc = nullptr;
    std::string varName;
    if (target->type == AST_IDENTIFIER) {
        varName = std::string(target->data.identifier.name);
        baseAlloc = scopeManager.findVariable(varName);
        if (!baseAlloc) baseAlloc = findVariableInMain(varName);
    }

    if (baseAlloc) {
        Type* allocatedType = getActualType(baseAlloc);
        if (allocatedType && allocatedType->isArrayTy()) {
            ArrayType* at = cast<ArrayType>(allocatedType);
            Type* elemType = at->getElementType();
            Value* gep = builder.CreateInBoundsGEP(allocatedType, baseAlloc, 
                {ConstantInt::get(Type::getInt32Ty(context),0), idxVal}, "arr_index_ptr");

            VisitResult rightVal = visit(node->data.assign.right);
            if (!rightVal.value) return VisitResult();
            ValueType vt = typeHelper.getValueTypeFromType(elemType);
            Value* casted = typeHelper.castValue(builder, rightVal.value, rightVal.type, vt);
            builder.CreateStore(casted, gep);
            return VisitResult(casted, vt);
        }
        
        if (allocatedType && allocatedType->isPointerTy()) {
            Value* arrayPtr = builder.CreateLoad(allocatedType, baseAlloc, "array_ptr");
            Type* elemType = getPointerElementTypeSafely(dyn_cast<PointerType>(allocatedType), varName);
            bool isStringBuffer = typeHelper.isStringVariable(varName);
            if (isStringBuffer) {
                elemType = Type::getInt8Ty(context);
            }
            if (varName == "argv") {
                elemType = PointerType::get(context, 0);
            }

            Value* gep = builder.CreateInBoundsGEP(elemType, arrayPtr, idxVal, "ptr_index_ptr");
            VisitResult rightVal = visit(node->data.assign.right);
            if (!rightVal.value) return VisitResult();
            if (!isStringBuffer && !varName.empty() && varName != "argv" && !typeHelper.getArrayTypeInfo(varName)) {
                typeHelper.registerArrayType(varName, rightVal.value->getType(), -1);
                elemType = rightVal.value->getType();
                gep = builder.CreateInBoundsGEP(elemType, arrayPtr, idxVal, "ptr_index_ptr");
            }
            ValueType vt = typeHelper.getValueTypeFromType(elemType);
            Value* casted = typeHelper.castValue(builder, rightVal.value, rightVal.type, vt);
            builder.CreateStore(casted, gep);
            return VisitResult(casted, vt);
        }

        if (allocatedType && allocatedType->isIntegerTy()) {
            Value* baseInt = builder.CreateLoad(allocatedType, baseAlloc, "addr_base");
            if (!baseInt->getType()->isIntegerTy(64)) {
                baseInt = builder.CreateSExtOrTrunc(baseInt, Type::getInt64Ty(context), "addr_base64");
            }
            Value* idx64 = idxVal;
            if (!idx64->getType()->isIntegerTy(64)) {
                idx64 = builder.CreateSExtOrTrunc(idx64, Type::getInt64Ty(context), "idx64");
            }

            VisitResult rightVal = visit(node->data.assign.right);
            if (!rightVal.value) return VisitResult();

            Type* elemType = Type::getInt8Ty(context);
            ValueType vt = ValueType::INT8;
            Value* basePtr = builder.CreateIntToPtr(baseInt, PointerType::get(context, 0), "mmio_ptr");
            Value* gep = builder.CreateInBoundsGEP(elemType, basePtr, idx64, "mmio_index_ptr");
            Value* casted = typeHelper.castValue(builder, rightVal.value, rightVal.type, vt);
            if (!casted->getType()->isIntegerTy(8)) {
                casted = builder.CreateIntCast(casted, Type::getInt8Ty(context), true, "mmio_i8");
            }
            builder.CreateStore(casted, gep);
            return VisitResult(casted, vt);
        }
    }

    if (target->type == AST_INDEX) {
        VisitResult targRes = computeIndexPtr(target);
        if (targRes.value && targRes.value->getType()->isPointerTy()) {
            Type* elemType = nullptr;
            auto hintIt = pointerElementHints.find(targRes.value);
            if (hintIt != pointerElementHints.end() && hintIt->second) {
                elemType = hintIt->second;
            }
            if (!elemType) {
                elemType = getPointerElementTypeSafely(dyn_cast<PointerType>(targRes.value->getType()), "");
            }
            if (!elemType) elemType = Type::getInt32Ty(context);

            Value* gep = builder.CreateInBoundsGEP(elemType, targRes.value, idxVal, "chain_arr_index_ptr");
            VisitResult rightVal = visit(node->data.assign.right);
            if (!rightVal.value) return VisitResult();
            Type* storeElemType = elemType;
            if (storeElemType->isArrayTy()) {
                storeElemType = cast<ArrayType>(storeElemType)->getElementType();
            }
            ValueType vt = typeHelper.getValueTypeFromType(storeElemType);
            Value* casted = typeHelper.castValue(builder, rightVal.value, rightVal.type, vt);
            builder.CreateStore(casted, gep);
            return VisitResult(casted, vt);
        }
    }

    VisitResult targRes = visit(target);
    if (!targRes.value) return VisitResult();

    if (targRes.value->getType()->isPointerTy()) {
        if (target->type == AST_IDENTIFIER) {
            varName = std::string(target->data.identifier.name);
        }
        Type* elemType = nullptr;
        auto hintIt = pointerElementHints.find(targRes.value);
        if (hintIt != pointerElementHints.end() && hintIt->second) {
            elemType = hintIt->second;
        }
        if (!elemType) {
            elemType = getPointerElementTypeSafely(dyn_cast<PointerType>(targRes.value->getType()), varName);
        }
        if (!elemType) return VisitResult();

        Value* gep = builder.CreateInBoundsGEP(elemType, targRes.value, idxVal, "arr_index_ptr");

        if (gep) {
            VisitResult rightVal = visit(node->data.assign.right);
            if (!rightVal.value) return VisitResult();
            if (!varName.empty() && varName != "argv" && !typeHelper.getArrayTypeInfo(varName)) {
                typeHelper.registerArrayType(varName, rightVal.value->getType(), -1);
                elemType = rightVal.value->getType();
                gep = builder.CreateInBoundsGEP(elemType, targRes.value, idxVal, "arr_index_ptr");
            }
            ValueType vt = typeHelper.getValueTypeFromType(elemType);
            Value* casted = typeHelper.castValue(builder, rightVal.value, rightVal.type, vt);
            builder.CreateStore(casted, gep);
            return VisitResult(casted, vt);
        }
    }

    return VisitResult();
}

LLVMCodeGenerator::VisitResult LLVMCodeGenerator::visitIndex(ASTNode* node) {
    if (!node || !node->data.index.target || !node->data.index.index) return VisitResult();
    ASTNode* target = node->data.index.target;
    ASTNode* indexNode = node->data.index.index;
    
    
    if (target->type == AST_IDENTIFIER) {//处理字符串索引s[i]
        std::string varName(target->data.identifier.name);
        
        AllocaInst* alloc = scopeManager.findVariable(varName);
        if (!alloc) alloc = findVariableInMain(varName);
        
        if (alloc) {
            Type* allocatedType = getActualType(alloc);
            bool isStringType = false;
            auto* arrayInfo = typeHelper.getArrayTypeInfo(varName);
            if (arrayInfo && arrayInfo->first->isIntegerTy(8)) {
                isStringType = true;
            }
            
            if (typeHelper.isStringVariable(varName)) {
                isStringType = true;
            }
            
            if (allocatedType->isArrayTy()) {
                ArrayType* arrType = cast<ArrayType>(allocatedType);
                if (arrType->getElementType()->isIntegerTy(8)) {
                    isStringType = true;
                }
            }
            if (allocatedType->isPointerTy() && typeHelper.isStringVariable(varName)) {
                isStringType = true;
            }
            if (isStringType) {
                VisitResult idxRes = visit(indexNode);
                if (!idxRes.value) return VisitResult();
                
                Value* idxVal = idxRes.value;
                if (!idxVal->getType()->isIntegerTy(32)) {
                    idxVal = builder.CreateIntCast(idxVal, Type::getInt32Ty(context), true, "idxcast");
                }
                
                Value* charPtr = nullptr;
                
                if (allocatedType->isArrayTy()) {
                    Value* zero = ConstantInt::get(Type::getInt32Ty(context), 0);
                    charPtr = builder.CreateInBoundsGEP(
                        allocatedType, alloc, {zero, idxVal}, "char_ptr");
                } else {
                    Value* strPtr = builder.CreateLoad(allocatedType, alloc, varName);
                    Type* pointeeType = nullptr;
                    if (allocatedType->isPointerTy()) {
                        pointeeType = getPointerElementTypeSafely(dyn_cast<PointerType>(allocatedType), varName);
                    } else {
                        pointeeType = allocatedType;
                    }
                    
                    charPtr = builder.CreateInBoundsGEP(
                        pointeeType, strPtr, idxVal, "char_ptr");
                }
                
                Value* charVal = builder.CreateLoad(Type::getInt8Ty(context), charPtr, "char");
                
                VIX_DEBUG_LOG << "[DEBUG] string index: " << varName << "[i] = char (i8)\n";
                
                return VisitResult(charVal, ValueType::INT8);
            }
        }
    }
    AllocaInst* baseAlloc = nullptr;
    std::string varName;
    if (target->type == AST_IDENTIFIER) {
        varName = std::string(target->data.identifier.name);
        baseAlloc = scopeManager.findVariable(varName);
        if (!baseAlloc) baseAlloc = findVariableInMain(varName);
    }

    VisitResult idxRes = visit(indexNode);
    if (!idxRes.value) return VisitResult();
    Value* idxVal = idxRes.value;
    if (!idxVal->getType()->isIntegerTy(32)) {
        idxVal = builder.CreateIntCast(idxVal, Type::getInt32Ty(context), true, "idxcast");
    }

    if (baseAlloc) {
        Type* allocatedType = getActualType(baseAlloc);
        if (allocatedType && allocatedType->isArrayTy()) {
            ArrayType* at = cast<ArrayType>(allocatedType);
            Type* elemType = at->getElementType();
            Value* gep = builder.CreateInBoundsGEP(allocatedType, baseAlloc, 
                {ConstantInt::get(Type::getInt32Ty(context),0), idxVal}, "arr_index_ptr");
            Value* loaded = builder.CreateLoad(elemType, gep, "arr_index_load");
            ValueType vt = typeHelper.getValueTypeFromType(elemType);
            StructType* st = elemType->isStructTy() ? cast<StructType>(elemType) : nullptr;
            return VisitResult(loaded, vt, st);
        }
        
        if (allocatedType && allocatedType->isPointerTy()) {
            Value* arrayPtr = builder.CreateLoad(allocatedType, baseAlloc, "array_ptr");
            Type* elemType = getPointerElementTypeSafely(dyn_cast<PointerType>(allocatedType), varName);
            if (varName == "argv") {//处理argv特殊情况
                elemType = PointerType::get(context, 0);
            }
            StructType* elemStructType = elemType && elemType->isStructTy() ? cast<StructType>(elemType) : nullptr;
            Type* storageElemType = elemStructType ? PointerType::get(context, 0) : elemType;
            ValueType vt = typeHelper.getValueTypeFromType(elemType);

            Function* fn = builder.GetInsertBlock() ? builder.GetInsertBlock()->getParent() : nullptr;
            if (!fn) return VisitResult();

            BasicBlock* nullBB = BasicBlock::Create(context, "idx_null", fn);
            BasicBlock* loadBB = BasicBlock::Create(context, "idx_load", fn);
            BasicBlock* contBB = BasicBlock::Create(context, "idx_cont", fn);

            Value* isNull = builder.CreateIsNull(arrayPtr, "idx_ptr_is_null");
            builder.CreateCondBr(isNull, nullBB, loadBB);

            builder.SetInsertPoint(nullBB);
            Value* nullVal = Constant::getNullValue(storageElemType);
            builder.CreateBr(contBB);

            builder.SetInsertPoint(loadBB);
            Value* gep = builder.CreateInBoundsGEP(storageElemType, arrayPtr, idxVal, "ptr_index_ptr");
            Value* loaded = builder.CreateLoad(storageElemType, gep, "ptr_index_load");
            builder.CreateBr(contBB);

            builder.SetInsertPoint(contBB);
            PHINode* result = builder.CreatePHI(storageElemType, 2, "idx_safe_val");
            result->addIncoming(nullVal, nullBB);
            result->addIncoming(loaded, loadBB);
            if (elemStructType) {
                pointerElementHints[result] = elemStructType;
                return VisitResult(result, ValueType::POINTER, elemStructType);
            }
            if (elemType->isPointerTy()) {
                Type* inferredElem = getInferredPointerElementType(node);
                pointerElementHints[result] = inferredElem ? inferredElem : Type::getInt32Ty(context);
            }
            return VisitResult(result, vt);
        }
    }
    VisitResult targetRes = visit(target);
    if (!targetRes.value) return VisitResult();

    if (AllocaInst* alloc = dyn_cast<AllocaInst>(targetRes.value)) {
        Type* allocatedType = getActualType(alloc);
        if (allocatedType && allocatedType->isArrayTy()) {
            ArrayType* at = cast<ArrayType>(allocatedType);
            Type* elemType = at->getElementType();
            Value* gep = builder.CreateInBoundsGEP(allocatedType, alloc, 
                {ConstantInt::get(Type::getInt32Ty(context),0), idxVal}, "arr_index_ptr2");
            Value* loaded = builder.CreateLoad(elemType, gep, "arr_index_load2");
            ValueType vt = typeHelper.getValueTypeFromType(elemType);
            StructType* st = elemType->isStructTy() ? cast<StructType>(elemType) : nullptr;
            return VisitResult(loaded, vt, st);
        }
    }

    if (targetRes.value->getType()->isPointerTy()) {
        if (target->type == AST_IDENTIFIER) {
            varName = std::string(target->data.identifier.name);
        }
        Type* elemType = nullptr;
        auto hintIt = pointerElementHints.find(targetRes.value);
        if (hintIt != pointerElementHints.end() && hintIt->second) {
            elemType = hintIt->second;
        }
        if (!elemType) {
            elemType = getPointerElementTypeSafely(dyn_cast<PointerType>(targetRes.value->getType()), varName);
        }
        if (!elemType) {
            elemType = Type::getInt8Ty(context);
        }
        StructType* elemStructType = elemType && elemType->isStructTy() ? cast<StructType>(elemType) : nullptr;
        Type* storageElemType = elemStructType ? PointerType::get(context, 0) : elemType;
        ValueType vt = typeHelper.getValueTypeFromType(elemType);

        Function* fn = builder.GetInsertBlock() ? builder.GetInsertBlock()->getParent() : nullptr;
        if (!fn) return VisitResult();

        BasicBlock* nullBB = BasicBlock::Create(context, "idx_null2", fn);
        BasicBlock* loadBB = BasicBlock::Create(context, "idx_load2", fn);
        BasicBlock* contBB = BasicBlock::Create(context, "idx_cont2", fn);

        Value* isNull = builder.CreateIsNull(targetRes.value, "idx_ptr_is_null2");
        builder.CreateCondBr(isNull, nullBB, loadBB);

        builder.SetInsertPoint(nullBB);
        Value* nullVal = Constant::getNullValue(storageElemType);
        builder.CreateBr(contBB);

        builder.SetInsertPoint(loadBB);
        Value* gep = builder.CreateInBoundsGEP(storageElemType, targetRes.value, idxVal, "arr_index_ptr3");
        Value* loaded = builder.CreateLoad(storageElemType, gep, "arr_index_load3");
        builder.CreateBr(contBB);

        builder.SetInsertPoint(contBB);
        PHINode* result = builder.CreatePHI(storageElemType, 2, "idx_safe_val2");
        result->addIncoming(nullVal, nullBB);
        result->addIncoming(loaded, loadBB);
        if (elemStructType) {
            pointerElementHints[result] = elemStructType;
            return VisitResult(result, ValueType::POINTER, elemStructType);
        }
        if (elemType->isPointerTy()) {
            Type* inferredElem = getInferredPointerElementType(node);
            pointerElementHints[result] = inferredElem ? inferredElem : Type::getInt32Ty(context);
        }
        return VisitResult(result, vt);
    }

    return VisitResult();
}
