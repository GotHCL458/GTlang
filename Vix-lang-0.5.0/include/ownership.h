#ifndef VIX_OWNERSHIP_H
#define VIX_OWNERSHIP_H

#include "ast.h"

#ifdef __cplusplus
extern "C" {
#endif
int ownership_check_program(ASTNode* root);

#ifdef __cplusplus
}
#endif

#endif /* VIX_OWNERSHIP_H */
