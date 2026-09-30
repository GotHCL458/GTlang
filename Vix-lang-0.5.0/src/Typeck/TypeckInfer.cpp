#include "../../include/typeck.h"
#include "../../include/typeckinternal.h"

extern "C" int typecheck_program(ASTNode* root) {
	return vix_typecheck_program(root);
}
