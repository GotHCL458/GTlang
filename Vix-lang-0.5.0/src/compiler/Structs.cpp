#include "Codegen.h"
#include <system_error>

using namespace llvm;

LLVMCodeGenerator::VisitResult
LLVMCodeGenerator::visitStructDef(ASTNode *node) {
  std::string structName(node->data.struct_def.name);
  if (typeHelper.getStructType(structName) ||
      typeHelper.getStructTemplate(structName))
    return VisitResult();

  ASTNode *fields = node->data.struct_def.fields;
  if (!fields || fields->type != AST_EXPRESSION_LIST)
    return VisitResult();

  if (node->data.struct_def.generic_params &&
      node->data.struct_def.generic_params->type == AST_EXPRESSION_LIST) {
    typeHelper.registerStructTemplate(structName, node);
    return VisitResult(nullptr, ValueType::VOID);
  }

  std::vector<Type *> fieldTypes;
  std::vector<std::pair<std::string, Type *>> fieldInfo;
  StructType *structType = StructType::create(context, structName);
  typeHelper.registerStructType(structName, structType, {});

  int fieldCount = fields->data.expression_list.expression_count;
  for (int i = 0; i < fieldCount; i++) {
    ASTNode *field = fields->data.expression_list.expressions[i];

    if (field->type == AST_ASSIGN) {
      ASTNode *left = field->data.assign.left;
      ASTNode *right = field->data.assign.right;

        if (left && left->type == AST_IDENTIFIER) {
        std::string fieldName(left->data.identifier.name);
        Type *fieldType = typeHelper.getTypeFromTypeNode(right);
        if (!fieldType)
          fieldType = Type::getInt32Ty(context);
        if (fieldType == structType) {
          reportCodegenSemanticError(
              node, "self-recursive struct fields must use pointer type");
          return VisitResult();
        }
        if (right && (right->type == AST_TYPE_LIST ||
                      right->type == AST_TYPE_FIXED_SIZE_LIST)) {
          typeHelper.registerFieldArrayElementType(
              structName, fieldName,
              typeHelper.getArrayElementTypeFromNode(right));
        }
        fieldTypes.push_back(fieldType);
        fieldInfo.push_back({fieldName, fieldType});
      }
    } else if (field->type == AST_TYPE_INT32 || field->type == AST_TYPE_INT64 ||
               field->type == AST_TYPE_FLOAT32 ||
               field->type == AST_TYPE_FLOAT64 ||
               field->type == AST_TYPE_STRING) {
      char defaultName[32];
      snprintf(defaultName, sizeof(defaultName), "field%d", i);
      std::string fieldName(defaultName);
      Type *fieldType = typeHelper.getTypeFromTypeNode(field);
      fieldTypes.push_back(fieldType);
      fieldInfo.push_back({fieldName, fieldType});
    }
  }

  if (fieldTypes.empty()) {
    fieldTypes.push_back(Type::getInt8Ty(context));
    fieldInfo.push_back({"__empty", Type::getInt8Ty(context)});
  }
  structType->setBody(fieldTypes, false);
  typeHelper.registerStructType(structName, structType, fieldInfo);
  return VisitResult(nullptr, ValueType::VOID);
}

LLVMCodeGenerator::VisitResult
LLVMCodeGenerator::visitStructLiteral(ASTNode *node) {
  ASTNode *typeName = node->data.struct_literal.type_name;
  if (!typeName)
    return VisitResult();

  std::string structName;
  ASTNode *typeArgs = nullptr;
  if (typeName->type == AST_IDENTIFIER && typeName->data.identifier.name) {
    structName = typeName->data.identifier.name;
  } else if (typeName->type == AST_TYPE_APP && typeName->data.type_app.ctor &&
             typeName->data.type_app.ctor->type == AST_IDENTIFIER &&
             typeName->data.type_app.ctor->data.identifier.name) {
    structName = typeName->data.type_app.ctor->data.identifier.name;
    typeArgs = typeName->data.type_app.args;
  } else {
    return VisitResult();
  }

  Type *rawType = typeHelper.getTypeFromTypeNode(typeName);
  StructType *structType =
      rawType && rawType->isStructTy() ? cast<StructType>(rawType) : nullptr;
  if (!structType && typeArgs && typeArgs->type == AST_EXPRESSION_LIST) {
    if (Type *inst = typeHelper.getTypeFromTypeNode(typeName)) {
      if (inst->isStructTy()) {
        structType = cast<StructType>(inst);
      }
    }
  }
  if (!structType)
    return VisitResult();

  Function *func = getCurrentFunction();
  if (!func) {
    func = module->getFunction("main");
    if (!func) {
      createDefaultMain();
      func = module->getFunction("main");
    }
  }

  if (!func)
    return VisitResult();
  BasicBlock *entryBB = &func->getEntryBlock();
  BasicBlock *savedBB = builder.GetInsertBlock();

  IRBuilder<> tempBuilder(entryBB, entryBB->begin());
  AllocaInst *structAlloc =
      tempBuilder.CreateAlloca(structType, nullptr, "tmp_struct");

  if (savedBB) {
    builder.SetInsertPoint(savedBB);
  }

  initStructLiteral(structAlloc, node);

  return VisitResult(structAlloc, ValueType::POINTER, structType);
}

