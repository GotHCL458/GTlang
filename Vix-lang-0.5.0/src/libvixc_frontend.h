#ifndef VIXC_FRONTEND_H
#define VIXC_FRONTEND_H

#include "ast.h"
#include "compiler.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    ASTNode *root;
    int error_count;
} CompileResult;

CompileResult vixc_compile_string(const char *source);
void vixc_free_result(CompileResult *result);
const char *vixc_get_last_error(void);

#ifdef __cplusplus
}
#endif

#endif
