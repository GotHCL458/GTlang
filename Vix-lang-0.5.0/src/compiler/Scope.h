#ifndef SCOPE_MANAGER_H
#define SCOPE_MANAGER_H

#include <llvm/IR/Instructions.h>
#include <llvm/IR/Function.h>
#include <map>
#include <string>
#include <vector>

class ScopeManager {
private:
    std::vector<std::map<std::string, llvm::AllocaInst*>> scopes;
    llvm::Function* currentFunction;

public:
    ScopeManager() : currentFunction(nullptr) { enterScope(); }

    void enterScope() { scopes.push_back({}); }

    void exitScope() {
        if (scopes.size() > 1) scopes.pop_back();
    }

    void setCurrentFunction(llvm::Function* func) { currentFunction = func; }

    llvm::Function* getCurrentFunction() { return currentFunction; }

    void defineVariable(const std::string& name, llvm::AllocaInst* alloc) {
        if (!scopes.empty()) scopes.back()[name] = alloc;
    }

    llvm::AllocaInst* findVariable(const std::string& name) {
        for (auto it = scopes.rbegin(); it != scopes.rend(); ++it) {
            auto found = it->find(name);
            if (found != it->end()) {
                llvm::AllocaInst* alloc = found->second;
                if (currentFunction && alloc->getFunction() != currentFunction) {
                    continue;
                }
                return alloc;
            }
        }
        return nullptr;
    }

    llvm::AllocaInst* findVariableInCurrentScope(const std::string& name) {
        if (scopes.empty()) return nullptr;
        auto found = scopes.back().find(name);
        if (found == scopes.back().end()) return nullptr;
        llvm::AllocaInst* alloc = found->second;
        if (currentFunction && alloc->getFunction() != currentFunction) {
            return nullptr;
        }
        return alloc;
    }

    void clear() { scopes.clear(); enterScope(); }
};

#endif
