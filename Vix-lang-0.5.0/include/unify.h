#ifndef VIX_UNIFY_H
#define VIX_UNIFY_H

#include <sstream>
#include <stdexcept>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>

#include "type.h"

class Unifier {
public:
  Unifier() : next_var_id_(1) {}

  TypePtr fresh() { return make_var(next_var_id_++); }

  TypePtr apply(const TypePtr &t) {
    if (!t) {
      return t;
    }
    if (t->kind != TypeKind::Var) {
      switch (t->kind) {
      case TypeKind::Ptr:
        return Type::make_ptr(apply(t->data.ptr.pointee));
      case TypeKind::Array:
        return Type::make_array(apply(t->data.array.element));
      case TypeKind::FixedArray:
        return Type::make_fixed_array(apply(t->data.fixed_array.element),
                                      t->data.fixed_array.size);
      case TypeKind::App: {
        std::vector<TypePtr> args;
        args.reserve(t->data.app.args.size());
        for (const auto &arg : t->data.app.args) {
          args.push_back(apply(arg));
        }
        return Type::make_app(apply(t->data.app.ctor), std::move(args));
      }
      case TypeKind::Fn: {
        std::vector<TypePtr> params;
        params.reserve(t->data.fn.params.size());
        for (const auto &param : t->data.fn.params) {
          params.push_back(apply(param));
        }
        return Type::make_fn(std::move(params), apply(t->data.fn.ret),
                             t->data.fn.generic_param_ids, t->data.fn.vararg);
      }
      case TypeKind::Tuple: {
        std::vector<TypePtr> elems;
        elems.reserve(t->data.tuple.elements.size());
        for (const auto &elem : t->data.tuple.elements) {
          elems.push_back(apply(elem));
        }
        return Type::make_tuple(std::move(elems));
      }
      default:
        return t;
      }
    }
    return resolve_var(t->data.var.id);
  }

  void unify(const TypePtr &lhs, const TypePtr &rhs) {
    begin_transaction();
    try {
      unify_impl(lhs, rhs);
      commit();
    } catch (...) {
      rollback();
      throw;
    }
  }

  void begin_transaction() {
    undo_subst_stack_.push_back(subst_);
    undo_rank_stack_.push_back(rank_);
  }

  void commit() {
    if (!undo_subst_stack_.empty()) {
      undo_subst_stack_.pop_back();
      undo_rank_stack_.pop_back();
    }
  }

  void rollback() {
    if (!undo_subst_stack_.empty()) {
      subst_ = std::move(undo_subst_stack_.back());
      rank_ = std::move(undo_rank_stack_.back());
      undo_subst_stack_.pop_back();
      undo_rank_stack_.pop_back();
    }
  }

  std::string pretty(const TypePtr &t) {
    if (!t) {
      return "<null>";
    }
    TypePtr a = apply(t);
    switch (a->kind) {
    case TypeKind::Void:
      return "Void";
    case TypeKind::I8:
      return "I8";
    case TypeKind::I32:
      return "I32";
    case TypeKind::I64:
      return "I64";
    case TypeKind::F32:
      return "F32";
    case TypeKind::F64:
      return "F64";
    case TypeKind::Bool:
      return "Bool";
    case TypeKind::String:
      return "String";
    case TypeKind::Ptr:
      return "Ptr[" + pretty(a->data.ptr.pointee) + "]";
    case TypeKind::Struct:
      return a->data.struct_data.name;
    case TypeKind::Array:
      return "Array[" + pretty(a->data.array.element) + "]";
    case TypeKind::FixedArray: {
      std::ostringstream os;
      os << "FixedArray[" << pretty(a->data.fixed_array.element) << ","
         << a->data.fixed_array.size << "]";
      return os.str();
    }
    case TypeKind::Var: {
      std::ostringstream os;
      os << "T" << a->data.var.id;
      return os.str();
    }
    case TypeKind::App: {
      std::ostringstream os;
      os << pretty(a->data.app.ctor) << "[";
      for (size_t i = 0; i < a->data.app.args.size(); ++i) {
        if (i)
          os << ",";
        os << pretty(a->data.app.args[i]);
      }
      os << "]";
      return os.str();
    }
    case TypeKind::Fn: {
      std::ostringstream os;
      os << "Fn(";
      for (size_t i = 0; i < a->data.fn.params.size(); ++i) {
        if (i)
          os << ",";
        os << pretty(a->data.fn.params[i]);
      }
      os << ")->" << pretty(a->data.fn.ret);
      return os.str();
    }
    case TypeKind::Tuple: {
      std::ostringstream os;
      os << "(";
      for (size_t i = 0; i < a->data.tuple.elements.size(); ++i) {
        if (i)
          os << ",";
        os << pretty(a->data.tuple.elements[i]);
      }
      os << ")";
      return os.str();
    }
    }
    return "<unknown>";
  }

private:
  int next_var_id_;
  std::unordered_map<int, TypePtr> subst_;
  std::unordered_map<int, size_t> rank_;
  std::vector<std::unordered_map<int, TypePtr>> undo_subst_stack_;
  std::vector<std::unordered_map<int, size_t>> undo_rank_stack_;
  std::unordered_map<int, TypePtr> var_pool_;

