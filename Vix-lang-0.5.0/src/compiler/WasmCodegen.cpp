#include "WasmCodegen.h"
#include "WasmTypeMap.h"
#include "binaryen-c.h"
#include <cstring>

WasmCodegen::WasmCodegen() : m_module(nullptr), m_current_func(nullptr), m_has_error(false) {}

WasmCodegen::~WasmCodegen() {
    if (m_module) BinaryenModuleDispose(m_module);
}

void WasmCodegen::set_error(const std::string &msg) {
    if (!m_has_error) {
        m_error = msg;
        m_has_error = true;
    }
}

void WasmCodegen::add_imports() {
    BinaryenType one_i32[] = {BinaryenTypeInt32()};
    BinaryenType void_t = BinaryenTypeNone();

    BinaryenAddFunctionImport(m_module, "vix_putchar", "env", "vix_putchar",
                              BinaryenTypeCreate(one_i32, 1), void_t);
    BinaryenAddFunctionImport(m_module, "vix_puts", "env", "vix_puts",
                              BinaryenTypeCreate(one_i32, 1), void_t);
    BinaryenAddFunctionImport(m_module, "vix_exit", "env", "vix_exit",
                              BinaryenTypeCreate(one_i32, 1), void_t);
}

void WasmCodegen::register_function(ASTNode *node) {
    if (!node || node->type != AST_FUNCTION || !node->data.function.name) return;

    std::string name(node->data.function.name);
    std::vector<uintptr_t> param_types;
    ASTNode *params = node->data.function.params;
    if (params && params->type == AST_EXPRESSION_LIST) {
        for (int i = 0; i < params->data.expression_list.expression_count; i++) {
            ASTNode *p = params->data.expression_list.expressions[i];
            if (p && p->type == AST_ASSIGN && p->data.assign.left) {
                // param has a declared type, use i32 default
                param_types.push_back(BinaryenTypeInt32());
            } else {
                param_types.push_back(BinaryenTypeInt32());
            }
        }
    }

    uintptr_t ret_type = BinaryenTypeInt32();
    if (node->data.function.return_type) {
        ASTNode *rt = node->data.function.return_type;
        if (rt->type == AST_TYPE_VOID) {
            ret_type = BinaryenTypeNone();
        }
    }

    FuncInfo info;
    info.func_ref = BinaryenAddFunction(
        m_module, name.c_str(),
        BinaryenTypeCreate(param_types.data(), param_types.size()),
        ret_type,
        nullptr, 0, // no var types initially
        BinaryenNop(m_module));
    info.next_local = param_types.size();

    m_functions[name] = info;
}

uint32_t WasmCodegen::get_or_create_local(const char *name, uintptr_t wasm_type) {
    if (!m_current_func) return 0;
    auto it = m_current_func->local_indices.find(name);
    if (it != m_current_func->local_indices.end()) {
        return it->second;
    }
    uint32_t idx = m_current_func->next_local++;
    m_current_func->local_indices[name] = idx;
    return idx;
}

void WasmCodegen::compile_function_body(ASTNode *node) {
    if (!node || node->type != AST_FUNCTION || !node->data.function.name) return;

    std::string name(node->data.function.name);
    auto fit = m_functions.find(name);
    if (fit == m_functions.end()) return;

    m_current_func = &fit->second;

    ASTNode *body = node->data.function.body;
    (void)body;
    // TODO: compile function body into Binaryen expression
    // Binaryen C API requires recreating the function to set body

    m_current_func = nullptr;
}

uintptr_t WasmCodegen::compile_node(ASTNode *node) {
    if (!node || m_has_error) return (uintptr_t)BinaryenNop(m_module);

    switch (node->type) {
        case AST_PROGRAM:
            return compile_block(node);
        case AST_IF:
            return compile_if(node);
        case AST_WHILE:
            return compile_while(node);
        case AST_BINOP:
            return compile_binary_op(node);
        case AST_CALL:
            return compile_call(node);
        case AST_IDENTIFIER:
            return compile_ident(node);
        case AST_NUM_INT:
            return (uintptr_t)BinaryenConst(m_module, BinaryenLiteralInt32((int32_t)node->data.num_int.value));
        case AST_NUM_FLOAT:
            return (uintptr_t)BinaryenConst(m_module, BinaryenLiteralFloat64(node->data.num_float.value));
        case AST_STRING:
            // Strings are stored in memory; return pointer for now
            return (uintptr_t)BinaryenConst(m_module, BinaryenLiteralInt32(0));
        case AST_RETURN:
            return compile_return(node);
        case AST_ASSIGN:
            return compile_assign(node);
        default:
            return (uintptr_t)BinaryenNop(m_module);
    }
}