LLVMCodeGenerator::VisitResult
LLVMCodeGenerator::handleArrayLength(ASTNode *object) {

  if (object->type == AST_IDENTIFIER) {
    std::string varName(object->data.identifier.name);
    VIX_DEBUG_LOG << "[DEBUG] to geet length of variable: " << varName << "\n";

    if (Value *runtimeLen = getRuntimeArrayLengthValue(varName)) {
      VIX_DEBUG_LOG << "[DEBUG] Array length (runtime fat ptr): dynamic\n";
      return VisitResult(runtimeLen, ValueType::INT32);
    }

    AllocaInst *alloc = scopeManager.findVariable(varName);
    VIX_DEBUG_LOG << "[DEBUG] found in scopeMar: " << (alloc ? "yes" : "no")
                  << "\n";

    if (!alloc) {
      alloc = findVariableInMain(varName);
      VIX_DEBUG_LOG << "[DEBUG] found in main: " << (alloc ? "yes" : "no")
                    << "\n";
    }
    if (alloc) {
      Type *allocatedType = getActualType(alloc);
      VIX_DEBUG_LOG << "[DEBUG] type: " << *allocatedType << "\n";
      if (allocatedType && allocatedType->isArrayTy()) {
        ArrayType *arrayType = cast<ArrayType>(allocatedType);
        uint64_t numElements = arrayType->getNumElements();
        Value *length =
            ConstantInt::get(Type::getInt32Ty(context), numElements);
        VIX_DEBUG_LOG << "[DEBUG] Arr length (s): " << numElements << "\n";
        return VisitResult(length, ValueType::INT32);
      }
      if (allocatedType && allocatedType->isPointerTy()) {
        if (typeHelper.isStringVariable(varName)) {
          Value *strPtr = builder.CreateLoad(allocatedType, alloc, varName);
          CallInst *strlenCall =
              builder.CreateCall(strlenFunction, {strPtr}, "strlen");
          Value *length = builder.CreateIntCast(
              strlenCall, Type::getInt32Ty(context), false, "len");
          VIX_DEBUG_LOG << "[DEBUG] String length (strlen): dynamic\n";
          return VisitResult(length, ValueType::INT32);
        }
        auto *arrayInfo = typeHelper.getArrayTypeInfo(varName);
        if (arrayInfo) {
          int elementCount = arrayInfo->second;
          if (elementCount > 0) {
            Value *length =
                ConstantInt::get(Type::getInt32Ty(context), elementCount);
            VIX_DEBUG_LOG << "[DEBUG] Array length (r): " << elementCount
                          << "\n";
            return VisitResult(length, ValueType::INT32);
          }
          if (elementCount < 0) {
            Value *dataPtr = builder.CreateLoad(allocatedType, alloc, varName);
            Value *runtimeLen = emitLoadArrayLength(dataPtr, varName + "_dyn_len");
            VIX_DEBUG_LOG << "[DEBUG] Array length (dynamic from header)\n";
            return VisitResult(runtimeLen, ValueType::INT32);
          }
        }
        VIX_DEBUG_LOG << "[DEBUG] Unknown pointer type, returning 0\n";
        Value *length = ConstantInt::get(Type::getInt32Ty(context), 0);
        return VisitResult(length, ValueType::INT32);
      }
    }
    auto *arrayInfo = typeHelper.getArrayTypeInfo(varName);
    if (arrayInfo) {
      int elementCount = arrayInfo->second;
      Value *length = ConstantInt::get(Type::getInt32Ty(context), elementCount);
      VIX_DEBUG_LOG << "[DEBUG] Array length (type info): " << elementCount
                    << "\n";
      return VisitResult(length, ValueType::INT32);
    }
    if (typeHelper.isStringVariable(varName)) {
      AllocaInst *varAlloc = scopeManager.findVariable(varName);
      if (!varAlloc)
        varAlloc = findVariableInMain(varName);

      if (varAlloc) {
        Type *allocatedType = getActualType(varAlloc);
        Value *strPtr = builder.CreateLoad(allocatedType, varAlloc, varName);
        CallInst *strlenCall =
            builder.CreateCall(strlenFunction, {strPtr}, "strlen");
        Value *length = builder.CreateIntCast(
            strlenCall, Type::getInt32Ty(context), false, "len");
        VIX_DEBUG_LOG << "[DEBUG] String length (strlen from "
                         "isStringVariable): dynamic\n";
        return VisitResult(length, ValueType::INT32);
      }
    }
  }
  if (object->type == AST_EXPRESSION_LIST) {
    int count = object->data.expression_list.expression_count;
    Value *length = ConstantInt::get(Type::getInt32Ty(context), count);
    VIX_DEBUG_LOG << "[DEBUG] Literal length: " << count << "\n";
    return VisitResult(length, ValueType::INT32);
  }

  if (object->type == AST_MEMBER_ACCESS) {
    VisitResult objRes = visit(object);
    if (objRes.value && objRes.value->getType()->isPointerTy()) {
      Value *runtimeLen = emitLoadArrayLength(objRes.value, "member_arr_len");
      return VisitResult(runtimeLen, ValueType::INT32);
    }

    ASTNode *obj = object->data.member_access.object;
    ASTNode *field = object->data.member_access.field;
    if (obj && field && obj->type == AST_IDENTIFIER &&
        field->type == AST_IDENTIFIER && obj->data.identifier.name &&
        field->data.identifier.name) {
      std::string key = std::string(obj->data.identifier.name) + "." +
                        std::string(field->data.identifier.name);
      auto it = memberArrayLengthHints.find(key);
      if (it != memberArrayLengthHints.end() && it->second >= 0) {
        return VisitResult(
            ConstantInt::get(Type::getInt32Ty(context), it->second),
            ValueType::INT32);
      }
    }
  }

  if (object->type == AST_INDEX) {
    ASTNode *outerTarget = object->data.index.target;
    if (outerTarget && outerTarget->type == AST_MEMBER_ACCESS) {
      ASTNode *obj = outerTarget->data.member_access.object;
      ASTNode *field = outerTarget->data.member_access.field;
      if (obj && field && obj->type == AST_IDENTIFIER &&
          field->type == AST_IDENTIFIER && obj->data.identifier.name &&
          field->data.identifier.name) {
        std::string key = std::string(obj->data.identifier.name) + "." +
                          std::string(field->data.identifier.name);
        auto it = memberNestedArrayLengthHints.find(key);
        if (it != memberNestedArrayLengthHints.end()) {
          return VisitResult(
              ConstantInt::get(Type::getInt32Ty(context), it->second),
              ValueType::INT32);
        }
      }
    }
  }

  {
    VisitResult objRes = visit(object);
    if (objRes.value && objRes.value->getType()->isPointerTy()) {
      Value *runtimeLen = emitLoadArrayLength(objRes.value, "member_arr_len");
      return VisitResult(runtimeLen, ValueType::INT32);
    }
  }

  Value *length = ConstantInt::get(Type::getInt32Ty(context), 0);
  return VisitResult(length, ValueType::INT32);
}

