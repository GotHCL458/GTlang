#ifndef VIX_ENV_H
#define VIX_ENV_H

#include "type.h"

#ifdef __cplusplus

#include <cstddef>
#include <optional>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

struct ValInfo {
	std::string name;
	TypePtr type;
	bool is_mutable;
	bool is_global;
};

struct StructFieldInfo {
	std::string name;
	TypePtr type;
	size_t offset;
};

struct StructInfo {
	std::string name;
	std::vector<StructFieldInfo> fields;
	size_t total_size;
	size_t align;
	std::vector<int> generic_param_ids;
};

struct AdtCtorInfo {
	std::string name;
	std::vector<TypePtr> args;
};

struct AdtInfo {
	std::string name;
	std::vector<std::string> type_params;
	std::vector<AdtCtorInfo> ctors;
};

class TypeEnv {
public:
	TypeEnv() {
		scopes_.emplace_back();
	}

	void enter_scope() {
		scopes_.emplace_back();
	}

	void exit_scope() {
		if (scopes_.size() > 1) {
			scopes_.pop_back();
		}
	}

	bool declare_value(const std::string& name, TypePtr type, bool is_mutable, bool is_global) {
		auto& scope = scopes_.back();
		if (scope.find(name) != scope.end()) {
			return false;
		}
		scope.emplace(name, ValInfo{name, std::move(type), is_mutable, is_global});
		return true;
	}

	ValInfo* lookup_value(const std::string& name) {
		for (auto it = scopes_.rbegin(); it != scopes_.rend(); ++it) {
			auto pos = it->find(name);
			if (pos != it->end()) {
				return &pos->second;
			}
		}
		return nullptr;
	}

	const ValInfo* lookup_value(const std::string& name) const {
		for (auto it = scopes_.rbegin(); it != scopes_.rend(); ++it) {
			auto pos = it->find(name);
			if (pos != it->end()) {
				return &pos->second;
			}
		}
		return nullptr;
	}

	bool register_struct(StructInfo info) {
		auto [it, inserted] = structs_.emplace(info.name, std::move(info));
		return inserted;
	}

	StructInfo* lookup_struct(const std::string& name) {
		auto it = structs_.find(name);
		if (it == structs_.end()) {
			return nullptr;
		}
		return &it->second;
	}

	const StructInfo* lookup_struct(const std::string& name) const {
		auto it = structs_.find(name);
		if (it == structs_.end()) {
			return nullptr;
		}
		return &it->second;
	}

	bool register_adt(AdtInfo info) {
		auto [it, inserted] = adts_.emplace(info.name, std::move(info));
		return inserted;
	}

	AdtInfo* lookup_adt(const std::string& name) {
		auto it = adts_.find(name);
		if (it == adts_.end()) {
			return nullptr;
		}
		return &it->second;
	}

	const AdtInfo* lookup_adt(const std::string& name) const {
		auto it = adts_.find(name);
		if (it == adts_.end()) {
			return nullptr;
		}
		return &it->second;
	}

	bool register_ctor(const std::string& ctor_name, TypePtr ctor_type) {
		constructors_[ctor_name] = std::move(ctor_type);
		return true;
	}

	TypePtr lookup_ctor(const std::string& ctor_name) const {
		auto it = constructors_.find(ctor_name);
		if (it == constructors_.end()) {
			return nullptr;
		}
		return it->second;
	}

private:
	std::vector<std::unordered_map<std::string, ValInfo>> scopes_;
	std::unordered_map<std::string, StructInfo> structs_;
	std::unordered_map<std::string, AdtInfo> adts_;
	std::unordered_map<std::string, TypePtr> constructors_;
};

size_t type_sizeof(const TypePtr& type, const TypeEnv& env);
size_t type_alignof(const TypePtr& type, const TypeEnv& env);
void compute_struct_layout(StructInfo& info, const TypeEnv& env);
StructInfo compute_adt_instance_layout(const std::string& adt_name, const TypePtr& payload, const TypeEnv& env);

#endif

#endif