uintptr_t WasmCodegen::compile_block(ASTNode *node) {
    if (!node) return (uintptr_t)BinaryenNop(m_module);

    std::vector<uintptr_t> exprs;

    if (node->type == AST_PROGRAM) {
        for (int i = 0; i < node->data.program.statement_count; i++) {
            ASTNode *stmt = node->data.program.statements[i];
            if (stmt) {
                uintptr_t expr = compile_node(stmt);
                if (expr) exprs.push_back(expr);
            }
        }
    } else {
        // Not a block-like node, compile directly
        uintptr_t expr = compile_node(node);
        if (expr) exprs.push_back(expr);
    }

    if (exprs.empty()) return (uintptr_t)BinaryenNop(m_module);
    if (exprs.size() == 1) return exprs[0];

    return (uintptr_t)BinaryenBlock(m_module, nullptr,
                                    (BinaryenExpressionRef*)exprs.data(),
                                    exprs.size(), BinaryenTypeAuto());
}

uintptr_t WasmCodegen::compile_if(ASTNode *if_node) {
    uintptr_t cond = compile_node(if_node->data.if_stmt.condition);
    uintptr_t then_expr = compile_node(if_node->data.if_stmt.then_body);
    uintptr_t else_expr = if_node->data.if_stmt.else_body
                          ? compile_node(if_node->data.if_stmt.else_body)
                          : (uintptr_t)BinaryenNop(m_module);
    return (uintptr_t)BinaryenIf(m_module, (BinaryenExpressionRef)cond,
                                 (BinaryenExpressionRef)then_expr,
                                 (BinaryenExpressionRef)else_expr);
}

uintptr_t WasmCodegen::compile_while(ASTNode *while_node) {
    (void)while_node;
    // TODO: implement while loop
    return (uintptr_t)BinaryenNop(m_module);
}

uintptr_t WasmCodegen::compile_binary_op(ASTNode *op_node) {
    uintptr_t left = compile_node(op_node->data.binop.left);
    uintptr_t right = compile_node(op_node->data.binop.right);

    BinaryenOp op;
    switch (op_node->data.binop.op) {
        case OP_ADD: op = BinaryenAddInt32(); break;
        case OP_SUB: op = BinaryenSubInt32(); break;
        case OP_MUL: op = BinaryenMulInt32(); break;
        case OP_DIV: op = BinaryenDivSInt32(); break;
        case OP_MOD: op = BinaryenRemSInt32(); break;
        case OP_EQ:  op = BinaryenEqInt32(); break;
        case OP_NE:  op = BinaryenNeInt32(); break;
        case OP_LT:  op = BinaryenLtSInt32(); break;
        case OP_LE:  op = BinaryenLeSInt32(); break;
        case OP_GT:  op = BinaryenGtSInt32(); break;
        case OP_GE:  op = BinaryenGeSInt32(); break;
        case OP_AND: op = BinaryenAndInt32(); break;
        case OP_OR:  op = BinaryenOrInt32(); break;
        default:
            set_error("unsupported binary operator");
            return (uintptr_t)BinaryenUnreachable(m_module);
    }
    return (uintptr_t)BinaryenBinary(m_module, op,
                                     (BinaryenExpressionRef)left,
                                     (BinaryenExpressionRef)right);
}

uintptr_t WasmCodegen::compile_call(ASTNode *call_node) {
    ASTNode *func = call_node->data.call.func;
    ASTNode *args = call_node->data.call.args;

    if (!func || func->type != AST_IDENTIFIER || !func->data.identifier.name) {
        set_error("call target is not an identifier");
        return (uintptr_t)BinaryenUnreachable(m_module);
    }

    std::string name(func->data.identifier.name);

    // Check if it's an import function
    bool is_import = (name == "putchar" || name == "puts" || name == "print" || name == "exit");

    std::vector<uintptr_t> arg_exprs;
    if (args && args->type == AST_EXPRESSION_LIST) {
        for (int i = 0; i < args->data.expression_list.expression_count; i++) {
            uintptr_t a = compile_node(args->data.expression_list.expressions[i]);
            arg_exprs.push_back(a);
        }
    }

    uintptr_t ret_type = BinaryenTypeInt32();
    if (is_import) {
        ret_type = BinaryenTypeNone();
    }

    return (uintptr_t)BinaryenCall(m_module, name.c_str(),
                                   (BinaryenExpressionRef*)arg_exprs.data(),
                                   arg_exprs.size(), ret_type);
}