LLVMCodeGenerator::VisitResult
LLVMCodeGenerator::visitMemberAccess(ASTNode *node) {
  ASTNode *object = node->data.member_access.object;
  ASTNode *field = node->data.member_access.field;
  if (!object || !field)
    return VisitResult();

  if (field->type == AST_IDENTIFIER) {
    std::string fieldName(field->data.identifier.name);
    if (fieldName == "length" || fieldName == "size") {
      return handleArrayLength(object);
    }
  }

  VisitResult objectRes = visit(object);
  if (!objectRes.value)
    return VisitResult();

  if (field->type != AST_IDENTIFIER)
    return VisitResult();

  std::string fieldName(field->data.identifier.name);

  bool numericField = !fieldName.empty();
  for (char c : fieldName) {
    if (c < '0' || c > '9') {
      numericField = false;
      break;
    }
  }
  if (numericField && objectRes.value->getType()->isPointerTy()) {
    long long tupleIndex = atoll(fieldName.c_str());
    if (tupleIndex < 0)
      tupleIndex = 0;

    if (objectRes.structType) {
      StructType *structTy = objectRes.structType;
      if ((unsigned)tupleIndex < structTy->getNumElements()) {
        Value *fieldPtr = builder.CreateStructGEP(
            structTy, objectRes.value, (unsigned)tupleIndex, "struct_field");
        Type *fieldType = structTy->getElementType((unsigned)tupleIndex);
        Value *loaded =
            builder.CreateLoad(fieldType, fieldPtr, "struct_field_val");
        if (fieldType->isPointerTy() && node && node->inferred_type) {
          Type *inferredLLVM = getLLVMTypeFromTypeInfo(node->inferred_type);
          if (inferredLLVM && inferredLLVM->isIntegerTy()) {
            Value *intVal = builder.CreatePtrToInt(
                loaded, Type::getInt64Ty(context), "adt_payload_ptrtoint");
            loaded = builder.CreateIntCast(intVal, inferredLLVM, true,
                                           "adt_payload_intcast");
            return VisitResult(loaded,
                               typeHelper.getValueTypeFromType(inferredLLVM));
          }
          if (inferredLLVM && inferredLLVM->isPointerTy() &&
              inferredLLVM != fieldType) {
            loaded = builder.CreateBitCast(loaded, inferredLLVM,
                                           "adt_payload_bcast");
            return VisitResult(loaded,
                               typeHelper.getValueTypeFromType(inferredLLVM));
          }
        }
        return VisitResult(loaded, typeHelper.getValueTypeFromType(fieldType));
      }
    }

    Type *elemType = nullptr;
    auto hintIt = pointerElementHints.find(objectRes.value);
    if (hintIt != pointerElementHints.end() && hintIt->second) {
      elemType = hintIt->second;
    }
    if (!elemType) {
      std::string objName;
      if (object->type == AST_IDENTIFIER && object->data.identifier.name) {
        objName = object->data.identifier.name;
      }
      elemType = getPointerElementTypeSafely(
          dyn_cast<PointerType>(objectRes.value->getType()), objName);
    }
    if (object && object->inferred_type &&
        (object->inferred_type->kind == TYPEINFO_APP ||
         (object->inferred_type->kind == TYPEINFO_STRUCT &&
          object->inferred_type->name &&
          (vix_is_adt_definition(object->inferred_type->name) ||
           strcmp(object->inferred_type->name, "Option") == 0 ||
           strcmp(object->inferred_type->name, "Result") == 0)))) {
      StructType *adtStructTy = StructType::get(
          context, {Type::getInt32Ty(context), PointerType::get(context, 0)});
      elemType = adtStructTy;
    }
    if (!elemType) {
      elemType = Type::getInt8Ty(context);
    }
    if (!elemType) {
      std::string objName;
      if (object->type == AST_IDENTIFIER && object->data.identifier.name) {
        objName = object->data.identifier.name;
      }
      elemType = getPointerElementTypeSafely(
          dyn_cast<PointerType>(objectRes.value->getType()), objName);
    }
    if (!elemType) {
      elemType = Type::getInt8Ty(context);
    }

    if (elemType->isStructTy()) {
      StructType *structTy = cast<StructType>(elemType);
      if ((unsigned)tupleIndex < structTy->getNumElements()) {
        Value *fieldPtr = builder.CreateStructGEP(
            structTy, objectRes.value, (unsigned)tupleIndex, "struct_field");
        Type *fieldType = structTy->getElementType((unsigned)tupleIndex);
        Value *loaded =
            builder.CreateLoad(fieldType, fieldPtr, "struct_field_val");
        if (fieldType->isPointerTy() && node && node->inferred_type) {
          Type *inferredLLVM = getLLVMTypeFromTypeInfo(node->inferred_type);
          if (inferredLLVM && inferredLLVM->isIntegerTy()) {
            Value *intVal = builder.CreatePtrToInt(
                loaded, Type::getInt64Ty(context), "adt_payload_ptrtoint");
            loaded = builder.CreateIntCast(intVal, inferredLLVM, true,
                                           "adt_payload_intcast");
            return VisitResult(loaded,
                               typeHelper.getValueTypeFromType(inferredLLVM));
          }
          if (inferredLLVM && inferredLLVM->isPointerTy() &&
              inferredLLVM != fieldType) {
            loaded = builder.CreateBitCast(loaded, inferredLLVM,
                                           "adt_payload_bcast");
            return VisitResult(loaded,
                               typeHelper.getValueTypeFromType(inferredLLVM));
          }
        }
        return VisitResult(loaded, typeHelper.getValueTypeFromType(fieldType));
      }
    }

    Value *basePtr = objectRes.value;
    Type *expectedPtrType = PointerType::get(context, 0);
    if (basePtr->getType() != expectedPtrType &&
        basePtr->getType()->isPointerTy() && expectedPtrType->isPointerTy()) {
      basePtr =
          builder.CreateBitCast(basePtr, expectedPtrType, "tuple_base_cast");
    }

    Function *fn = builder.GetInsertBlock()
                       ? builder.GetInsertBlock()->getParent()
                       : nullptr;
    if (!fn)
      return VisitResult();

    BasicBlock *nullBB = BasicBlock::Create(context, "tuple_idx_null", fn);
    BasicBlock *loadBB = BasicBlock::Create(context, "tuple_idx_load", fn);
    BasicBlock *contBB = BasicBlock::Create(context, "tuple_idx_cont", fn);

    Value *isNull = builder.CreateIsNull(basePtr, "tuple_base_is_null");
    builder.CreateCondBr(isNull, nullBB, loadBB);

    builder.SetInsertPoint(nullBB);
    Value *nullVal = Constant::getNullValue(elemType);
    builder.CreateBr(contBB);

    builder.SetInsertPoint(loadBB);
    Value *idxVal = ConstantInt::get(Type::getInt32Ty(context), tupleIndex);
    Value *elemPtr =
        builder.CreateInBoundsGEP(elemType, basePtr, idxVal, "tuple_elem_ptr");
    Value *loaded = builder.CreateLoad(elemType, elemPtr, "tuple_elem");
    builder.CreateBr(contBB);

    builder.SetInsertPoint(contBB);
    PHINode *safeLoaded = builder.CreatePHI(elemType, 2, "tuple_elem_safe");
    safeLoaded->addIncoming(nullVal, nullBB);
    safeLoaded->addIncoming(loaded, loadBB);

    if (elemType->isPointerTy()) {
      pointerElementHints[safeLoaded] = Type::getInt8Ty(context);
    }
    return VisitResult(safeLoaded, typeHelper.getValueTypeFromType(elemType));
  }

  StructType *structType = nullptr;
  Value *basePtr = nullptr;

  if (objectRes.value->getType()->isIntegerTy()) {
    std::string inferredStructName;
    StructType *inferredStructType =
        typeHelper.inferStructTypeByFieldName(fieldName, &inferredStructName);
    if (inferredStructType) {
      structType = inferredStructType;
      Type *int64Ty = Type::getInt64Ty(context);
      Value *addrVal = objectRes.value;
      if (addrVal->getType() != int64Ty) {
        if (!addrVal->getType()->isIntegerTy()) {
          return VisitResult(ConstantInt::get(Type::getInt32Ty(context), 0),
                             ValueType::INT32);
        }
        addrVal =
            builder.CreateIntCast(addrVal, int64Ty, true, "member_addr64");
      }
      basePtr = builder.CreateIntToPtr(addrVal, PointerType::get(context, 0),
                                       "member_obj_ptr");
    }
  }

  if (!objectRes.value->getType()->isPointerTy() && !basePtr) {
    if (objectRes.structType) {
      Function *fn = builder.GetInsertBlock()
                         ? builder.GetInsertBlock()->getParent()
                         : nullptr;
      if (fn) {
        AllocaInst *tempAlloc = builder.CreateAlloca(
            objectRes.value->getType(), nullptr, "struct_val_temp");
        builder.CreateStore(objectRes.value, tempAlloc);
        basePtr = tempAlloc;
        structType = objectRes.structType;
      }
    }
    if (!basePtr) {
      llvm::errs() << "Error: Object is not a pointer\n";
      return VisitResult(ConstantInt::get(Type::getInt32Ty(context), 0),
                         ValueType::INT32);
    }
  }

  if (objectRes.structType && !basePtr) {
    structType = objectRes.structType;
    basePtr = objectRes.value;
  } else if (!basePtr) {
    if (AllocaInst *alloc = dyn_cast<AllocaInst>(objectRes.value)) {
      Type *allocatedType = getActualType(alloc);
      if (allocatedType->isStructTy()) {
        structType = cast<StructType>(allocatedType);
        basePtr = alloc;
      }
    } else if (LoadInst *load = dyn_cast<LoadInst>(objectRes.value)) {
      Value *ptr = load->getPointerOperand();
      if (ptr->getType()->isPointerTy()) {
        if (AllocaInst *allocPtr = dyn_cast<AllocaInst>(ptr)) {
          Type *allocatedType = getActualType(allocPtr);
          if (allocatedType->isStructTy()) {
            structType = cast<StructType>(allocatedType);
            basePtr = ptr;
          }
        }
      }
    }
  }

  if (!structType && objectRes.value->getType()->isPointerTy()) {
    std::string inferredStructName;
    StructType *inferredStructType =
        typeHelper.inferStructTypeByFieldName(fieldName, &inferredStructName);
    if (inferredStructType) {
      structType = inferredStructType;
      Type *expectedPtrType = PointerType::get(context, 0);
      if (objectRes.value->getType() == expectedPtrType) {
        basePtr = objectRes.value;
      } else {
        basePtr = builder.CreateBitCast(objectRes.value, expectedPtrType,
                                        "struct_ptr_cast");
      }
    }
  }

  if (!structType && objectRes.value->getType()->isPointerTy()) {
    auto hintIt = pointerElementHints.find(objectRes.value);
    if (hintIt == pointerElementHints.end()) {
      // Try checking if the value is a LoadInst from an alloca that has a hint
      if (LoadInst *load = dyn_cast<LoadInst>(objectRes.value)) {
        Value *ptr = load->getPointerOperand();
        if (AllocaInst *allocPtr = dyn_cast<AllocaInst>(ptr)) {
          auto allocHintIt = pointerElementHints.find(allocPtr);
          if (allocHintIt != pointerElementHints.end() && allocHintIt->second &&
              allocHintIt->second->isStructTy()) {
            pointerElementHints[objectRes.value] = allocHintIt->second;
            hintIt = pointerElementHints.find(objectRes.value);
          }
        }
      }
    }
    if (hintIt != pointerElementHints.end() && hintIt->second &&
        hintIt->second->isStructTy()) {
      structType = cast<StructType>(hintIt->second);
      basePtr = objectRes.value;
    }
  }
  if (!structType && objectRes.value->getType()->isPointerTy()) {
    if (AllocaInst *alloc = dyn_cast<AllocaInst>(objectRes.value)) {
      Type *allocatedType = getActualType(alloc);
      if (allocatedType->isPointerTy()) {
        auto hintIt = pointerElementHints.find(alloc);
        if (hintIt != pointerElementHints.end() && hintIt->second &&
            hintIt->second->isStructTy()) {
          structType = cast<StructType>(hintIt->second);
          basePtr = builder.CreateLoad(allocatedType, alloc, "struct_ptr_ld");
        }
      }
    }
  }

  if (!structType || !basePtr) {
    llvm::errs() << "Error: Cannot access member '" << fieldName
                 << "' - not a struct type\n";
    return VisitResult(ConstantInt::get(Type::getInt32Ty(context), 0),
                       ValueType::INT32);
  }

  std::string structName = structType->getName().str();
  int idx = typeHelper.getFieldIndex(structName, fieldName);
  if (idx < 0) {
    std::string msg =
        "Struct '" + structName + "' has no member named '" + fieldName + "'";
    const char *filename =
        node && node->source_file
            ? node->source_file
            : (current_input_filename ? current_input_filename : "unknown");
    int line =
        node && node->location.first_line > 0 ? node->location.first_line : 1;
    report_semantic_error_with_location(msg.c_str(), filename, line);
    return VisitResult(ConstantInt::get(Type::getInt32Ty(context), 0),
                       ValueType::INT32);
  }

  Value *fieldPtr =
      builder.CreateStructGEP(structType, basePtr, idx, fieldName);

  Type *fieldType = structType->getElementType(idx);
  if (fieldType->isArrayTy()) {
    ArrayType *arrayType = cast<ArrayType>(fieldType);
    Type *elemType = arrayType->getElementType();
    Value *zero = ConstantInt::get(Type::getInt32Ty(context), 0);
    Value *elemPtr = builder.CreateInBoundsGEP(
        fieldType, fieldPtr, {zero, zero}, fieldName + "_ptr");

    if (elemType->isIntegerTy(8)) {
      return VisitResult(elemPtr, ValueType::STRING, structType);
    }
    return VisitResult(elemPtr, ValueType::POINTER, structType);
  }

  if (fieldType->isStructTy()) {
    StructType *nested = cast<StructType>(fieldType);
    return VisitResult(fieldPtr, ValueType::POINTER, nested);
  }

  Value *fieldVal = builder.CreateLoad(fieldType, fieldPtr, fieldName);
  ValueType resultType = typeHelper.getValueTypeFromType(fieldType);

  if (fieldType->isPointerTy() && node && node->inferred_type) {
    Type *inferredLLVM = getLLVMTypeFromTypeInfo(node->inferred_type);
    if (inferredLLVM && inferredLLVM->isIntegerTy() &&
        fieldType->isPointerTy()) {
      Value *intVal = builder.CreatePtrToInt(
          fieldVal, Type::getInt64Ty(context), "adt_payload_ptrtoint");
      fieldVal = builder.CreateIntCast(intVal, inferredLLVM, true,
                                       "adt_payload_intcast");
      resultType = typeHelper.getValueTypeFromType(inferredLLVM);
      return VisitResult(fieldVal, resultType);
    }
    if (inferredLLVM && inferredLLVM->isPointerTy() &&
        fieldType->isPointerTy() && inferredLLVM != fieldType) {
      fieldVal =
          builder.CreateBitCast(fieldVal, inferredLLVM, "adt_payload_bcast");
      resultType = typeHelper.getValueTypeFromType(inferredLLVM);
      return VisitResult(fieldVal, resultType);
    }
  }

  if (fieldType->isPointerTy()) {
    Type *inferredElem = getInferredPointerElementType(node);
    if (inferredElem) {
      pointerElementHints[fieldVal] = inferredElem;
    } else {
      std::string sname = structType->getName().str();
      Type* fieldArrElem = typeHelper.getFieldArrayElementType(sname, fieldName);
      if (fieldArrElem) {
        pointerElementHints[fieldVal] = fieldArrElem;
      } else if (fieldName == "scopes") {
        pointerElementHints[fieldVal] = PointerType::get(context, 0);
      } else {
        pointerElementHints[fieldVal] = Type::getInt32Ty(context);
      }
    }
  }

  return VisitResult(fieldVal, resultType, structType);
}

