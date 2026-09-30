/*
 * Copyright (c) 2026 Vix Language Authors. All rights reserved.
 * 
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 * 
 *     http://www.apache.org/licenses/LICENSE-2.0
 * 
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
#include "../../include/env.h"

#include <algorithm>

namespace {

size_t align_up(size_t value, size_t align) {
	if (align == 0) {
		return value;
	}
	size_t r = value % align;
	return r == 0 ? value : value + (align - r);
}

}

size_t type_alignof(const TypePtr& type, const TypeEnv& env) {
	if (!type) {
		return 1;
	}

	switch (type->kind) {
		case TypeKind::I8:
		case TypeKind::Bool:
			return 1;
		case TypeKind::I32:
		case TypeKind::F32:
			return 4;
		case TypeKind::I64:
		case TypeKind::F64:
		case TypeKind::Ptr:
		case TypeKind::String:
		case TypeKind::Fn:
			return 8;
		case TypeKind::Array:
		case TypeKind::FixedArray:
			return type_alignof(type->kind == TypeKind::Array ? type->data.array.element
															  : type->data.fixed_array.element,
								env);
		case TypeKind::Struct: {
			const StructInfo* info = env.lookup_struct(type->data.struct_data.name);
			return info ? std::max<size_t>(1, info->align) : 8;
		}
		case TypeKind::App:
			return type_alignof(type->data.app.ctor, env);
		case TypeKind::Tuple: {
			size_t max_a = 1;
			for (const auto& elem : type->data.tuple.elements) {
				max_a = std::max(max_a, type_alignof(elem, env));
			}
			return max_a;
		}
		default:
			return 1;
	}
}

size_t type_sizeof(const TypePtr& type, const TypeEnv& env) {
	if (!type) {
		return 0;
	}

	switch (type->kind) {
		case TypeKind::Void:
			return 0;
		case TypeKind::I8:
		case TypeKind::Bool:
			return 1;
		case TypeKind::I32:
		case TypeKind::F32:
			return 4;
		case TypeKind::I64:
		case TypeKind::F64:
		case TypeKind::Ptr:
		case TypeKind::String:
		case TypeKind::Fn:
			return 8;
		case TypeKind::Array:
			return 16;
		case TypeKind::FixedArray: {
			const size_t elem_sz = type_sizeof(type->data.fixed_array.element, env);
			return elem_sz * type->data.fixed_array.size;
		}
		case TypeKind::Struct: {
			const StructInfo* info = env.lookup_struct(type->data.struct_data.name);
			return info ? info->total_size : 0;
		}
		case TypeKind::App:
			return type_sizeof(type->data.app.ctor, env);
		case TypeKind::Tuple: {
			size_t total = 0;
			size_t max_a = 1;
			for (const auto& elem : type->data.tuple.elements) {
				const size_t a = std::max<size_t>(1, type_alignof(elem, env));
				total = align_up(total, a);
				total += type_sizeof(elem, env);
				max_a = std::max(max_a, a);
			}
			return align_up(total, max_a);
		}
		default:
			return 8;
	}
}

void compute_struct_layout(StructInfo& info, const TypeEnv& env) {
	size_t offset = 0;
	size_t max_align = 1;

	for (auto& field : info.fields) {
		const size_t align = std::max<size_t>(1, type_alignof(field.type, env));
		offset = align_up(offset, align);
		field.offset = offset;
		offset += type_sizeof(field.type, env);
		max_align = std::max(max_align, align);
	}

	info.align = max_align;
	info.total_size = align_up(offset, max_align);
}

StructInfo compute_adt_instance_layout(const std::string& adt_name, const TypePtr& payload, const TypeEnv& env) {
	StructInfo info;
	info.name = adt_name + "$instance";

	StructFieldInfo tag_field;
	tag_field.name = "tag";
	tag_field.type = Type::make(TypeKind::Bool);
	tag_field.offset = 0;
	info.fields.push_back(tag_field);

	StructFieldInfo value_field;
	value_field.name = "value";
	value_field.type = payload ? payload : Type::make(TypeKind::I8);
	value_field.offset = 0;
	info.fields.push_back(value_field);

	compute_struct_layout(info, env);
	return info;
}