  TypePtr make_var(int id) {
    auto &entry = var_pool_[id];
    if (!entry) {
      entry = Type::make_var(id);
    }
    return entry;
  }

  TypePtr resolve_var(int var_id) {
    std::unordered_set<int> visited;
    std::vector<int> path;
    int cur_id = var_id;
    while (true) {
      if (visited.count(cur_id)) {
        throw std::runtime_error("cycle detected in type substitutions");
      }
      visited.insert(cur_id);
      auto it = subst_.find(cur_id);
      if (it == subst_.end()) {
        TypePtr free_var = make_var(cur_id);
        for (int id : path) {
          subst_[id] = free_var;
        }
        return free_var;
      }
      TypePtr val = it->second;
      if (val->kind != TypeKind::Var) {
        TypePtr resolved = apply(val);
        for (int id : path) {
          subst_[id] = resolved;
        }
        subst_[cur_id] = resolved;
        return resolved;
      }
      path.push_back(cur_id);
      cur_id = val->data.var.id;
    }
  }

  bool occurs(int var_id, const TypePtr &t) {
    std::unordered_set<int> visited;
    return occurs_impl(var_id, t, visited);
  }

  bool occurs_impl(int var_id, const TypePtr &t,
                   std::unordered_set<int> &visited) {
    TypePtr a = apply(t);
    if (!a) {
      return false;
    }
    if (a->kind == TypeKind::Var) {
      if (a->data.var.id == var_id) {
        return true;
      }
      if (visited.count(a->data.var.id)) {
        return false;
      }
      visited.insert(a->data.var.id);
      auto it = subst_.find(a->data.var.id);
      if (it != subst_.end()) {
        return occurs_impl(var_id, it->second, visited);
      }
      return false;
    }
    switch (a->kind) {
    case TypeKind::Ptr:
      return occurs_impl(var_id, a->data.ptr.pointee, visited);
    case TypeKind::Array:
      return occurs_impl(var_id, a->data.array.element, visited);
    case TypeKind::FixedArray:
      return occurs_impl(var_id, a->data.fixed_array.element, visited);
    case TypeKind::App: {
      if (occurs_impl(var_id, a->data.app.ctor, visited)) {
        return true;
      }
      for (const auto &arg : a->data.app.args) {
        if (occurs_impl(var_id, arg, visited)) {
          return true;
        }
      }
      return false;
    }
    case TypeKind::Fn: {
      for (const auto &param : a->data.fn.params) {
        if (occurs_impl(var_id, param, visited)) {
          return true;
        }
      }
      return occurs_impl(var_id, a->data.fn.ret, visited);
    }
    case TypeKind::Tuple: {
      for (const auto &elem : a->data.tuple.elements) {
        if (occurs_impl(var_id, elem, visited)) {
          return true;
        }
      }
      return false;
    }
    default:
      return false;
    }
  }

  size_t get_rank(int id) {
    auto it = rank_.find(id);
    if (it == rank_.end()) {
      return 1;
    }
    return it->second;
  }

  void increase_rank(int id) { ++rank_[id]; }

  void set_binding(int id, const TypePtr &val) { subst_[id] = val; }

  void bind_var(int var_id, const TypePtr &t) {
    TypePtr a = apply(t);
    if (a->kind == TypeKind::Var) {
      int other_id = a->data.var.id;
      if (other_id == var_id) {
        return;
      }
      size_t rank1 = get_rank(var_id);
      size_t rank2 = get_rank(other_id);
      if (rank1 < rank2) {
        set_binding(var_id, a);
      } else if (rank1 > rank2) {
        set_binding(other_id, make_var(var_id));
      } else {
        set_binding(var_id, a);
        increase_rank(other_id);
      }
      return;
    }
    if (occurs(var_id, a)) {
      throw std::runtime_error("occurs check failed");
    }
    set_binding(var_id, a);
  }

