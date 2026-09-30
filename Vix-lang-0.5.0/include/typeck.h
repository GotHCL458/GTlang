#ifndef VIX_TYPECK_H
#define VIX_TYPECK_H

#include "ast.h"

#ifdef __cplusplus
extern "C" {
#endif

// Returns 0 on success, non-zero on type checking failure.
int typecheck_program(ASTNode* root);

#ifdef __cplusplus
}
#endif

#endif