LLVMCodeGenerator::VisitResult
LLVMCodeGenerator::visitStructAssign(ASTNode *node) {
  if (!node || node->type != AST_ASSIGN)
    return VisitResult();

  ASTNode *left = node->data.assign.left;
  ASTNode *right = node->data.assign.right;

  if (!left || left->type != AST_IDENTIFIER)
    return VisitResult();
  if (!right || right->type != AST_STRUCT_LITERAL)
    return VisitResult();

  std::string varName(left->data.identifier.name);

  ASTNode *typeNameNode = right->data.struct_literal.type_name;
  if (!typeNameNode)
    return VisitResult();

  Type *rawType = typeHelper.getTypeFromTypeNode(typeNameNode);
  StructType *structType =
      rawType && rawType->isStructTy() ? cast<StructType>(rawType) : nullptr;
  if (!structType)
    return VisitResult();

  Function *func = getCurrentFunction();
  if (!func) {
    func = module->getFunction("main");
    if (!func) {
      createDefaultMain();
      func = module->getFunction("main");
    }
    if (func) {
      builder.SetInsertPoint(&func->getEntryBlock());
    }
  }

  if (!func)
    return VisitResult();
  BasicBlock *entryBB = &func->getEntryBlock();
  BasicBlock *savedBB = builder.GetInsertBlock();

  IRBuilder<> tempBuilder(entryBB, entryBB->begin());
  AllocaInst *alloc = tempBuilder.CreateAlloca(structType, nullptr, varName);

  if (savedBB) {
    builder.SetInsertPoint(savedBB);
  }

  scopeManager.defineVariable(varName, alloc);
  initStructLiteral(alloc, right);

  ASTNode *initFields = right->data.struct_literal.fields;
  if (initFields && initFields->type == AST_EXPRESSION_LIST) {
    int initCount = initFields->data.expression_list.expression_count;
    for (int i = 0; i < initCount; i++) {
      ASTNode *f = initFields->data.expression_list.expressions[i];
      if (!f || f->type != AST_ASSIGN || !f->data.assign.left ||
          !f->data.assign.right)
        continue;
      ASTNode *lhs = f->data.assign.left;
      ASTNode *rhs = f->data.assign.right;
      if (lhs->type != AST_IDENTIFIER || !lhs->data.identifier.name)
        continue;
      if (rhs->type != AST_EXPRESSION_LIST)
        continue;
      std::string key = varName + "." + std::string(lhs->data.identifier.name);
      memberArrayLengthHints[key] = rhs->data.expression_list.expression_count;
      if (rhs->data.expression_list.expression_count > 0) {
        ASTNode *first = rhs->data.expression_list.expressions[0];
        if (first && first->type == AST_EXPRESSION_LIST) {
          memberNestedArrayLengthHints[key] =
              first->data.expression_list.expression_count;
        }
      }
    }
  }

  return VisitResult(alloc, ValueType::POINTER, structType);
}

