#include "WasmTypeMap.h"
#include "binaryen-c.h"

WasmTypeInfo map_vix_type_to_wasm(const Type *type) {
    WasmTypeInfo info = {};
    switch (type->kind) {
        case TypeKind::I32:
        case TypeKind::Bool:
            info.val_type = BinaryenTypeInt32();
            info.wasm_memory_size = 4;
            break;
        case TypeKind::I64:
            info.val_type = BinaryenTypeInt64();
            info.wasm_memory_size = 8;
            break;
        case TypeKind::F32:
            info.val_type = BinaryenTypeFloat32();
            info.wasm_memory_size = 4;
            break;
        case TypeKind::F64:
            info.val_type = BinaryenTypeFloat64();
            info.wasm_memory_size = 8;
            break;
        case TypeKind::Ptr:
        case TypeKind::String:
        case TypeKind::Array:
        case TypeKind::FixedArray:
            info.val_type = BinaryenTypeInt32(); // pointer in WASM
            info.wasm_memory_size = 4;
            break;
        case TypeKind::Struct:
        case TypeKind::App:
        case TypeKind::Tuple:
            info.val_type = BinaryenTypeInt32(); // structs by pointer
            info.is_struct = true;
            info.wasm_memory_size = 4;
            break;
        default:
            info.val_type = BinaryenTypeInt32();
            info.wasm_memory_size = 4;
            break;
    }
    return info;
}
