#include "Attrs.h"
#include <fstream>
#include <string>

static std::string trimCopy(const std::string& s) {
    size_t start = 0;
    while (start < s.size() && (s[start] == ' ' || s[start] == '\t' || s[start] == '\r' || s[start] == '\n')) start++;
    size_t end = s.size();
    while (end > start && (s[end - 1] == ' ' || s[end - 1] == '\t' || s[end - 1] == '\r' || s[end - 1] == '\n')) end--;
    return s.substr(start, end - start);
}

std::vector<PrintfFormatSpec> parsePrintfFormatSpecs(const std::string& fmt) {
    std::vector<PrintfFormatSpec> specs;
    size_t i = 0;
    while (i < fmt.size()) {
        if (fmt[i] != '%') { i++; continue; }
        i++;
        if (i < fmt.size() && fmt[i] == '%') { i++; continue; }
        while (i < fmt.size() &&
               (fmt[i] == '-' || fmt[i] == '+' || fmt[i] == ' ' || fmt[i] == '#' || fmt[i] == '0')) { i++; }
        while (i < fmt.size() && (fmt[i] >= '0' && fmt[i] <= '9')) { i++; }
        if (i < fmt.size() && fmt[i] == '.') {
            i++;
            while (i < fmt.size() && (fmt[i] >= '0' && fmt[i] <= '9')) { i++; }
        }
        int length = 0;
        if (i < fmt.size() && fmt[i] == 'l') {
            length = 1;
            i++;
            if (i < fmt.size() && fmt[i] == 'l') { length = 2; i++; }
        }
        if (i < fmt.size()) {
            PrintfFormatSpec spec;
            spec.conv = fmt[i];
            spec.length = length;
            specs.push_back(spec);
            i++;
        }
    }
    return specs;
}

SourceAttrInfo parseSourceAttributes(const char* filePath) {
    SourceAttrInfo info;
    if (!filePath) return info;

    std::ifstream in(filePath);
    if (!in.is_open()) return info;

    SymbolAttr pending;
    bool hasPending = false;
    std::string line;
    while (std::getline(in, line)) {
        std::string t = trimCopy(line);
        if (t.empty()) continue;

        if (t == "#[no_main]") { info.noMain = true; continue; }
        if (t == "#[no_std]") { info.noStd = true; continue; }
        if (t == "#[export]") { pending.exported = true; hasPending = true; continue; }

        if (t.rfind("#[link_section", 0) == 0) {
            size_t q1 = t.find('"');
            size_t q2 = t.rfind('"');
            if (q1 != std::string::npos && q2 != std::string::npos && q2 > q1) {
                pending.section = t.substr(q1 + 1, q2 - q1 - 1);
                hasPending = true;
            }
            continue;
        }

        if (t.rfind("pub fn ", 0) == 0 || t.rfind("fn ", 0) == 0) {
            size_t fnPos = t.rfind("fn ");
            size_t nameStart = fnPos + 3;
            size_t nameEnd = t.find('(', nameStart);
            if (nameEnd != std::string::npos && hasPending) {
                std::string name = trimCopy(t.substr(nameStart, nameEnd - nameStart));
                if (!name.empty()) info.functionAttrs[name] = pending;
            }
            pending = SymbolAttr();
            hasPending = false;
            continue;
        }

        if (t.rfind("const ", 0) == 0) {
            size_t nameStart = 6;
            size_t nameEnd = t.find_first_of(" :={", nameStart);
            if (nameEnd != std::string::npos && hasPending) {
                std::string name = trimCopy(t.substr(nameStart, nameEnd - nameStart));
                if (!name.empty()) info.constAttrs[name] = pending;
            }
            pending = SymbolAttr();
            hasPending = false;
            continue;
        }
    }

    return info;
}

extern "C" int vix_source_has_attr_no_std(const char *filePath) {
    SourceAttrInfo info = parseSourceAttributes(filePath);
    return info.noStd ? 1 : 0;
}

extern "C" int vix_source_has_attr_no_main(const char *filePath) {
    SourceAttrInfo info = parseSourceAttributes(filePath);
    return info.noMain ? 1 : 0;
}

extern "C" int vix_source_get_attrs(const char *filePath, int *out_no_std, int *out_no_main) {
    SourceAttrInfo info = parseSourceAttributes(filePath);
    if (out_no_std)  *out_no_std  = info.noStd  ? 1 : 0;
    if (out_no_main) *out_no_main = info.noMain ? 1 : 0;
    return 0;
}