  void unify_impl(const TypePtr &lhs, const TypePtr &rhs) {
    TypePtr a = apply(lhs);
    TypePtr b = apply(rhs);
    if (!a || !b) {
      throw std::runtime_error("cannot unify null types");
    }
    if (a->kind == TypeKind::Var) {
      bind_var(a->data.var.id, b);
      return;
    }
    if (b->kind == TypeKind::Var) {
      bind_var(b->data.var.id, a);
      return;
    }
    if (a->kind != b->kind) {
      bool compatible =
          (a->kind == TypeKind::Bool &&
           (b->kind == TypeKind::I32 || b->kind == TypeKind::I8)) ||
          (b->kind == TypeKind::Bool &&
           (a->kind == TypeKind::I32 || a->kind == TypeKind::I8));
      if (!compatible) {
        bool a_numeric = (a->kind == TypeKind::I8 || a->kind == TypeKind::I32 ||
                          a->kind == TypeKind::I64 ||
                          a->kind == TypeKind::F32 || a->kind == TypeKind::F64);
        bool b_numeric = (b->kind == TypeKind::I8 || b->kind == TypeKind::I32 ||
                          b->kind == TypeKind::I64 ||
                          b->kind == TypeKind::F32 || b->kind == TypeKind::F64);
        compatible = a_numeric && b_numeric;
      }
      if (!compatible && a->kind == TypeKind::Struct &&
          b->kind == TypeKind::App && b->data.app.args.empty()) {
        unify_impl(a, b->data.app.ctor);
        return;
      }
      if (!compatible && b->kind == TypeKind::Struct &&
          a->kind == TypeKind::App && a->data.app.args.empty()) {
        unify_impl(a->data.app.ctor, b);
        return;
      }
      if (!compatible) {
        throw std::runtime_error("expected type '" + pretty(a) +
                                 "', but got '" + pretty(b) + "'");
      }
      return;
    }
    switch (a->kind) {
    case TypeKind::Ptr:
      unify_impl(a->data.ptr.pointee, b->data.ptr.pointee);
      break;
    case TypeKind::Array:
      unify_impl(a->data.array.element, b->data.array.element);
      break;
    case TypeKind::FixedArray:
      if (a->data.fixed_array.size != b->data.fixed_array.size) {
        throw std::runtime_error("fixed array size mismatch");
      }
      unify_impl(a->data.fixed_array.element, b->data.fixed_array.element);
      break;
    case TypeKind::Struct:
      if (b->kind == TypeKind::App && b->data.app.args.empty()) {
        unify_impl(a, b->data.app.ctor);
        return;
      }
      if (a->data.struct_data.name != b->data.struct_data.name) {
        throw std::runtime_error("expected struct '" +
                                 a->data.struct_data.name + "', but got '" +
                                 b->data.struct_data.name + "'");
      }
      break;
    case TypeKind::App:
      if (b->kind == TypeKind::Struct && a->data.app.args.empty()) {
        unify_impl(a->data.app.ctor, b);
        return;
      }
      unify_impl(a->data.app.ctor, b->data.app.ctor);
      if (a->data.app.args.size() != b->data.app.args.size()) {
        throw std::runtime_error("type application arity mismatch");
      }
      for (size_t i = 0; i < a->data.app.args.size(); ++i) {
        unify_impl(a->data.app.args[i], b->data.app.args[i]);
      }
      break;
    case TypeKind::Fn:
      if (a->data.fn.params.size() != b->data.fn.params.size()) {
        throw std::runtime_error("function arity mismatch");
      }
      for (size_t i = 0; i < a->data.fn.params.size(); ++i) {
        unify_impl(a->data.fn.params[i], b->data.fn.params[i]);
      }
      unify_impl(a->data.fn.ret, b->data.fn.ret);
      break;
    case TypeKind::Tuple:
      if (a->data.tuple.elements.size() != b->data.tuple.elements.size()) {
        throw std::runtime_error("tuple arity mismatch");
      }
      for (size_t i = 0; i < a->data.tuple.elements.size(); ++i) {
        unify_impl(a->data.tuple.elements[i], b->data.tuple.elements[i]);
      }
      break;
    default:
      break;
    }
  }
};

#endif
