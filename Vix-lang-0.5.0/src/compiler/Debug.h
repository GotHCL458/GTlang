#ifndef DEBUG_UTIL_H
#define DEBUG_UTIL_H

#include <llvm/Support/raw_ostream.h>
#include <cstring>
#include <cstdlib>

static inline bool isVixDebugEnabled() {
    const char* value = std::getenv("VIX_DEBUG");
    if (!value || value[0] == '\0') return false;
    return strcmp(value, "0") != 0 && strcmp(value, "false") != 0 && strcmp(value, "off") != 0;
}

static inline llvm::raw_ostream& vixDebugStream() {
    return isVixDebugEnabled() ? llvm::errs() : llvm::nulls();
}

#define VIX_DEBUG_LOG vixDebugStream()

#endif
