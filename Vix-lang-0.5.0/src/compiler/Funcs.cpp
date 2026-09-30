#include "Codegen.h"

using namespace llvm;

    void LLVMCodeGenerator::preDeclareFunction(ASTNode* node) {
        if (!node || node->type != AST_FUNCTION || !node->data.function.name) return;
        std::string funcName(node->data.function.name);
        if (module->getFunction(funcName)) return;

        std::vector<Type*> paramTypes;
        if (node->data.function.params && node->data.function.params->type == AST_EXPRESSION_LIST) {
            int paramCount = node->data.function.params->data.expression_list.expression_count;
            for (int i = 0; i < paramCount; i++) {
                ASTNode* param = node->data.function.params->data.expression_list.expressions[i];
                if (!param) {
                    paramTypes.push_back(Type::getInt32Ty(context));
                    continue;
                }
                ASTNode* typeNode = nullptr;
                if (param->type == AST_ASSIGN) {
                    typeNode = param->data.assign.right;
                } else if (param->type == AST_IDENTIFIER) {
                    paramTypes.push_back(Type::getInt32Ty(context));
                    continue;
                }
                if (typeNode) {
                    paramTypes.push_back(typeHelper.typeNodeToLLVM(typeNode));
                } else {
                    paramTypes.push_back(Type::getInt32Ty(context));
                }
            }
        }

        Type* returnType = Type::getVoidTy(context);
        if (node->data.function.return_type) {
            returnType = typeHelper.typeNodeToLLVM(node->data.function.return_type);
        }

        bool isVarArg = node->data.function.vararg == 1;
        FunctionType* funcType = FunctionType::get(returnType, paramTypes, isVarArg);
        Function::Create(funcType, Function::ExternalLinkage, funcName, module.get());
    }

    LLVMCodeGenerator::VisitResult LLVMCodeGenerator::visitFunction(ASTNode* node, const std::string* overrideName) {
        std::string funcName = overrideName ? *overrideName : std::string(node->data.function.name);
        IRBuilder<>::InsertPoint callerIP = builder.saveIP();
        
        if (Function* existingFunc = module->getFunction(funcName)) {
            if (!existingFunc->isDeclaration()) {
                StructType* sretStructType = getStructSRetType(existingFunc);
                if (sretStructType) {
                    return VisitResult(existingFunc, ValueType::POINTER, sretStructType);
                }
                return VisitResult(existingFunc, ValueType::POINTER);
            }
            existingFunc->eraseFromParent();
        }
        
        std::vector<Type*> paramTypes;
        std::vector<std::string> paramNames;
        std::vector<ValueType> paramValueTypes;
        functionArrayParamPositions[funcName].clear();
        paramStructTypes.clear();
        
        if (node->data.function.params) {
            if (node->data.function.params->type == AST_EXPRESSION_LIST) {
                int paramCount = node->data.function.params->data.expression_list.expression_count;
                int userParamIndex = 0;
                for (int i = 0; i < paramCount; i++) {
                    ASTNode* param = node->data.function.params->data.expression_list.expressions[i];
                    
                    if (param->type == AST_ASSIGN) {
                        ASTNode* left = param->data.assign.left;
                        ASTNode* right = param->data.assign.right;
                        if (left && left->type == AST_IDENTIFIER) {
                            std::string paramName(left->data.identifier.name);
                            paramNames.push_back(paramName);
                            ValueType paramType = ValueType::INT32;
                            bool isArrayParam = false;
                            Type* arrayElementType = Type::getInt32Ty(context);
                            int arrayElementCount = -1;
                            if (right && right->type == AST_IDENTIFIER) {
                                std::string typeName(right->data.identifier.name);
                                auto gbt = activeGenericTypeBindings.find(typeName);
                                if (gbt != activeGenericTypeBindings.end() && gbt->second) {
                                    Type* llvmParamType = gbt->second;
                                    if (llvmParamType->isStructTy()) {
                                        paramType = ValueType::POINTER;
                                        paramTypes.push_back(PointerType::get(context, 0));
                                    } else {
                                        paramType = typeHelper.getValueTypeFromType(llvmParamType);
                                        paramTypes.push_back(llvmParamType);
                                    }
                                } else if (StructType* structTy = typeHelper.getStructType(typeName)) {
                                    paramType = ValueType::POINTER;
                                    paramTypes.push_back(PointerType::get(context, 0));
                                    paramStructTypes[paramName] = structTy;
                                } else if (vix_is_adt_definition(typeName.c_str())) {
                                    paramType = ValueType::POINTER;
                                    paramTypes.push_back(PointerType::get(context, 0));
                                    StructType* adtStructTy = StructType::get(context, {Type::getInt32Ty(context), PointerType::get(context, 0)});
                                    paramStructTypes[paramName] = adtStructTy;
                                } else if (typeName == "ptr") {
                                    if (funcName == "main" && paramName == "argv") {
                                        paramType = ValueType::POINTER;
                                        paramTypes.push_back(PointerType::get(context, 0));
                                    } else {
                                        paramType = ValueType::POINTER;
                                        paramTypes.push_back(PointerType::get(context, 0));
                                        typeHelper.registerStringVariable(paramName);//未指明更具体类型的 ptr 参数，通常按字符串/字节指针处理
                                    }
                                } else if (typeName == "i32") {
                                    paramType = ValueType::INT32;
                                    paramTypes.push_back(Type::getInt32Ty(context));
                                } else if (typeName == "i64") {
                                    paramType = ValueType::INT64;
                                    paramTypes.push_back(Type::getInt64Ty(context));
                                } else if (typeName == "f32") {
                                    paramType = ValueType::FLOAT32;
                                    paramTypes.push_back(Type::getFloatTy(context));
                                } else if (typeName == "f64") {
                                    paramType = ValueType::FLOAT64;
                                    paramTypes.push_back(Type::getDoubleTy(context));
                                } else if (typeName == "str") {
                                    paramType = ValueType::STRING;
                                    paramTypes.push_back(PointerType::get(context, 0));
                                } else {
                                    paramTypes.push_back(Type::getInt32Ty(context));
                                }
                            }
                            else if (right) {
                                paramType = typeHelper.fromTypeNode(right);
                                if (right->type == AST_TYPE_POINTER) {
                                    if (funcName == "main" && paramName == "argv") {
                                        paramTypes.push_back(PointerType::get(context, 0));
                                    } else {
                                        paramTypes.push_back(PointerType::get(context, 0));
                                        ASTNode* elemTypeNode = right->data.pointer_type.element_type;
                                        if (elemTypeNode) {
                                            Type* elemLLVM = typeHelper.getTypeFromTypeNode(elemTypeNode);
                                            if (elemLLVM && elemLLVM->isStructTy()) {
                                                paramStructTypes[paramName] = cast<StructType>(elemLLVM);
                                            } else {
                                                typeHelper.registerArrayType(paramName, Type::getInt32Ty(context), -1);
                                            }
                                        } else {
                                            typeHelper.registerArrayType(paramName, Type::getInt32Ty(context), -1);
                                        }
                                    }
                                } else if (right->type == AST_TYPE_LIST || right->type == AST_TYPE_FIXED_SIZE_LIST) {
                                    Type* elemType = typeHelper.getArrayElementTypeFromNode(right);
                                    int elemCount = typeHelper.getArrayElementCountFromNode(right);
                                    paramType = ValueType::POINTER;
                                    paramTypes.push_back(PointerType::get(context, 0));
                                    typeHelper.registerArrayType(paramName, elemType, elemCount > 0 ? elemCount : -1);
                                    isArrayParam = true;
                                    arrayElementType = elemType;
                                    arrayElementCount = (elemCount > 0 ? elemCount : -1);
                                } else {
                                    Type* llvmParamType = typeHelper.getTypeFromTypeNode(right);
                                    if (llvmParamType && llvmParamType->isStructTy()) {
                                        paramType = ValueType::POINTER;
                                        paramTypes.push_back(PointerType::get(context, 0));
                                    } else {
                                        paramType = typeHelper.getValueTypeFromType(llvmParamType);
                                        paramTypes.push_back(llvmParamType);
                                    }
                                }
                            } else {
                                paramTypes.push_back(Type::getInt32Ty(context));
                            }
                            paramValueTypes.push_back(paramType);
                            if (paramType == ValueType::STRING) {
                                typeHelper.registerStringVariable(paramName);
                            }

                            if (isArrayParam) {
                                functionArrayParamPositions[funcName].push_back(userParamIndex);
                                paramNames.push_back(paramName + "__len");
                                paramTypes.push_back(Type::getInt32Ty(context));
                                paramValueTypes.push_back(ValueType::INT32);
                                typeHelper.registerArrayType(paramName, arrayElementType, arrayElementCount);
                                typeHelper.registerVariableArraySize(paramName, arrayElementCount);
                            }

                            userParamIndex++;
                        }
                    } else if (param->type == AST_IDENTIFIER) {
                        std::string paramName(param->data.identifier.name);
                        paramNames.push_back(paramName);
                        paramValueTypes.push_back(ValueType::INT32);
                        paramTypes.push_back(Type::getInt32Ty(context));
                        userParamIndex++;
                    }
                }
            }
        }
        typeHelper.setGenericTypeBindings(activeGenericTypeBindings);

        Type* logicalReturnType = Type::getVoidTy(context);
        if (node->data.function.return_type) {
            logicalReturnType = typeHelper.getTypeFromTypeNode(node->data.function.return_type);
            typeHelper.getValueTypeFromType(logicalReturnType);
        }

        StructType* logicalReturnStructType = nullptr;
        bool useStructSRet = logicalReturnType && logicalReturnType->isStructTy();
        Type* abiReturnType = logicalReturnType;
        functionArrayReturnElementTypes.erase(funcName);
        if (node->data.function.return_type && node->data.function.return_type->type == AST_TYPE_LIST) {
            Type* elemType = typeHelper.getArrayElementTypeFromNode(node->data.function.return_type);
            functionArrayReturnElementTypes[funcName] = elemType;
        }
        if (useStructSRet) {
            logicalReturnStructType = cast<StructType>(logicalReturnType);
            abiReturnType = Type::getVoidTy(context);
            paramTypes.insert(paramTypes.begin(), PointerType::get(context, 0));
        }

        bool isVarArg = node->data.function.vararg == 1;
        FunctionType* funcType = FunctionType::get(abiReturnType, paramTypes, isVarArg);
        Function* func = Function::Create(funcType, Function::ExternalLinkage, funcName, module.get());
        if (useStructSRet && logicalReturnStructType) {
            registerStructSRetFunction(funcName, func, logicalReturnStructType);
            func->addParamAttr(0, Attribute::getWithStructRetType(context, logicalReturnStructType));
            func->addParamAttr(0, Attribute::NoAlias);
        }
        auto fit = sourceAttrs.functionAttrs.find(funcName);
        if (fit != sourceAttrs.functionAttrs.end()) {
            if (!fit->second.section.empty()) {
                func->setSection(fit->second.section);
            }
            if (fit->second.exported) {
                func->setLinkage(GlobalValue::ExternalLinkage);
            }
        }//对于声明但未定义的外部函数，直接返回函数对象，不生成函数体
        if (node->data.function.is_extern && node->data.function.body == NULL) {
            return VisitResult(func, ValueType::POINTER, logicalReturnStructType);
        }
        
        if (funcName == "main") {
            mainFunctionCreated = true;
        }
        Function* prevFunc = scopeManager.getCurrentFunction();
        scopeManager.setCurrentFunction(func);
        BasicBlock* entryBB = BasicBlock::Create(context, "entry", func);
        builder.SetInsertPoint(entryBB);
        scopeManager.enterScope();
        unsigned idx = 0;
        unsigned userIdx = 0;
        for (auto& arg : func->args()) {
            if (useStructSRet && idx == 0) {
                arg.setName("__sret");
                ++idx;
                continue;
            }

            if (userIdx < paramNames.size()) {
                arg.setName(paramNames[userIdx]);
                Type* paramAllocType = arg.getType();
                
                AllocaInst* alloc = builder.CreateAlloca(paramAllocType, nullptr, paramNames[userIdx]);
                builder.CreateStore(&arg, alloc);
                scopeManager.defineVariable(paramNames[userIdx], alloc);
                if (userIdx < paramValueTypes.size() && paramValueTypes[userIdx] == ValueType::STRING) {
                    typeHelper.registerStringVariable(paramNames[userIdx]);
                }
                // Track struct types for pointer parameters
                if (paramAllocType->isPointerTy() && userIdx < paramNames.size()) {
                    auto stIt = paramStructTypes.find(paramNames[userIdx]);
                    if (stIt != paramStructTypes.end() && stIt->second) {
                        pointerElementHints[alloc] = stIt->second;
                    }
                }
                userIdx++;
            }
            ++idx;
        }
        
        ASTNode* body = node->data.function.body;
        VisitResult lastBodyResult;
        if (body) {
            if (body->type == AST_PROGRAM) {
                for (int i = 0; i < body->data.program.statement_count; i++) {
                    lastBodyResult = visit(body->data.program.statements[i]);
                }
            } else {
                lastBodyResult = visit(body);
            }
        }

        if (!useStructSRet && !logicalReturnType->isVoidTy()) {
            BasicBlock* curBB = builder.GetInsertBlock();
            if (curBB && !curBB->getTerminator() && lastBodyResult.value) {
                /* Auto-deref: if returning a pointer but expected type is a value, load from the pointer */
                bool lastIsPtr = (lastBodyResult.type == ValueType::POINTER || lastBodyResult.type == ValueType::STRING) && lastBodyResult.value->getType()->isPointerTy();
                if (lastIsPtr && !logicalReturnType->isPointerTy()) {
                    Value* loaded = builder.CreateLoad(logicalReturnType, lastBodyResult.value, "autoderef_ret");
                    builder.CreateRet(loaded);
                } else {
                    ValueType expectedValueType = typeHelper.getValueTypeFromType(logicalReturnType);
                    Value* retValue = typeHelper.castValue(builder, lastBodyResult.value, lastBodyResult.type, expectedValueType);
                    if (logicalReturnType->isPointerTy() && retValue->getType()->isPointerTy() &&
                        retValue->getType() != logicalReturnType) {
                        retValue = builder.CreateBitCast(retValue, logicalReturnType, "ret_ptr_cast");
                    }
                    builder.CreateRet(retValue);
                }
            }
        }

        if (useStructSRet) {
            BasicBlock* curBB = builder.GetInsertBlock();
            if (curBB && !curBB->getTerminator() && lastBodyResult.value && logicalReturnStructType) {
                Argument* sretArg = func->arg_begin();
                Value* sretPtr = sretArg;
                Type* expectSretPtrType = PointerType::get(context, 0);
                if (sretPtr->getType() != expectSretPtrType) {
                    sretPtr = builder.CreateBitCast(sretPtr, expectSretPtrType, "fn_sret_ptrcast");
                }

                Value* retStructValue = nullptr;
                if (lastBodyResult.value->getType() == logicalReturnStructType) {
                    retStructValue = lastBodyResult.value;
                } else if (lastBodyResult.value->getType()->isPointerTy()) {
                    Value* srcPtr = lastBodyResult.value;
                    if (srcPtr->getType() != expectSretPtrType) {
                        srcPtr = builder.CreateBitCast(srcPtr, expectSretPtrType, "fn_sret_src_ptrcast");
                    }
                    retStructValue = builder.CreateLoad(logicalReturnStructType, srcPtr, "fn_sret_val");
                }

                if (retStructValue) {
                    builder.CreateStore(retStructValue, sretPtr);
                    builder.CreateRetVoid();
                }
            }
        }
        
        scopeManager.exitScope();
        BasicBlock* endBB = BasicBlock::Create(context, "func_end", func);
        std::vector<BasicBlock*> blocks;
        for (auto& BB : *func) {
            blocks.push_back(&BB);
        }
        
        for (BasicBlock* BB : blocks) {
            if (BB == endBB) continue;
            if (!BB->getTerminator()) {
                IRBuilder<> tmpBuilder(BB);
                tmpBuilder.CreateBr(endBB);
            }
        }
        builder.SetInsertPoint(endBB);
        if (useStructSRet || logicalReturnType->isVoidTy()) {
            builder.CreateRetVoid();
        } else {
            Value* defaultRetVal = nullptr;
            if (logicalReturnType->isIntegerTy()) {
                defaultRetVal = ConstantInt::get(logicalReturnType, 0);
            } else if (logicalReturnType->isFloatTy()) {
                defaultRetVal = ConstantFP::get(logicalReturnType, 0.0);
            } else if (logicalReturnType->isDoubleTy()) {
                defaultRetVal = ConstantFP::get(logicalReturnType, 0.0);
            } else if (logicalReturnType->isPointerTy()) {
                defaultRetVal = ConstantPointerNull::get(cast<PointerType>(logicalReturnType));
            } else {
                defaultRetVal = Constant::getNullValue(logicalReturnType);
            }
            builder.CreateRet(defaultRetVal);
        }
        scopeManager.setCurrentFunction(prevFunc);
        if (callerIP.isSet()) {
            builder.restoreIP(callerIP);
        } else {
            builder.ClearInsertionPoint();
        }
        return VisitResult(func, ValueType::POINTER, logicalReturnStructType);
    }
    
    LLVMCodeGenerator::VisitResult LLVMCodeGenerator::visitCall(ASTNode* node) {
        if (!node->data.call.func) return VisitResult();

        if (node->data.call.func->type == AST_MEMBER_ACCESS) {
            ASTNode* mem = node->data.call.func;
            ASTNode* objectNode = mem->data.member_access.object;
            ASTNode* fieldNode = mem->data.member_access.field;
            if (!objectNode || !fieldNode || fieldNode->type != AST_IDENTIFIER || !fieldNode->data.identifier.name) {
                return VisitResult();
            }

            std::string methodName(fieldNode->data.identifier.name);

            /* Try impl method resolution first */
            {
                std::string typeName;
                /* Get type name from the object's inferred_type */
                VIX_DEBUG_LOG << "[DEBUG] impl method lookup: object has inferred_type=" 
                              << (objectNode->inferred_type ? "yes" : "no")
                              << " methodName=" << methodName << "\n";
                if (objectNode->inferred_type) {
                    const TypeInfo* ti = objectNode->inferred_type;
                    VIX_DEBUG_LOG << "[DEBUG] impl method lookup: object kind=" << ti->kind 
                                  << " methodName=" << methodName << "\n";
                    if (ti->kind == TYPEINFO_APP && ti->app_ctor && ti->app_ctor->name) {
                        typeName = ti->app_ctor->name;
                    } else if (ti->kind == TYPEINFO_STRUCT && ti->name) {
                        typeName = ti->name;
                    } else if (ti->kind == TYPEINFO_STRING) {
                        typeName = "string";
                    } else if (ti->kind == TYPEINFO_I32) {
                        typeName = "i32";
                    } else if (ti->kind == TYPEINFO_I64) {
                        typeName = "i64";
                    } else if (ti->kind == TYPEINFO_F64) {
                        typeName = "f64";
                    } else if (ti->kind == TYPEINFO_F32) {
                        typeName = "f32";
                    } else if (ti->kind == TYPEINFO_I8) {
                        typeName = "i8";
                    }
                }
                /* Check if object is a type name (static method call) */
                if (typeName.empty() && objectNode->type == AST_IDENTIFIER && objectNode->data.identifier.name) {
                    std::string objName(objectNode->data.identifier.name);
                    if (typeHelper.getStructType(objName) || vix_is_adt_definition(objName.c_str())) {
                        typeName = objName;
                    }
                }
                /* Fallback: get type name from struct type */
                if (typeName.empty()) {
                    VisitResult objRes = visit(objectNode);
                    if (objRes.structType) {
                        typeName = objRes.structType->getName().str();
                    } else if (objRes.type == ValueType::STRING) {
                        typeName = "string";
                    }
                }
                if (!typeName.empty()) {
                    const char* mangledFunc = vix_lookup_impl_method(typeName.c_str(), methodName.c_str());
                    if (mangledFunc) {
                        /* Check if object is a type name (static method call) */
                        bool isStaticCall = false;
                        if (objectNode->type == AST_IDENTIFIER && objectNode->data.identifier.name) {
                            std::string objName(objectNode->data.identifier.name);
                            if (typeHelper.getStructType(objName) || vix_is_adt_definition(objName.c_str())) {
                                isStaticCall = true;
                            }
                        }

                        /* Look up the function in the module */
                        Function* implFunc = module->getFunction(mangledFunc);
                        if (!implFunc) {
                            /* Try generic instantiation */
                            auto fit = genericFunctionTemplates.find(mangledFunc);
                            if (fit != genericFunctionTemplates.end() && node->data.call.type_args) {
                                implFunc = instantiateGenericFunction(mangledFunc, node->data.call.type_args);
                            }
                        }
                        if (implFunc) {
                            /* Build call arguments */
                            std::vector<Value*> callArgs;
                            FunctionType* fnTy = implFunc->getFunctionType();
                            unsigned paramIdx = 0;
                            /* Check if the function has a 'self' parameter */
                            if (implFunc->arg_begin() != implFunc->arg_end()) {
                                auto firstArg = implFunc->arg_begin();
                                if (firstArg->hasName() && firstArg->getName() == "self") {
                                }
                            }
                            /* If not static call, first arg is the object */
                            if (!isStaticCall) {
                                VisitResult objRes = visit(objectNode);
                                if (!objRes.value) return VisitResult();
                                if (paramIdx < fnTy->getNumParams()) {
                                    Type* expectedTy = fnTy->getParamType(paramIdx);
                                    Value* argVal = objRes.value;
                                    if (argVal->getType() != expectedTy) {
                                        if (expectedTy->isPointerTy() && argVal->getType()->isPointerTy()) {
                                            argVal = builder.CreateBitCast(argVal, expectedTy, "impl_self_cast");
                                        } else if (expectedTy->isIntegerTy() && argVal->getType()->isIntegerTy()) {
                                            argVal = builder.CreateIntCast(argVal, expectedTy, true, "impl_self_intcast");
                                        }
                                    }
                                    callArgs.push_back(argVal);
                                    paramIdx++;
                                }
                            }
                            /* Remaining args */
                            ASTNode* callArgs_node = node->data.call.args;
                            if (callArgs_node && callArgs_node->type == AST_EXPRESSION_LIST) {
                                for (int i = 0; i < callArgs_node->data.expression_list.expression_count; i++) {
                                    VisitResult argRes = visit(callArgs_node->data.expression_list.expressions[i]);
                                    if (!argRes.value) return VisitResult();
                                    if (paramIdx < fnTy->getNumParams()) {
                                        Type* expectedTy = fnTy->getParamType(paramIdx);
                                        Value* argVal = argRes.value;
                                        if (argVal->getType() != expectedTy) {
                                            if (expectedTy->isPointerTy() && argVal->getType()->isPointerTy()) {
                                                argVal = builder.CreateBitCast(argVal, expectedTy, "impl_arg_cast");
                                            } else if (expectedTy->isIntegerTy() && argVal->getType()->isIntegerTy()) {
                                                argVal = builder.CreateIntCast(argVal, expectedTy, true, "impl_arg_intcast");
                                            }
                                        }
                                        callArgs.push_back(argVal);
                                    }
                                    paramIdx++;
                                }
                            }
                            CallInst* call = builder.CreateCall(implFunc, callArgs);
                            if (!implFunc->getReturnType()->isVoidTy())
                                call->setName("impl_calltmp");
                            ValueType retType = typeHelper.getValueTypeFromType(implFunc->getReturnType());
                            return VisitResult(call, retType);
                        }
                    }
                }
            }

            if (methodName == "push") {
                if (!node->data.call.args || node->data.call.args->type != AST_EXPRESSION_LIST ||
                    node->data.call.args->data.expression_list.expression_count != 1) {
                    llvm::errs() << "Error: push expects exactly one argument\n";
                    return VisitResult();
                }

                std::string objectName;
                if (objectNode->type == AST_IDENTIFIER && objectNode->data.identifier.name) {
                    objectName = objectNode->data.identifier.name;
                }

                VisitResult objectRes = visit(objectNode);
                if (!objectRes.value || !objectRes.value->getType()->isPointerTy()) {
                    return VisitResult();
                }

                ASTNode* argNode = node->data.call.args->data.expression_list.expressions[0];
                VisitResult argRes = visit(argNode);
                if (!argRes.value) return VisitResult();

                AllocaInst* objectAlloc = nullptr;
                GlobalVariable* objectGlobal = nullptr;
                Value* objectSlotPtr = nullptr;
                Type* objectSlotElemType = nullptr;
                Value* objectFieldPtr = nullptr;
                Type* objectFieldType = nullptr;
                if (!objectName.empty()) {
                    objectAlloc = scopeManager.findVariable(objectName);
                    if (!objectAlloc) objectAlloc = findVariableInMain(objectName);
                    if (!objectAlloc) objectGlobal = findGlobalVariable(objectName);
                }

                if (!objectAlloc && objectNode->type == AST_MEMBER_ACCESS) {
                    ASTNode* baseObject = objectNode->data.member_access.object;
                    ASTNode* baseField = objectNode->data.member_access.field;
                    if (baseObject && baseField && baseField->type == AST_IDENTIFIER && baseField->data.identifier.name) {
                        std::string fieldName(baseField->data.identifier.name);
                        VisitResult baseRes = visit(baseObject);
                        StructType* structType = nullptr;
                        Value* basePtr = nullptr;

                        if (baseRes.structType) {
                            structType = baseRes.structType;
                            basePtr = baseRes.value;
                        }

                        if (!structType && baseObject->type == AST_IDENTIFIER && baseObject->data.identifier.name) {
                            std::string baseName(baseObject->data.identifier.name);
                            AllocaInst* baseAlloc = scopeManager.findVariable(baseName);
                            if (!baseAlloc) baseAlloc = findVariableInMain(baseName);
                            if (baseAlloc) {
                                Type* allocType = getActualType(baseAlloc);
                                if (allocType && allocType->isStructTy()) {
                                    structType = cast<StructType>(allocType);
                                    basePtr = baseAlloc;
                                } else if (allocType && allocType->isPointerTy()) {
                                    auto hintIt = pointerElementHints.find(baseAlloc);
                                    if (hintIt != pointerElementHints.end() && hintIt->second && hintIt->second->isStructTy()) {
                                        structType = cast<StructType>(hintIt->second);
                                        basePtr = builder.CreateLoad(allocType, baseAlloc, fieldName + "_push_base_ld");
                                    }
                                }
                            }
                        }

                        if (structType && basePtr) {
                            std::string structName = structType->getName().str();
                            int idx = typeHelper.getFieldIndex(structName, fieldName);
                            if (idx >= 0) {
                                objectFieldPtr = builder.CreateStructGEP(structType, basePtr, idx, fieldName + "_push_addr");
                                objectFieldType = structType->getElementType(idx);
                            }
                        }
                    }
                }

                if (!objectAlloc && objectNode->type == AST_INDEX) {
                    ASTNode* slotTarget = objectNode->data.index.target;
                    ASTNode* slotIndexExpr = objectNode->data.index.index;
                    VisitResult slotIdxRes = visit(slotIndexExpr);
                    VisitResult slotTargetRes = visit(slotTarget);
                    if (slotIdxRes.value && slotTargetRes.value && slotTargetRes.value->getType()->isPointerTy()) {
                        Value* slotIdxVal = slotIdxRes.value;
                        if (!slotIdxVal->getType()->isIntegerTy(32)) {
                            slotIdxVal = builder.CreateIntCast(slotIdxVal, Type::getInt32Ty(context), true, "push_slot_idxcast");
                        }

                        std::string slotTargetName;
                        if (slotTarget->type == AST_IDENTIFIER && slotTarget->data.identifier.name) {
                            slotTargetName = slotTarget->data.identifier.name;
                        }
                        auto hintIt = pointerElementHints.find(slotTargetRes.value);
                        if (hintIt != pointerElementHints.end() && hintIt->second) {
                            objectSlotElemType = hintIt->second;
                        } else {
                            objectSlotElemType = getPointerElementTypeSafely(
                                dyn_cast<PointerType>(slotTargetRes.value->getType()),
                                slotTargetName
                            );
                        }
                        objectSlotPtr = builder.CreateInBoundsGEP(objectSlotElemType, slotTargetRes.value, slotIdxVal, "push_slot_ptr");
                    }
                }

                Type* elemType = Type::getInt32Ty(context);
                if (Type* inferredElemType = getInferredArrayElementType(objectNode)) {
                    elemType = inferredElemType;
                } else if (objectAlloc) {
                    Type* allocType = getActualType(objectAlloc);
                    if (allocType && allocType->isPointerTy()) {
                        elemType = getPointerElementTypeSafely(dyn_cast<PointerType>(allocType), objectName);
                    }
                } else if (objectGlobal && objectGlobal->getValueType()->isPointerTy()) {
                    elemType = getPointerElementTypeSafely(dyn_cast<PointerType>(objectGlobal->getValueType()), objectName);
                } else if (objectRes.value->getType()->isPointerTy()) {
                    auto objHintIt = pointerElementHints.find(objectRes.value);
                    if (objHintIt != pointerElementHints.end() && objHintIt->second) {
                        elemType = objHintIt->second;
                    } else if (argRes.structType &&
                               (argRes.value->getType()->isStructTy() ||
                                (argRes.value->getType()->isPointerTy() && argRes.type == ValueType::POINTER))) {
                        elemType = argRes.structType;
                    } else if (argRes.value && argRes.value->getType()->isStructTy()) {
                        elemType = argRes.value->getType();
                    } else if (argRes.value) {
                        elemType = argRes.value->getType();
                    }
                }
                if (elemType && elemType->isPointerTy()) {
                    if (argRes.structType &&
                        (argRes.value->getType()->isStructTy() ||
                         (argRes.value->getType()->isPointerTy() && argRes.type == ValueType::POINTER))) {
                        elemType = argRes.structType;
                    } else if (argRes.value && argRes.value->getType()->isStructTy()) {
                        elemType = argRes.value->getType();
                    } else if (argRes.value && argRes.value->getType()->isPointerTy()) {
                        auto argHintIt = pointerElementHints.find(argRes.value);
                        if (argHintIt != pointerElementHints.end() && argHintIt->second &&
                            argHintIt->second->isStructTy()) {
                            elemType = argHintIt->second;
                        } else if (auto* argAlloc = dyn_cast<AllocaInst>(argRes.value)) {
                            Type* argAllocTy = getActualType(argAlloc);
                            if (argAllocTy && argAllocTy->isStructTy()) {
                                elemType = argAllocTy;
                            }
                        }
                    }
                }
                if (argRes.structType &&
                    (argRes.value->getType()->isStructTy() ||
                     (argRes.value->getType()->isPointerTy() && argRes.type == ValueType::POINTER))) {
                    elemType = argRes.structType;
                } else if (argRes.value && argRes.value->getType()->isStructTy()) {
                    elemType = argRes.value->getType();
                } else if (argRes.value && argRes.value->getType()->isPointerTy()) {
                    if (auto* argAlloc = dyn_cast<AllocaInst>(argRes.value)) {
                        Type* argAllocTy = getActualType(argAlloc);
                        if (argAllocTy && argAllocTy->isStructTy()) {
                            elemType = argAllocTy;
                        }
                    }
                }

                // Fallback: if elemType is still ptr but the arg has a concrete
                // non-pointer type, use the arg's type. This handles generic
                // type params like T=i32 where TYPEINFO_VAR maps to ptr.
                if (elemType && elemType->isPointerTy() && argRes.value &&
                    argRes.value->getType() && !argRes.value->getType()->isPointerTy() &&
                    !argRes.value->getType()->isStructTy() &&
                    !(argRes.structType && (argRes.value->getType()->isStructTy() ||
                       (argRes.value->getType()->isPointerTy() && argRes.type == ValueType::POINTER)))) {
                    elemType = argRes.value->getType();
                }

                Type* storageElemType = elemType;
                ValueType elemVT = typeHelper.getValueTypeFromType(elemType);
                Value* argCast = nullptr;
                if (elemType->isStructTy()) {
                    storageElemType = PointerType::get(context, 0);
                    Value* srcStructPtr = nullptr;
                    if (argRes.value->getType()->isPointerTy()) {
                        srcStructPtr = argRes.value;
                    }

                    Function* reallocFn = getOrCreateReallocFunction();
                    uint64_t structBytes = module->getDataLayout().getTypeAllocSize(elemType);
                    Value* heapI8 = builder.CreateCall(
                        reallocFn,
                        {ConstantPointerNull::get(PointerType::get(context, 0)),
                         ConstantInt::get(Type::getInt64Ty(context), structBytes)},
                        "push_struct_heap");
                    Value* structPtr = builder.CreateBitCast(heapI8, PointerType::get(context, 0), "push_struct_ptr");
                    if (srcStructPtr) {
                        Value* srcValue = builder.CreateLoad(elemType, srcStructPtr, "push_struct_val");
                        builder.CreateStore(srcValue, structPtr);
                    } else {
                        builder.CreateStore(argRes.value, structPtr);
                    }
                    pointerElementHints[structPtr] = elemType;
                    argCast = structPtr;
                } else {
                    argCast = typeHelper.castValue(builder, argRes.value, argRes.type, elemVT);
                }
                if (argCast->getType() != storageElemType) {
                    if (argCast->getType()->isPointerTy() && storageElemType->isPointerTy()) {
                        argCast = builder.CreateBitCast(argCast, storageElemType, "push_arg_ptrcast");
                    } else if (argCast->getType()->isIntegerTy() && storageElemType->isIntegerTy()) {
                        argCast = builder.CreateIntCast(argCast, storageElemType, true, "push_arg_intcast");
                    }
                }

                std::string pushStateName = objectName;
                if (pushStateName.empty()) {
                    pushStateName = std::string("__tmp_push_") + std::to_string((uintptr_t)objectNode);
                }

                uint64_t elemBytes = module->getDataLayout().getTypeAllocSize(storageElemType);
                if (elemBytes == 0) {
                    elemBytes = 4;
                    if (storageElemType->isIntegerTy(8)) elemBytes = 1;
                    else if (storageElemType->isIntegerTy(64) || storageElemType->isDoubleTy() || storageElemType->isPointerTy()) elemBytes = 8;
                    else if (storageElemType->isFloatTy()) elemBytes = 4;
                }

                Value* oldPtr = objectRes.value;
                if (objectFieldPtr && objectFieldType) {
                    oldPtr = builder.CreateLoad(objectFieldType, objectFieldPtr, "push_field_load");
                }
                Type* targetPtrTy = PointerType::get(context, 0);
                if (oldPtr->getType() != targetPtrTy) {
                    oldPtr = builder.CreateBitCast(oldPtr, targetPtrTy, "push_old_ptr_cast");
                }

                Value* oldLen = emitLoadArrayLength(oldPtr, pushStateName + "_old_len");
                Value* newLen = builder.CreateAdd(oldLen, ConstantInt::get(Type::getInt32Ty(context), 1), pushStateName + "__len_new");

                PointerType* i8PtrTy = PointerType::get(context, 0);
                Value* oldPtrI8 = builder.CreateBitCast(oldPtr, i8PtrTy, "push_old_i8");

                Value* oldPtrInt = builder.CreatePtrToInt(oldPtrI8, Type::getInt64Ty(context));
                Value* baseInt = builder.CreateSub(oldPtrInt, ConstantInt::get(Type::getInt64Ty(context), ARRAY_HEADER_BYTES));
                Value* isOldNull = builder.CreateIsNull(oldPtrI8);
                Value* baseNonNullI8 = builder.CreateIntToPtr(baseInt, i8PtrTy);
                Value* baseI8 = builder.CreateSelect(isOldNull,
                    ConstantPointerNull::get(cast<PointerType>(i8PtrTy)),
                    baseNonNullI8, "push_base_ptr");

                Value* headerSizeVal = ConstantInt::get(Type::getInt64Ty(context), ARRAY_HEADER_BYTES);
                Value* newDataBytes = builder.CreateMul(
                    builder.CreateSExt(newLen, Type::getInt64Ty(context)),
                    ConstantInt::get(Type::getInt64Ty(context), elemBytes),
                    "push_data_bytes");
                Value* totalBytes = builder.CreateAdd(headerSizeVal, newDataBytes, "push_total_bytes");

                Function* reallocFn = getOrCreateReallocFunction();
                Value* newBaseI8 = builder.CreateCall(reallocFn, {baseI8, totalBytes}, "push_realloc");

                Value* newLenPtr = builder.CreateBitCast(newBaseI8, PointerType::get(context, 0));
                builder.CreateStore(newLen, newLenPtr);

                Value* newDataI8 = builder.CreateInBoundsGEP(Type::getInt8Ty(context), newBaseI8, headerSizeVal, "push_new_data_i8");
                Value* newPtr = builder.CreateBitCast(newDataI8, targetPtrTy, "push_new_ptr");

                Value* dstPtr = builder.CreateInBoundsGEP(storageElemType, newPtr, oldLen, "push_dst_ptr");
                builder.CreateStore(argCast, dstPtr);

                if (objectAlloc) {
                    Type* allocType = getActualType(objectAlloc);
                    Value* storePtr = newPtr;
                    if (allocType && allocType != targetPtrTy && allocType->isPointerTy()) {
                        storePtr = builder.CreateBitCast(newPtr, allocType, "push_store_ptr_cast");
                    }
                    builder.CreateStore(storePtr, objectAlloc);
                }

                if (objectGlobal) {
                    Type* globalType = objectGlobal->getValueType();
                    Value* storePtr = newPtr;
                    if (globalType && storePtr->getType() != globalType && globalType->isPointerTy()) {
                        storePtr = builder.CreateBitCast(storePtr, globalType, "push_global_store_ptr_cast");
                    }
                    builder.CreateStore(storePtr, objectGlobal);
                }

                if (objectSlotPtr && objectSlotElemType) {
                    Value* slotStorePtr = newPtr;
                    if (slotStorePtr->getType() != objectSlotElemType &&
                        slotStorePtr->getType()->isPointerTy() && objectSlotElemType->isPointerTy()) {
                        slotStorePtr = builder.CreateBitCast(slotStorePtr, objectSlotElemType, "push_slot_store_cast");
                    }
                    builder.CreateStore(slotStorePtr, objectSlotPtr);
                }

                if (objectFieldPtr && objectFieldType && objectFieldType->isPointerTy()) {
                    Value* fieldStorePtr = newPtr;
                    if (fieldStorePtr->getType() != objectFieldType) {
                        fieldStorePtr = builder.CreateBitCast(fieldStorePtr, objectFieldType, "push_field_store_cast");
                    }
                    builder.CreateStore(fieldStorePtr, objectFieldPtr);

                    if (objectNode->type == AST_MEMBER_ACCESS) {
                        ASTNode* baseObject = objectNode->data.member_access.object;
                        ASTNode* baseField = objectNode->data.member_access.field;
                        if (baseObject && baseField &&
                            baseObject->type == AST_IDENTIFIER && baseObject->data.identifier.name &&
                            baseField->type == AST_IDENTIFIER && baseField->data.identifier.name) {
                            std::string key = std::string(baseObject->data.identifier.name) + "." + std::string(baseField->data.identifier.name);
                            memberArrayLengthHints[key] = -1;
                        }
                    }
                }

                if (!objectName.empty()) {
                    typeHelper.registerArrayType(objectName, elemType, -1);
                    typeHelper.registerVariableArraySize(objectName, -1);
                }

                return VisitResult(newPtr, ValueType::POINTER);
            }

            return VisitResult();
        }

        if (node->data.call.func->type != AST_IDENTIFIER) {
            VisitResult calleeRes = visit(node->data.call.func);
            if (calleeRes.value && calleeRes.value->getType()->isPointerTy()) {
                Type* hintRetTy = nullptr;
                if (Function* curFn = getCurrentFunction()) {
                    hintRetTy = curFn->getReturnType();
                }
                return emitFunctionPointerCall(calleeRes.value, node->data.call.args, hintRetTy);
            }
            return VisitResult();
        }
        
        std::string calleeName(node->data.call.func->data.identifier.name);

        if (calleeName == VIX_STRING_SLICE_INTRINSIC) {
            if (!node->data.call.args || node->data.call.args->type != AST_EXPRESSION_LIST ||
                node->data.call.args->data.expression_list.expression_count != 3) {
                llvm::errs() << "Error: string slice expects three arguments\n";
                return VisitResult();
            }
            ASTNode** args = node->data.call.args->data.expression_list.expressions;
            VisitResult source = visit(args[0]);
            VisitResult start = visit(args[1]);
            VisitResult end = visit(args[2]);
            Value* sliced = emitStringSlice(source.value, start.value, end.value);
            return VisitResult(sliced, ValueType::STRING);
        }

        if (calleeName == "stdin") {
            int stdinArgCount = node->data.call.args ?
                node->data.call.args->data.expression_list.expression_count : 0;
            if (stdinArgCount == 0) {
                Type* i8PtrTy = PointerType::get(context, 0);
                Function* fdopenFn = module->getFunction("fdopen");
                if (!fdopenFn) {
                    FunctionType* fdopenTy = FunctionType::get(
                        i8PtrTy,
                        {Type::getInt32Ty(context), i8PtrTy},
                        false
                    );
                    fdopenFn = Function::Create(
                        fdopenTy,
                        Function::ExternalLinkage,
                        "fdopen",
                        module.get()
                    );
                    fdopenFn->setCallingConv(CallingConv::C);
                }

                Value* modeStr = safeCreateGlobalString("r", "stdin_mode");
                Value* fdZero = ConstantInt::get(Type::getInt32Ty(context), 0);
                Value* stdinVal = builder.CreateCall(fdopenFn, {fdZero, modeStr}, "stdin_val");
                return VisitResult(stdinVal, ValueType::POINTER);
            }
        }

        std::vector<PrintfFormatSpec> printfSpecs;
        if ((calleeName == "printf" || calleeName == "sprintf" || calleeName == "snprintf") &&
            node->data.call.args &&
            node->data.call.args->type == AST_EXPRESSION_LIST &&
            node->data.call.args->data.expression_list.expression_count > 0) {
            ASTNode* fmtNode = node->data.call.args->data.expression_list.expressions[0];
            if (fmtNode && fmtNode->type == AST_STRING && fmtNode->data.string.value) {
                printfSpecs = parsePrintfFormatSpecs(fmtNode->data.string.value);
            }
        }

        if (isBuiltinUnionCtorName(calleeName) || isRegisteredUnionCtorName(calleeName)) {
            int argCount = node->data.call.args ?
                node->data.call.args->data.expression_list.expression_count : 0;

            if (calleeName == "None") {
                if (argCount != 0) {
                    llvm::errs() << "Error: None() does not accept arguments\n";
                    return VisitResult();
                }
                Type* i32Ty = Type::getInt32Ty(context);
                Type* i64Ty = Type::getInt64Ty(context);
                StructType* adtStructTy = StructType::get(context, {i32Ty, i64Ty});

                Function* reallocFn = getOrCreateReallocFunction();
                uint64_t structSize = 16;
                Value* heapBytes = ConstantInt::get(Type::getInt64Ty(context), structSize);
                Value* heapI8 = builder.CreateCall(reallocFn, {ConstantPointerNull::get(PointerType::get(context, 0)), heapBytes}, "adt_heap");
                Value* adtPtr = builder.CreateBitCast(heapI8, PointerType::get(context, 0), "adt_ptr");

                Value* tagPtr = builder.CreateStructGEP(adtStructTy, adtPtr, 0, "tag_ptr");
                builder.CreateStore(ConstantInt::get(i32Ty, 1), tagPtr);

                Value* payloadPtr = builder.CreateStructGEP(adtStructTy, adtPtr, 1, "payload_ptr");
                builder.CreateStore(ConstantInt::get(i64Ty, 0), payloadPtr);

                pointerElementHints[adtPtr] = adtStructTy;
                return VisitResult(adtPtr, ValueType::POINTER, adtStructTy);
            }

            if (isRegisteredUnionCtorName(calleeName) && !isBuiltinUnionCtorName(calleeName)) {
                int payload_count = vix_adt_ctor_payload_count(calleeName.c_str());
                if (payload_count > 0 && argCount == 1 && node->data.call.args &&
                    node->data.call.args->type == AST_EXPRESSION_LIST) {
                    VisitResult argRes = visit(node->data.call.args->data.expression_list.expressions[0]);
                    if (!argRes.value) return VisitResult();

                    int32_t tagValue = ctorTagValue(calleeName);
                    /* Try to use the const value from the global variable instead of hash */
                    GlobalVariable* ctorGv = module->getGlobalVariable(calleeName, true);
                    if (ctorGv && ctorGv->hasInitializer()) {
                        if (auto* intInit = dyn_cast<ConstantInt>(ctorGv->getInitializer())) {
                            tagValue = static_cast<int32_t>(intInit->getSExtValue());
                        }
                    } else {
                        int ctor_idx = vix_adt_ctor_index(calleeName.c_str());
                        if (ctor_idx >= 0) tagValue = static_cast<int32_t>(ctor_idx);
                    }
                    Type* i32Ty = Type::getInt32Ty(context);
                    Type* i8PtrTy = PointerType::get(context, 0);
                    StructType* adtStructTy = StructType::get(context, {i32Ty, i8PtrTy});

                    Function* reallocFn = getOrCreateReallocFunction();
                    uint64_t structSize = 16;
                    Value* heapBytes = ConstantInt::get(Type::getInt64Ty(context), structSize);
                    Value* heapI8 = builder.CreateCall(reallocFn, {ConstantPointerNull::get(PointerType::get(context, 0)), heapBytes}, "adt_heap");
                    Value* adtPtr = builder.CreateBitCast(heapI8, PointerType::get(context, 0), "adt_ptr");

                    Value* tagPtr = builder.CreateStructGEP(adtStructTy, adtPtr, 0, "tag_ptr");
                    builder.CreateStore(ConstantInt::get(i32Ty, tagValue), tagPtr);

                    Value* payloadPtr = builder.CreateStructGEP(adtStructTy, adtPtr, 1, "payload_ptr");
                    Value* payload = argRes.value;
                    if (!payload->getType()->isPointerTy()) {
                        payload = builder.CreateIntToPtr(
                            builder.CreateIntCast(payload, Type::getInt64Ty(context), true, "adt_payload_cast"),
                            i8PtrTy);
                    } else if (payload->getType() != i8PtrTy) {
                        payload = builder.CreateBitCast(payload, i8PtrTy, "adt_payload_bcast");
                    }
                    builder.CreateStore(payload, payloadPtr);

                    pointerElementHints[adtPtr] = adtStructTy;
                    return VisitResult(adtPtr, ValueType::POINTER, adtStructTy);
                }
                if (argCount != 0) {
                    llvm::errs() << "Error: " << calleeName << "() does not accept arguments\n";
                    return VisitResult();
                }
                int32_t noArgTag = ctorTagValue(calleeName);
                GlobalVariable* noArgGv = module->getGlobalVariable(calleeName, true);
                if (noArgGv && noArgGv->hasInitializer()) {
                    if (auto* intInit = dyn_cast<ConstantInt>(noArgGv->getInitializer())) {
                        noArgTag = static_cast<int32_t>(intInit->getSExtValue());
                    }
                } else {
                    int ctor_idx = vix_adt_ctor_index(calleeName.c_str());
                    if (ctor_idx >= 0) noArgTag = static_cast<int32_t>(ctor_idx);
                }
                /* Create tagged struct for no-payload constructor */
                {
                    Type* i32Ty = Type::getInt32Ty(context);
                    Type* i8PtrTy = PointerType::get(context, 0);
                    StructType* adtStructTy = StructType::get(context, {i32Ty, i8PtrTy});
                    Function* reallocFn = getOrCreateReallocFunction();
                    Value* heapBytes = ConstantInt::get(Type::getInt64Ty(context), 16);
                    Value* heapI8 = builder.CreateCall(reallocFn, {ConstantPointerNull::get(PointerType::get(context, 0)), heapBytes}, "adt_heap");
                    Value* adtPtr = builder.CreateBitCast(heapI8, PointerType::get(context, 0), "adt_ptr");
                    Value* tagPtr = builder.CreateStructGEP(adtStructTy, adtPtr, 0, "tag_ptr");
                    builder.CreateStore(ConstantInt::get(i32Ty, noArgTag), tagPtr);
                    Value* payloadPtr = builder.CreateStructGEP(adtStructTy, adtPtr, 1, "payload_ptr");
                    builder.CreateStore(ConstantPointerNull::get(cast<PointerType>(i8PtrTy)), payloadPtr);
                    pointerElementHints[adtPtr] = adtStructTy;
                    return VisitResult(adtPtr, ValueType::POINTER, adtStructTy);
                }
            }

            if (argCount != 1 || !node->data.call.args || node->data.call.args->type != AST_EXPRESSION_LIST) {
                llvm::errs() << "Error: " << calleeName << "(...) expects one argument\n";
                return VisitResult();
            }

            VisitResult argRes = visit(node->data.call.args->data.expression_list.expressions[0]);
            if (!argRes.value) return VisitResult();

            if (calleeName == "Ok" || calleeName == "Err" || calleeName == "Some") {
                int32_t tagValue = (calleeName == "Err") ? 1 : 0;
                    Type* i32Ty = Type::getInt32Ty(context);
                    Type* i8PtrTy = PointerType::get(context, 0);
                    StructType* adtStructTy = StructType::get(context, {i32Ty, i8PtrTy});

                    Function* reallocFn = getOrCreateReallocFunction();
                    uint64_t structSize = 16;
                    Value* heapBytes = ConstantInt::get(Type::getInt64Ty(context), structSize);
                    Value* heapI8 = builder.CreateCall(reallocFn, {ConstantPointerNull::get(PointerType::get(context, 0)), heapBytes}, "adt_heap");
                    Value* adtPtr = builder.CreateBitCast(heapI8, PointerType::get(context, 0), "adt_ptr");

                    Value* tagPtr = builder.CreateStructGEP(adtStructTy, adtPtr, 0, "tag_ptr");
                    builder.CreateStore(ConstantInt::get(i32Ty, tagValue), tagPtr);

                    Value* payloadPtr = builder.CreateStructGEP(adtStructTy, adtPtr, 1, "payload_ptr");
                    Value* payload = argRes.value;
                    if (!payload->getType()->isPointerTy()) {
                        payload = builder.CreateIntToPtr(
                            builder.CreateIntCast(payload, Type::getInt64Ty(context), true, "adt_payload_cast"),
                            i8PtrTy);
                    } else if (payload->getType() != i8PtrTy) {
                        payload = builder.CreateBitCast(payload, i8PtrTy, "adt_payload_bcast");
                    }
                    builder.CreateStore(payload, payloadPtr);

                pointerElementHints[adtPtr] = adtStructTy;
                return VisitResult(adtPtr, ValueType::POINTER, adtStructTy);
            }

            return argRes;
        }

        if (node->data.call.type_args && node->data.call.type_args->type == AST_EXPRESSION_LIST) {
            int actualTypeArgCount = node->data.call.type_args->data.expression_list.expression_count;
            auto arityIt = genericFunctionArity.find(calleeName);
            if (arityIt != genericFunctionArity.end() && arityIt->second != actualTypeArgCount) {
                llvm::errs() << "Error: Generic function '" << calleeName << "' expects "
                             << arityIt->second << " type arguments but got " << actualTypeArgCount << "\n";
                return VisitResult();
            }

            Function* instFn = instantiateGenericFunction(calleeName, node->data.call.type_args);
            if (!instFn) {
                llvm::errs() << "Error: Failed to instantiate generic function '" << calleeName << "'\n";
                return VisitResult();
            }
            calleeName = instFn->getName().str();
        }

        Function* callee = module->getFunction(calleeName);
        if (!callee) {
            auto fit = genericFunctionTemplates.find(calleeName);
            if (fit != genericFunctionTemplates.end()) {
                Function* instFn = instantiateGenericFunction(calleeName, nullptr);
                if (instFn) {
                    callee = instFn;
                    calleeName = instFn->getName().str();
                }
            }
        }
        if (!callee) {
            AllocaInst* fnPtrAlloc = scopeManager.findVariable(calleeName);
            if (!fnPtrAlloc) fnPtrAlloc = findVariableInMain(calleeName);
            if (fnPtrAlloc) {
                Type* allocType = getActualType(fnPtrAlloc);
                if (allocType && allocType->isPointerTy()) {
                    Value* fnPtr = builder.CreateLoad(allocType, fnPtrAlloc, calleeName + "_fnptr");
                    if (fnPtr && fnPtr->getType()->isPointerTy()) {
                        Type* hintRetTy = nullptr;
                        if (Function* curFn = getCurrentFunction()) {
                            hintRetTy = curFn->getReturnType();
                        }
                        return emitFunctionPointerCall(fnPtr, node->data.call.args, hintRetTy);
                    }
                }
            }
            auto afit = allFunctionNodes.find(calleeName);
            if (afit != allFunctionNodes.end()) {
                IRBuilder<>::InsertPoint savedIP = builder.saveIP();
                visitFunction(afit->second);
                if (savedIP.isSet()) builder.restoreIP(savedIP);
                callee = module->getFunction(calleeName);
            }
            if (!callee) {
                llvm::errs() << "Error: Call to undefined function '" << calleeName << "'\n";
                return VisitResult();
            }
        }
        
        int expectedParamCount = callee->getFunctionType()->getNumParams();
        int actualParamCount = node->data.call.args ? 
            node->data.call.args->data.expression_list.expression_count : 0;
        int hiddenArrayLenParamCount = 0;
        int hiddenSRetParamCount = usesStructSRet(callee) ? 1 : 0;
        auto arrayMetaIt = functionArrayParamPositions.find(calleeName);
        if (arrayMetaIt != functionArrayParamPositions.end()) {
            hiddenArrayLenParamCount = (int)arrayMetaIt->second.size();
        }
        int expectedUserParamCount = expectedParamCount - hiddenArrayLenParamCount - hiddenSRetParamCount;
        bool isVarArg = callee->getFunctionType()->isVarArg();
        bool isKnownVarArgFunc = (calleeName == "printf" || calleeName == "sprintf" ||
                                  calleeName == "snprintf" || calleeName == "scanf");
        
        if (!isVarArg && !isKnownVarArgFunc && actualParamCount != expectedUserParamCount) {
            llvm::errs() << "Error: Function '" << calleeName 
                        << "' expects " << expectedUserParamCount 
                        << " arguments but got " << actualParamCount << "\n";
            return VisitResult();
        }
        
        std::vector<Value*> args;
        int llvmParamIndex = 0;
        StructType* sretType = getStructSRetType(callee);
        AllocaInst* sretAlloc = nullptr;
        if (sretType) {
            Function* caller = getCurrentFunction();
            if (!caller) {
                caller = module->getFunction("main");
                if (!caller) {
                    createDefaultMain();
                    caller = module->getFunction("main");
                }
            }
            if (!caller) return VisitResult();

            BasicBlock* entryBB = &caller->getEntryBlock();
            BasicBlock* savedBB = builder.GetInsertBlock();
            IRBuilder<> tmpBuilder(entryBB, entryBB->begin());
            sretAlloc = tmpBuilder.CreateAlloca(sretType, nullptr, calleeName + "_ret");
            if (savedBB) {
                builder.SetInsertPoint(savedBB);
            }

            args.push_back(sretAlloc);
            llvmParamIndex = 1;
        }

        if (node->data.call.args) {
            for (int i = 0; i < actualParamCount; i++) {
                ASTNode* argNode = node->data.call.args->data.expression_list.expressions[i];
                VisitResult argRes;
                if (argNode && argNode->type == AST_FUNCTION) {
                    IRBuilder<>::InsertPoint savedIP = builder.saveIP();
                    argRes = visit(argNode);
                    if (savedIP.isSet()) {
                        builder.restoreIP(savedIP);
                    }
                } else {
                    argRes = visit(argNode);
                }
                if (!argRes.value) {
                    llvm::errs() << "Error: Failed to evaluate argument " << i 
                                << " for function '" << calleeName << "'\n";
                    return VisitResult();
                }
                // Fix: When passing `ref` of a pointer-type variable to a function,
                // load the pointer value from the alloca instead of passing the alloca.
                // This handles `quicksort(ref arr, 0, 5)` where `arr: &[i32]` —
                // without this fix, the alloca address (not the data pointer) is passed,
                // causing array indexing to read from stack memory and hang.
                if (argNode && argNode->type == AST_UNARYOP &&
                    argNode->data.unaryop.op == OP_ADDRESS &&
                    argRes.value && argRes.value->getType()->isPointerTy()) {
                    ASTNode* innerExpr = argNode->data.unaryop.expr;
                    if (innerExpr && innerExpr->type == AST_IDENTIFIER && innerExpr->data.identifier.name) {
                        std::string refVarName(innerExpr->data.identifier.name);
                        AllocaInst* refVarAlloc = scopeManager.findVariable(refVarName);
                        if (!refVarAlloc) refVarAlloc = findVariableInMain(refVarName);
                        if (refVarAlloc) {
                            Type* refAllocType = getActualType(refVarAlloc);
                            if (refAllocType && refAllocType->isPointerTy()) {
                                argRes.value = builder.CreateLoad(refAllocType, refVarAlloc, refVarName + "_ref_ptr");
                                argRes.type = typeHelper.getValueTypeFromType(refAllocType);
                            }
                        }
                    }
                }
                if (llvmParamIndex < expectedParamCount || isVarArg || isKnownVarArgFunc) {
                    Type* expectedType = nullptr;
                    if (llvmParamIndex < expectedParamCount) {
                        expectedType = callee->getFunctionType()->getParamType(llvmParamIndex);
                    } else {
                        if (isKnownVarArgFunc) {
                            if ((calleeName == "printf" || calleeName == "sprintf" || calleeName == "snprintf") && i > 0) {
                                int fmtArgIndex = i - 1;
                                if (fmtArgIndex >= 0 && fmtArgIndex < (int)printfSpecs.size()) {
                                    const PrintfFormatSpec& spec = printfSpecs[fmtArgIndex];
                                    switch (spec.conv) {
                                        case 's':
                                        case 'p':
                                            expectedType = PointerType::get(context, 0);
                                            break;
                                        case 'f':
                                        case 'F':
                                        case 'e':
                                        case 'E':
                                        case 'g':
                                        case 'G':
                                        case 'a':
                                        case 'A':
                                            expectedType = Type::getDoubleTy(context);
                                            break;
                                        case 'd':
                                        case 'i':
                                        case 'u':
                                        case 'x':
                                        case 'X':
                                        case 'o':
                                        case 'c':
                                            expectedType = (spec.length >= 2)
                                                ? Type::getInt64Ty(context)
                                                : Type::getInt32Ty(context);
                                            break;
                                        default:
                                            break;
                                    }
                                }
                            }

                            if (argRes.value->getType()->isIntegerTy() && 
                                argRes.value->getType()->getIntegerBitWidth() < 32) {
                                if (!expectedType || expectedType->isIntegerTy()) {
                                    expectedType = Type::getInt32Ty(context);//提升到i32
                                }
                            }
                            if (!expectedType) {
                                expectedType = argRes.value->getType();
                            }
                        } else {
                            expectedType = argRes.value->getType();
                        }
                    }

                    if ((calleeName == "fgets" || calleeName == "gets") && i == 0 &&
                        argNode && argNode->type == AST_IDENTIFIER && argNode->data.identifier.name) {
                        std::string strBufName(argNode->data.identifier.name);
                        typeHelper.registerStringVariable(strBufName);
                        typeHelper.registerArrayType(strBufName, Type::getInt8Ty(context), -1);
                    }

                    if (calleeName == "strncpy" && i == 0 &&
                        argNode && argNode->type == AST_IDENTIFIER && argNode->data.identifier.name) {
                        std::string strBufName(argNode->data.identifier.name);
                        typeHelper.registerStringVariable(strBufName);
                        typeHelper.registerArrayType(strBufName, Type::getInt8Ty(context), -1);
                    }

                    if (expectedType && expectedType->isPointerTy() &&
                        (!argRes.value->getType()->isPointerTy()) &&
                        argNode && argNode->type == AST_INDEX &&
                        argNode->data.index.target && argNode->data.index.target->type == AST_IDENTIFIER &&
                        argNode->data.index.target->data.identifier.name) {
                        std::string indexedBaseName(argNode->data.index.target->data.identifier.name);
                        AllocaInst* indexedBaseAlloc = scopeManager.findVariable(indexedBaseName);
                        if (!indexedBaseAlloc) indexedBaseAlloc = findVariableInMain(indexedBaseName);
                        GlobalVariable* indexedBaseGlobal = nullptr;
                        if (!indexedBaseAlloc) {
                            indexedBaseGlobal = findGlobalVariable(indexedBaseName);
                        }

                        Value* indexedBasePtr = nullptr;
                        if (indexedBaseAlloc) {
                            Type* indexedAllocTy = getActualType(indexedBaseAlloc);
                            if (indexedAllocTy && indexedAllocTy->isPointerTy()) {
                                indexedBasePtr = builder.CreateLoad(indexedAllocTy, indexedBaseAlloc, indexedBaseName + "_ptr");
                            }
                        } else if (indexedBaseGlobal) {
                            Type* globalTy = indexedBaseGlobal->getValueType();
                            if (globalTy && globalTy->isPointerTy()) {
                                indexedBasePtr = builder.CreateLoad(globalTy, indexedBaseGlobal, indexedBaseName + "_ptr");
                            }
                        }

                        if (indexedBasePtr && indexedBasePtr->getType()->isPointerTy()) {
                            VisitResult reIdxRes = visit(argNode->data.index.index);
                            if (reIdxRes.value) {
                                Value* reIdxVal = reIdxRes.value;
                                if (!reIdxVal->getType()->isIntegerTy(32)) {
                                    reIdxVal = builder.CreateIntCast(reIdxVal, Type::getInt32Ty(context), true, "idxcast_printf");
                                }

                                Type* forcedElemType = PointerType::get(context, 0);
                                Value* forcedGep = builder.CreateInBoundsGEP(forcedElemType, indexedBasePtr, reIdxVal, "fmt_ptr_index_ptr");
                                Value* forcedLoaded = builder.CreateLoad(forcedElemType, forcedGep, "fmt_ptr_index_load");

                                argRes.value = forcedLoaded;
                                argRes.type = ValueType::POINTER;
                                typeHelper.registerArrayType(indexedBaseName, forcedElemType, -1);
                            }
                        }
                    }
                    
                    ValueType expectedValueType = typeHelper.getValueTypeFromType(expectedType);

                    Value* arg = nullptr;
                    if (expectedType && expectedType->isPointerTy() &&
                        argRes.value->getType()->isStructTy()) {
                        StructType* argStructTy = cast<StructType>(argRes.value->getType());
                        Value* sourcePtr = nullptr;
                        if (auto* loadInst = dyn_cast<LoadInst>(argRes.value)) {
                            sourcePtr = loadInst->getPointerOperand();
                        }
                        if (sourcePtr && sourcePtr->getType()->isPointerTy()) {
                            arg = sourcePtr;
                        } else {
                            AllocaInst* tmpStructArg = builder.CreateAlloca(argStructTy, nullptr, "struct_arg_tmp");
                            builder.CreateStore(argRes.value, tmpStructArg);
                            arg = tmpStructArg;
                        }
                        pointerElementHints[arg] = argStructTy;
                    } else {
                        arg = typeHelper.castValue(builder, argRes.value, argRes.type, expectedValueType);
                    }
                    if (expectedType && expectedType->isPointerTy() && arg->getType()->isPointerTy() &&
                        arg->getType() != expectedType) {
                        arg = builder.CreateBitCast(arg, expectedType, "arg_ptrcast");
                    }//对于已知的可变参数函数，like printf 就允许传入与参数类型不完全匹配但可转换的值 like: i8* 传递给 %s
                    args.push_back(arg);
                    llvmParamIndex++;

                    if (!isVarArg && !isKnownVarArgFunc && isArrayParamPosition(calleeName, i)) {
                        Value* lenVal = inferArrayLengthFromArgument(argNode);
                        Type* lenExpectedType = Type::getInt32Ty(context);
                        if (llvmParamIndex < expectedParamCount) {
                            lenExpectedType = callee->getFunctionType()->getParamType(llvmParamIndex);
                        }

                        if (!lenVal->getType()->isIntegerTy()) {
                            lenVal = typeHelper.castValue(builder, lenVal, ValueType::INT32, ValueType::INT32);
                        }
                        if (lenVal->getType() != lenExpectedType && lenExpectedType->isIntegerTy()) {
                            lenVal = builder.CreateIntCast(lenVal, lenExpectedType, false, "arr_len_arg_cast");
                        }
                        args.push_back(lenVal);
                        llvmParamIndex++;
                    }
                }
            }
        }
        
        CallInst* callInst = builder.CreateCall(callee, args);
        if (sretAlloc) {
            return VisitResult(sretAlloc, ValueType::POINTER, sretType);
        }
        if (!callee->getReturnType()->isVoidTy()) callInst->setName("calltmp");
        if (callee->getReturnType()->isPointerTy()) {
            auto retElemIt = functionArrayReturnElementTypes.find(calleeName);
            if (retElemIt != functionArrayReturnElementTypes.end() && retElemIt->second) {
                pointerElementHints[callInst] = retElemIt->second;
            }
        }
        ValueType returnType = typeHelper.getValueTypeFromType(callee->getReturnType());
        return VisitResult(callInst, returnType);
    }