uintptr_t WasmCodegen::compile_ident(ASTNode *ident_node) {
    if (!ident_node->data.identifier.name) {
        return (uintptr_t)BinaryenNop(m_module);
    }
    uint32_t idx = get_or_create_local(ident_node->data.identifier.name, BinaryenTypeInt32());
    return (uintptr_t)BinaryenLocalGet(m_module, idx, BinaryenTypeInt32());
}

uintptr_t WasmCodegen::compile_return(ASTNode *ret_node) {
    if (!ret_node->data.return_stmt.expr) {
        return (uintptr_t)BinaryenReturn(m_module, nullptr);
    }
    uintptr_t val = compile_node(ret_node->data.return_stmt.expr);
    return (uintptr_t)BinaryenReturn(m_module, (BinaryenExpressionRef)val);
}

uintptr_t WasmCodegen::compile_assign(ASTNode *assign_node) {
    // Assignment: left = right
    uintptr_t val = compile_node(assign_node->data.assign.right);
    ASTNode *target = assign_node->data.assign.left;

    if (target && target->type == AST_IDENTIFIER && target->data.identifier.name) {
        uint32_t idx = get_or_create_local(target->data.identifier.name, BinaryenTypeInt32());
        return (uintptr_t)BinaryenLocalSet(m_module, idx, (BinaryenExpressionRef)val);
    }

    return (uintptr_t)BinaryenNop(m_module);
}

uintptr_t WasmCodegen::compile_var_decl(ASTNode *decl_node, ASTNode *init_expr) {
    if (decl_node->type != AST_ASSIGN) return (uintptr_t)BinaryenNop(m_module);
    ASTNode *target = decl_node->data.assign.left;
    if (target && target->type == AST_IDENTIFIER && target->data.identifier.name) {
        uintptr_t val;
        if (init_expr) {
            val = compile_node(init_expr);
        } else {
            val = (uintptr_t)BinaryenConst(m_module, BinaryenLiteralInt32(0));
        }
        // local already created in register_function, just set it
        return (uintptr_t)BinaryenLocalSet(m_module,
            get_or_create_local(target->data.identifier.name, BinaryenTypeInt32()),
            (BinaryenExpressionRef)val);
    }
    return (uintptr_t)BinaryenNop(m_module);
}

bool WasmCodegen::emit(ASTNode *root, std::vector<uint8_t> &out_bytes, std::string &error_msg) {
    if (!root) {
        error_msg = "null AST root";
        return false;
    }

    m_module = BinaryenModuleCreate();
    m_has_error = false;
    m_error.clear();

    add_imports();

    BinaryenSetMemory(m_module, 1, -1, "memory", nullptr, nullptr, nullptr, nullptr, nullptr, 0, false, false, nullptr);

    // First pass: register all functions
    if (root->type == AST_PROGRAM) {
        for (int i = 0; i < root->data.program.statement_count; i++) {
            ASTNode *stmt = root->data.program.statements[i];
            if (stmt && stmt->type == AST_FUNCTION) {
                register_function(stmt);
            }
        }
    }

    // Second pass: compile function bodies
    if (root->type == AST_PROGRAM) {
        for (int i = 0; i < root->data.program.statement_count; i++) {
            ASTNode *stmt = root->data.program.statements[i];
            if (stmt && stmt->type == AST_FUNCTION) {
                compile_function_body(stmt);
            }
        }
    }

    // Export main
    if (m_functions.count("main")) {
        BinaryenAddFunctionExport(m_module, "main", "main");
    }

    if (m_has_error) {
        BinaryenModuleDispose(m_module);
        m_module = nullptr;
        error_msg = m_error;
        return false;
    }

    BinaryenModuleAllocateAndWriteResult write_result = BinaryenModuleAllocateAndWrite(m_module, nullptr);
    if (write_result.binary) {
        out_bytes.assign((uint8_t*)write_result.binary,
                         (uint8_t*)write_result.binary + write_result.binaryBytes);
        free(write_result.binary);
        return true;
    }

    error_msg = "Failed to write WASM binary";
    return false;
}
