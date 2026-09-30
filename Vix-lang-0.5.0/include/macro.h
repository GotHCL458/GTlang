#ifndef VIX_MACRO_H
#define VIX_MACRO_H

#include <stdio.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Expand Vix declarative macros in source text.  The returned string and
 * stream are owned by the caller.  On failure these functions print a
 * diagnostic and return NULL.
 */
char *vix_expand_macros(const char *source, const char *filename);
FILE *vix_preprocess_macros(FILE *input, const char *filename);

#ifdef __cplusplus
}
#endif

#endif
