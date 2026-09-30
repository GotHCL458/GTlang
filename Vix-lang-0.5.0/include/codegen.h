#ifndef LLVM_EMIT_H
#define LLVM_EMIT_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdio.h>
typedef struct ASTNode ASTNode;
void llvm_emit_from_ast(ASTNode* ast_root, FILE* llvm_fp);
void llvm_set_target_triple(const char* triple);
void vix_set_opt_level(int level);
int vix_get_opt_level(void);
void vix_optimize_module(void* llvm_module, int level);

#ifdef __cplusplus
}//c api
#endif
#endif