#ifndef VIX_TYPE_H
#define VIX_TYPE_H

#ifdef __cplusplus

#include <cstddef>
#include <memory>
#include <string>
#include <utility>
#include <vector>

enum class TypeKind {
    Void,
    I8,
    I32,
    I64,
    F32,
    F64,
    Bool,
    String,
    Ptr,
    Struct,
    Array,
    FixedArray,
    Var,
    App,
    Fn,
    Tuple
};

struct Type;
using TypePtr = std::shared_ptr<Type>;

struct PtrData {
    TypePtr pointee;
};

struct ArrayData {
    TypePtr element;
};

struct FixedArrayData {
    TypePtr element;
    size_t size;
};

struct StructData {
    std::string name;
};

struct VarData {
    int id;
};

struct AppData {
    TypePtr ctor;/*类型构造器*/
    std::vector<TypePtr> args;/*类型参数*/
};

struct FnData {
    std::vector<TypePtr> params;
    TypePtr ret;
    std::vector<int> generic_param_ids;
    bool vararg;
};

struct TupleData {
    std::vector<TypePtr> elements;
};

union TypeData {
    PtrData ptr;
    ArrayData array;
    FixedArrayData fixed_array;
    StructData struct_data;
    VarData var;
    AppData app;
    FnData fn;
    TupleData tuple;

    TypeData() {}
    ~TypeData() {}
};

struct Type {
    TypeKind kind;
    TypeData data;

    Type() : kind(TypeKind::Void) {}

    explicit Type(TypeKind k) : kind(k) {
        switch (kind) {
            case TypeKind::Ptr:
                new (&data.ptr) PtrData{};
                break;
            case TypeKind::Array:
                new (&data.array) ArrayData{};
                break;
            case TypeKind::FixedArray:
                new (&data.fixed_array) FixedArrayData{};
                break;
            case TypeKind::Struct:
                new (&data.struct_data) StructData{};
                break;
            case TypeKind::Var:
                new (&data.var) VarData{-1};
                break;
            case TypeKind::App:
                new (&data.app) AppData{};
                break;
            case TypeKind::Fn:
                new (&data.fn) FnData{};
                break;
            case TypeKind::Tuple:
                new (&data.tuple) TupleData{};
                break;
            default:
                break;
        }
    }

    Type(const Type& other) : kind(other.kind) {
        copy_from(other);
    }

    Type& operator=(const Type& other) {
        if (this == &other) {
            return *this;
        }
        destroy();
        kind = other.kind;
        copy_from(other);
        return *this;
    }

    Type(Type&& other) noexcept : kind(other.kind) {
        move_from(std::move(other));
    }

    Type& operator=(Type&& other) noexcept {
        if (this == &other) {
            return *this;
        }
        destroy();
        kind = other.kind;
        move_from(std::move(other));
        return *this;
    }

    ~Type() {
        destroy();
    }

    static TypePtr make(TypeKind kind) {
        return std::make_shared<Type>(kind);
    }

    static TypePtr make_ptr(TypePtr pointee) {
        TypePtr t = make(TypeKind::Ptr);
        t->data.ptr.pointee = std::move(pointee);
        return t;
    }

    static TypePtr make_array(TypePtr element) {
        TypePtr t = make(TypeKind::Array);
        t->data.array.element = std::move(element);
        return t;
    }

    static TypePtr make_fixed_array(TypePtr element, size_t size) {
        TypePtr t = make(TypeKind::FixedArray);
        t->data.fixed_array.element = std::move(element);
        t->data.fixed_array.size = size;
        return t;
    }

    static TypePtr make_struct(std::string name) {
        TypePtr t = make(TypeKind::Struct);
        t->data.struct_data.name = std::move(name);
        return t;
    }

    static TypePtr make_var(int id) {
        TypePtr t = make(TypeKind::Var);
        t->data.var.id = id;
        return t;
    }

    static TypePtr make_app(TypePtr ctor, std::vector<TypePtr> args) {
        TypePtr t = make(TypeKind::App);
        t->data.app.ctor = std::move(ctor);
        t->data.app.args = std::move(args);
        return t;
    }

    static TypePtr make_fn(std::vector<TypePtr> params, TypePtr ret,
                           std::vector<int> generic_param_ids = {}, bool vararg = false) {
        TypePtr t = make(TypeKind::Fn);
        t->data.fn.params = std::move(params);
        t->data.fn.ret = std::move(ret);
        t->data.fn.generic_param_ids = std::move(generic_param_ids);
        t->data.fn.vararg = vararg;
        return t;
    }

    static TypePtr make_tuple(std::vector<TypePtr> elements) {
        TypePtr t = make(TypeKind::Tuple);
        t->data.tuple.elements = std::move(elements);
        return t;
    }

private:
    void destroy() {
        switch (kind) {
            case TypeKind::Ptr:
                data.ptr.~PtrData();
                break;
            case TypeKind::Array:
                data.array.~ArrayData();
                break;
            case TypeKind::FixedArray:
                data.fixed_array.~FixedArrayData();
                break;
            case TypeKind::Struct:
                data.struct_data.~StructData();
                break;
            case TypeKind::Var:
                data.var.~VarData();
                break;
            case TypeKind::App:
                data.app.~AppData();
                break;
            case TypeKind::Fn:
                data.fn.~FnData();
                break;
            case TypeKind::Tuple:
                data.tuple.~TupleData();
                break;
            default:
                break;
        }
    }

    void copy_from(const Type& other) {
        switch (kind) {
            case TypeKind::Ptr:
                new (&data.ptr) PtrData(other.data.ptr);
                break;
            case TypeKind::Array:
                new (&data.array) ArrayData(other.data.array);
                break;
            case TypeKind::FixedArray:
                new (&data.fixed_array) FixedArrayData(other.data.fixed_array);
                break;
            case TypeKind::Struct:
                new (&data.struct_data) StructData(other.data.struct_data);
                break;
            case TypeKind::Var:
                new (&data.var) VarData(other.data.var);
                break;
            case TypeKind::App:
                new (&data.app) AppData(other.data.app);
                break;
            case TypeKind::Fn:
                new (&data.fn) FnData(other.data.fn);
                break;
            case TypeKind::Tuple:
                new (&data.tuple) TupleData(other.data.tuple);
                break;
            default:
                break;
        }
    }

    void move_from(Type&& other) {
        switch (kind) {
            case TypeKind::Ptr:
                new (&data.ptr) PtrData(std::move(other.data.ptr));
                break;
            case TypeKind::Array:
                new (&data.array) ArrayData(std::move(other.data.array));
                break;
            case TypeKind::FixedArray:
                new (&data.fixed_array) FixedArrayData(std::move(other.data.fixed_array));
                break;
            case TypeKind::Struct:
                new (&data.struct_data) StructData(std::move(other.data.struct_data));
                break;
            case TypeKind::Var:
                new (&data.var) VarData(other.data.var);
                break;
            case TypeKind::App:
                new (&data.app) AppData(std::move(other.data.app));
                break;
            case TypeKind::Fn:
                new (&data.fn) FnData(std::move(other.data.fn));
                break;
            case TypeKind::Tuple:
                new (&data.tuple) TupleData(std::move(other.data.tuple));
                break;
            default:
                break;
        }
    }
};

#else

typedef struct Type Type;

#endif

#endif