void LLVMCodeGenerator::initStructLiteral(AllocaInst *structAlloc,
                                          ASTNode *node) {
  if (!node || node->type != AST_STRUCT_LITERAL)
    return;

  ASTNode *typeName = node->data.struct_literal.type_name;
  ASTNode *fields = node->data.struct_literal.fields;
  if (!typeName)
    return;

  std::string structName;
  if (typeName->type == AST_IDENTIFIER && typeName->data.identifier.name) {
    structName = typeName->data.identifier.name;
  } else if (typeName->type == AST_TYPE_APP && typeName->data.type_app.ctor &&
             typeName->data.type_app.ctor->type == AST_IDENTIFIER &&
             typeName->data.type_app.ctor->data.identifier.name) {
    structName = typeName->data.type_app.ctor->data.identifier.name;
  } else {
    return;
  }

  StructType *structType =
      cast_or_null<StructType>(structAlloc->getAllocatedType());
  if (!structType)
    return;
  std::string mangledName(structType->getName());
  auto *fieldInfo = typeHelper.getStructFields(mangledName);
  if (!fieldInfo)
    fieldInfo = typeHelper.getStructFields(structName);
  if (!fieldInfo)
    return;

  if (!fields || fields->type != AST_EXPRESSION_LIST)
    return;

  int fieldCount = fields->data.expression_list.expression_count;
  for (size_t i = 0; i < fieldInfo->size(); i++) {
    const auto &field = (*fieldInfo)[i];
    std::string fieldName = field.first;
    Type *expectedType = field.second;

    Value *fieldPtr =
        builder.CreateStructGEP(structType, structAlloc, i, fieldName);
    Value *defaultValue = nullptr;

    if (expectedType->isPointerTy()) {
      defaultValue = ConstantPointerNull::get(cast<PointerType>(expectedType));
    } else if (expectedType->isIntegerTy()) {
      defaultValue = ConstantInt::get(expectedType, 0);
    } else if (expectedType->isFloatTy()) {
      defaultValue = ConstantFP::get(expectedType, 0.0);
    } else if (expectedType->isDoubleTy()) {
      defaultValue = ConstantFP::get(expectedType, 0.0);
    } else if (expectedType->isStructTy()) {
      defaultValue = Constant::getNullValue(expectedType);
    } else {
      continue;
    }

    builder.CreateStore(defaultValue, fieldPtr);
  }
  for (int i = 0; i < fieldCount && i < (int)fieldInfo->size(); i++) {
    ASTNode *fieldExpr = fields->data.expression_list.expressions[i];

    if (fieldExpr && fieldExpr->type == AST_ASSIGN) {
      ASTNode *fieldNameNode = fieldExpr->data.assign.left;
      ASTNode *exprNode = fieldExpr->data.assign.right;

      if (fieldNameNode && fieldNameNode->type == AST_IDENTIFIER && exprNode) {
        std::string fieldName(fieldNameNode->data.identifier.name);

        int idx = -1;
        for (size_t j = 0; j < fieldInfo->size(); j++) {
          if ((*fieldInfo)[j].first == fieldName) {
            idx = j;
            break;
          }
        }

        if (idx >= 0) {
          VisitResult fieldVal = visit(exprNode);
          if (fieldVal.value) {
            Value *fieldPtr = builder.CreateStructGEP(structType, structAlloc,
                                                      idx, fieldName);
            Type *expectedType = (*fieldInfo)[idx].second;

            if (expectedType->isStructTy()) {
              Value *toStore = nullptr;
              if (fieldVal.value->getType()->isPointerTy()) {
                toStore = builder.CreateLoad(expectedType, fieldVal.value,
                                             fieldName + "_val");
              } else if (fieldVal.value->getType() == expectedType) {
                toStore = fieldVal.value;
              }
              if (toStore)
                builder.CreateStore(toStore, fieldPtr);
            } else if (expectedType->isPointerTy() &&
                       fieldVal.value->getType()->isPointerTy()) {
              Value *storeVal = fieldVal.value;
              if (storeVal->getType() != expectedType) {
                storeVal = builder.CreateBitCast(storeVal, expectedType,
                                                 "field_ptr_cast");
              }
              builder.CreateStore(storeVal, fieldPtr);
            } else {
              ValueType expectedValueType =
                  typeHelper.getValueTypeFromType(expectedType);
              Value *castedVal = typeHelper.castValue(
                  builder, fieldVal.value, fieldVal.type, expectedValueType);
              builder.CreateStore(castedVal, fieldPtr);
            }
          }
        }
      }
    }
  }
}

