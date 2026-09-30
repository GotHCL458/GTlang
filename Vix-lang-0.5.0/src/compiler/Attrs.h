#ifndef SOURCE_ATTRS_H
#define SOURCE_ATTRS_H

#include <string>
#include <map>
#include <vector>

struct PrintfFormatSpec {
    char conv = 0;
    int length = 0;
};

struct SymbolAttr {
    bool exported = false;
    std::string section;
};

struct SourceAttrInfo {
    bool noMain = false;
    bool noStd = false;
    std::map<std::string, SymbolAttr> functionAttrs;
    std::map<std::string, SymbolAttr> constAttrs;
};

std::vector<PrintfFormatSpec> parsePrintfFormatSpecs(const std::string& fmt);
SourceAttrInfo parseSourceAttributes(const char* filePath);

#endif