LLVMCodeGenerator::VisitResult
LLVMCodeGenerator::visitMemberAssign(ASTNode *node) {
  if (!node || node->type != AST_ASSIGN)
    return VisitResult();

  ASTNode *left = node->data.assign.left;
  ASTNode *right = node->data.assign.right;
  if (!left || left->type != AST_MEMBER_ACCESS)
    return VisitResult();

  ASTNode *object = left->data.member_access.object;
  ASTNode *field = left->data.member_access.field;
  if (!object || !field)
    return VisitResult();

  VisitResult objectRes = visit(object);
  if (!objectRes.value)
    return VisitResult();

  if (!objectRes.value->getType()->isPointerTy())
    return VisitResult();
  if (field->type != AST_IDENTIFIER)
    return VisitResult();

  std::string fieldName(field->data.identifier.name);

  StructType *structType = nullptr;
  Value *basePtr = nullptr;

  if (objectRes.structType) {
    structType = objectRes.structType;
    basePtr = objectRes.value;
  } else if (AllocaInst *alloc = dyn_cast<AllocaInst>(objectRes.value)) {
    Type *allocatedType = getActualType(alloc);
    if (allocatedType->isStructTy()) {
      structType = cast<StructType>(allocatedType);
      basePtr = alloc;
    }
  } else if (LoadInst *load = dyn_cast<LoadInst>(objectRes.value)) {
    Value *ptr = load->getPointerOperand();
    if (ptr->getType()->isPointerTy()) {
      if (AllocaInst *allocPtr = dyn_cast<AllocaInst>(ptr)) {
        Type *allocatedType = getActualType(allocPtr);
        if (allocatedType->isStructTy()) {
          structType = cast<StructType>(allocatedType);
          basePtr = ptr;
        }
      }
    }
  }

  if (!structType || !basePtr) {
    // Try pointerElementHints for struct pointer params
    if (objectRes.value->getType()->isPointerTy()) {
      auto hintIt = pointerElementHints.find(objectRes.value);
      if (hintIt == pointerElementHints.end()) {
        if (LoadInst *load = dyn_cast<LoadInst>(objectRes.value)) {
          Value *ptr = load->getPointerOperand();
          if (AllocaInst *allocPtr = dyn_cast<AllocaInst>(ptr)) {
            auto allocHintIt = pointerElementHints.find(allocPtr);
            if (allocHintIt != pointerElementHints.end() && allocHintIt->second &&
                allocHintIt->second->isStructTy()) {
              pointerElementHints[objectRes.value] = allocHintIt->second;
              hintIt = pointerElementHints.find(objectRes.value);
            }
          }
        }
      }
      if (hintIt != pointerElementHints.end() && hintIt->second &&
          hintIt->second->isStructTy()) {
        structType = cast<StructType>(hintIt->second);
        basePtr = objectRes.value;
      }
    }
    if (!structType && objectRes.value->getType()->isPointerTy()) {
      // Try loading pointer from alloca and checking hints
      if (AllocaInst *alloc = dyn_cast<AllocaInst>(objectRes.value)) {
        Type *allocatedType = getActualType(alloc);
        if (allocatedType->isPointerTy()) {
          auto hintIt = pointerElementHints.find(alloc);
          if (hintIt != pointerElementHints.end() && hintIt->second &&
              hintIt->second->isStructTy()) {
            structType = cast<StructType>(hintIt->second);
            basePtr = builder.CreateLoad(allocatedType, alloc, "struct_ptr_ld");
          }
        }
      }
    }
  }

  if (!structType || !basePtr) {
    llvm::errs() << "Error: Cannot assign to member '" << fieldName
                 << "' of non-struct type\n";
    return VisitResult();
  }

  std::string structName = structType->getName().str();
  int idx = typeHelper.getFieldIndex(structName, fieldName);
  if (idx < 0) {
    std::string msg =
        "Struct '" + structName + "' has no member named '" + fieldName + "'";
    const char *filename =
        node && node->source_file
            ? node->source_file
            : (current_input_filename ? current_input_filename : "unknown");
    int line =
        node && node->location.first_line > 0 ? node->location.first_line : 1;
    report_semantic_error_with_location(msg.c_str(), filename, line);
    return VisitResult();
  }

  Value *fieldPtr =
      builder.CreateStructGEP(structType, basePtr, idx, fieldName);

  VisitResult rightVal = visit(right);
  if (!rightVal.value)
    return VisitResult();

  Type *fieldType = structType->getElementType(idx);
  ValueType fieldValueType = typeHelper.getValueTypeFromType(fieldType);
  Value *castedVal = typeHelper.castValue(builder, rightVal.value,
                                          rightVal.type, fieldValueType);

  builder.CreateStore(castedVal, fieldPtr);

  return VisitResult(castedVal, fieldValueType, structType);
}
