/**
 * @file ownership.cpp
 * @brief Ownership and borrow checker implementation
 *
 * This module traverses the abstract syntax tree (AST) and enforces Rust-like
 * ownership and borrowing rules. Core features include:
 *   - Tracking the move state of each variable
 *   - Checking the legality of mutable/immutable borrows (only one mutable
 * borrow or any number of immutable borrows at a time)
 *   - Handling control-flow branch (if/else) state merging
 *   - Supporting transaction rollback for branch preview
 *   - Checking whether a returned borrow refers to a local variable
 *   - Field-level borrow precision: borrowing `p.x` does not lock the entire
 * struct `p`
 *   - NLL (Non-Lexical Lifetimes): a borrow ends when the borrow variable's
 * last use is passed, not at end of scope
 */

#include "../../include/ownership.h"
#include "../../include/compiler.h"

#include <cctype>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>

extern "C" const char *current_input_filename;

namespace {

/**
 * @brief Expression usage type, used to distinguish read, move, take address,
 * and assignment target
 */
enum class ExprUse { Read, Move, Address, AssignTarget, ReadField };

/**
 * @brief Per-field borrow state for struct fields
 */
struct FieldBorrowState {
  bool borrowed_shared = false; ///< Whether this field is immutably borrowed
  bool borrowed_mut = false;    ///< Whether this field is mutably borrowed
};

/**
 * @brief Full state of a variable, used to track ownership, borrowing, and
 * moves
 */
struct VarState {
  std::string name;               ///< Variable name
  int scope_level = -1;           ///< Scope depth (0 is global)
  const TypeInfo *type = nullptr; ///< Type information
  bool is_mutable = false;        ///< Whether mutable
  bool moved = false;             ///< Whether moved (ownership transferred)
  ASTNode *moved_at =
      nullptr; ///< AST node where the move occurred (for error reporting)
  bool is_global = false; ///< Whether a global variable
  /// Whole-variable borrow flags (set by `ref p` / `mut ref p`)
  bool whole_borrowed_shared = false;
  bool whole_borrowed_mut = false;
  /// Per-field borrow flags (set by `ref p.x` / `mut ref p.x`)
  std::unordered_map<std::string, FieldBorrowState> field_borrows;
  std::unordered_set<std::string>
      borrow_sources; ///< Source variable names of current borrows (for chain
                      ///< tracking)
};

/**
 * @brief A scope containing all variables within that scope
 */
struct Scope {
  std::unordered_map<std::string, VarState> vars;
};

/**
 * @brief Information after evaluating an expression
 */
struct ExprInfo {
  const TypeInfo *type = nullptr; ///< Expression type
  bool copy = true;               ///< Whether copyable (Copy trait)
  bool is_ref = false;            ///< Whether a reference type
  std::unordered_set<std::string>
      borrow_sources; ///< Source variables that this expression refers to
};

/**
 * @brief Transaction log entry, recording the state of a variable before the
 * transaction began
 */
struct LogEntry {
  int scope_level;
  std::string name;
  VarState old_state;
};

/**
 * @brief Branch state snapshot, used for if/else merging
 */
struct BranchState {
  bool moved = false;
  bool whole_borrowed_shared = false;
  bool whole_borrowed_mut = false;
  std::unordered_set<std::string> borrow_sources;
  std::unordered_map<std::string, FieldBorrowState> field_borrows;
};

/**
 * @brief Main ownership checker class
 *
 * Traverses the AST, maintains a scope stack and variable states, and detects
 * errors such as use-after-move and illegal borrows. Supports a transaction
 * mechanism to handle branch merging: record state before entering a branch,
 * roll back after branch checking, then merge the side effects of each branch.
 *
 * **Field-level borrow precision**: borrowing `ref p.x` marks only
 * `field_borrows["x"]`, not the whole struct. Other fields remain accessible.
 *
 * **NLL (Non-Lexical Lifetimes)**: a pre-scan records the last use line for
 * each variable in a function. In `cleanup_borrows()`, borrows held by dead
 * variables (past their last use) are eagerly released.
 */
class OwnershipChecker {
public:
  /**
   * @brief Public entry point: perform ownership checking on the entire program
   * AST
   * @param root AST root node
   * @return 0 indicates check passed, 1 indicates errors exist
   */
  int check(ASTNode *root) {
    push_scope();
    check_node(root);
    pop_scope();
    return errors;
  }

private:
  std::vector<Scope> scopes; ///< Scope stack
  int errors = 0;            ///< Accumulated error count
  int current_seq = 0;       ///< Current statement sequence number (for NLL)

  // NLL pre-scan: last sequence number where each variable is used
  std::unordered_map<std::string, int> last_use_seq;

  // Transaction related
  std::vector<std::vector<LogEntry>>
      transaction_logs; ///< Transaction log stack (one log list per
                        ///< transaction)
  size_t transaction_baseline_depth =
      0; ///< Scope depth at transaction start, used to limit logging range

  static const char *node_file(const ASTNode *node) {
    if (node && node->source_file)
      return node->source_file;
    return current_input_filename ? current_input_filename : "unknown";
  }

  static int node_line(const ASTNode *node) {
    return node && node->location.first_line > 0 ? node->location.first_line
                                                 : 1;
  }

  static int node_col(const ASTNode *node) {
    return node && node->location.first_column > 0 ? node->location.first_column
                                                   : 1;
  }

  /**
   * @brief Determine whether a type satisfies Copy semantics (can be implicitly
   * copied)
   * @details Primitives, pointers, function pointers are always Copy;
   *          structs are Copy if all fields are Copy;
   *          strings, dynamic arrays, generic applications are non-Copy by
   * default; fixed-size arrays are considered Copy if size ≤ 16 and element
   * type is Copy (optimization for small arrays)
   */
  static bool is_copy_type(const TypeInfo *t) {
    if (!t)
      return true;
    switch (t->kind) {
    case TYPEINFO_VOID:
    case TYPEINFO_I8:
    case TYPEINFO_I32:
    case TYPEINFO_I64:
    case TYPEINFO_F32:
    case TYPEINFO_F64:
    case TYPEINFO_BOOL:
    case TYPEINFO_PTR:
    case TYPEINFO_FN:
    case TYPEINFO_VAR:
      return true;
    case TYPEINFO_STRING:
      return false;
    case TYPEINFO_STRUCT:
      if (t->params && t->param_count > 0) {
        for (int i = 0; i < t->param_count; i++) {
          if (!is_copy_type(t->params[i]))
            return false;
        }
      }
      return true;
    case TYPEINFO_FIXED_ARRAY:
      return t->size <= 16 && is_copy_type(t->element);
    case TYPEINFO_ARRAY:
    case TYPEINFO_APP:
      return false;
    }
    return true;
  }

  void push_scope() { scopes.emplace_back(); }
  void pop_scope() {
    if (!scopes.empty())
      scopes.pop_back();
  }

  /**
   * @brief Report a semantic error, set location and call the frontend error
   * interface
   */
  void report(ASTNode *node, const std::string &message) {
    set_location_with_column(node_file(node), node_line(node), node_col(node));
    int length = 1;
    // Extract variable name from messages like "use of moved value 'buf'"
    auto start = message.find('\'');
    auto end = message.find('\'', start + 1);
    if (start != std::string::npos && end != std::string::npos && end > start) {
      std::string vname = message.substr(start + 1, end - start - 1);
      adjust_column_to_identifier(vname.c_str());
      length = (int)vname.size();
    }
    report_simple_error_with_length(ERROR_LEVEL_ERROR, ERROR_SEMANTIC,
                                    message.c_str(), length);
    errors++;
  }

  /**
   * @brief Look up a variable starting from the current scope upwards
   */
  VarState *lookup(const std::string &name) {
    for (auto it = scopes.rbegin(); it != scopes.rend(); ++it) {
      auto found = it->vars.find(name);
      if (found != it->vars.end())
        return &found->second;
    }
    return nullptr;
  }

  /**
   * @brief Check whether a name is a local variable (non-global)
   */
  bool is_local_name(const std::string &name) const {
    if (name.empty())
      return false;
    for (auto it = scopes.rbegin(); it != scopes.rend(); ++it) {
      auto found = it->vars.find(name);
      if (found != it->vars.end())
        return !found->second.is_global;
    }
    return false;
  }

  // ---- NLL pre-scan: record last statement sequence for each variable ----

  /**
   * @brief Walk the AST children of a statement and record identifier uses at
   * the given sequence number
   */
  void scan_node_ids(ASTNode *node, int seq,
                     std::unordered_map<std::string, int> &uses) {
    if (!node)
      return;
    if (node->type == AST_IDENTIFIER && node->data.identifier.name) {
      uses[node->data.identifier.name] = seq;
    }
    switch (node->type) {
    case AST_ASSIGN:
    case AST_CONST:
      scan_node_ids(node->data.assign.left, seq, uses);
      scan_node_ids(node->data.assign.right, seq, uses);
      break;
    case AST_PRINT:
      scan_node_ids(node->data.print.expr, seq, uses);
      break;
    case AST_RETURN:
      scan_node_ids(node->data.return_stmt.expr, seq, uses);
      break;
    case AST_CALL:
      scan_node_ids(node->data.call.func, seq, uses);
      scan_node_ids(node->data.call.args, seq, uses);
      break;
    case AST_INDEX:
      scan_node_ids(node->data.index.target, seq, uses);
      scan_node_ids(node->data.index.index, seq, uses);
      break;
    case AST_MEMBER_ACCESS:
      scan_node_ids(node->data.member_access.object, seq, uses);
      scan_node_ids(node->data.member_access.field, seq, uses);
      break;
    case AST_UNARYOP:
      scan_node_ids(node->data.unaryop.expr, seq, uses);
      break;
    case AST_BINOP:
      scan_node_ids(node->data.binop.left, seq, uses);
      scan_node_ids(node->data.binop.right, seq, uses);
      break;
    case AST_EXPRESSION_LIST:
      for (int i = 0; i < node->data.expression_list.expression_count; i++)
        scan_node_ids(node->data.expression_list.expressions[i], seq, uses);
      break;
    case AST_STRUCT_LITERAL:
      scan_node_ids(node->data.struct_literal.fields, seq, uses);
      break;
    case AST_IDENTIFIER:
    case AST_NUM_INT:
    case AST_NUM_FLOAT:
    case AST_CHAR:
    case AST_NIL:
    case AST_STRING:
      break; // leaf nodes
    default:
      break;
    }
  }

  /**
   * @brief Walk a function body in statement order, assigning sequence numbers,
   * and record the last sequence where each variable is used.
   *
   * This mirrors the traversal order of check_stmt so that NLL can compare
   * current_seq against last_use_seq.
   */
  void scan_last_use_seq(ASTNode *node, int &seq,
                         std::unordered_map<std::string, int> &uses) {
    if (!node)
      return;
    switch (node->type) {
    case AST_PROGRAM:
      for (int i = 0; i < node->data.program.statement_count; i++)
        scan_last_use_seq(node->data.program.statements[i], seq, uses);
      break;
    case AST_FUNCTION:
      scan_last_use_seq(node->data.function.params, seq, uses);
      scan_last_use_seq(node->data.function.body, seq, uses);
      break;
    case AST_ASSIGN:
    case AST_CONST:
    case AST_PRINT:
    case AST_RETURN:
    case AST_BREAK:
    case AST_CONTINUE:
      seq++;
      scan_node_ids(node, seq, uses);
      break;
    case AST_IF: {
      // Condition is checked before the branch
      seq++;
      scan_node_ids(node->data.if_stmt.condition, seq, uses);
      // Then-branch
      scan_last_use_seq(node->data.if_stmt.then_body, seq, uses);
      // Else-branch (if any)
      if (node->data.if_stmt.else_body)
        scan_last_use_seq(node->data.if_stmt.else_body, seq, uses);
      break;
    }
    case AST_WHILE:
      seq++;
      scan_node_ids(node->data.while_stmt.condition, seq, uses);
      scan_last_use_seq(node->data.while_stmt.body, seq, uses);
      break;
    case AST_FOR:
      seq++;
      scan_node_ids(node->data.for_stmt.start, seq, uses);
      scan_node_ids(node->data.for_stmt.end, seq, uses);
      scan_last_use_seq(node->data.for_stmt.body, seq, uses);
      break;
    case AST_GLOBAL:
      scan_last_use_seq(node->data.global_decl.initializer, seq, uses);
      break;
    default:
      // Expression-like nodes that go through check_stmt default case
      seq++;
      scan_node_ids(node, seq, uses);
      break;
    }
  }

  // ---- Transaction helpers ----

  /**
   * @brief Begin a new transaction for branch preview
   * @details Record the current scope depth; subsequent modifications to
   * variables will be logged, so they can be restored on rollback.
   */
  void begin_transaction() {
    transaction_logs.emplace_back();
    transaction_baseline_depth = scopes.size();
  }

  /**
   * @brief Commit the current transaction, merging the log into the parent
   * transaction (if any)
   * @details If a parent transaction exists, merge current log entries into the
   * parent log (deduplicating), indicating that these modifications should be
   * preserved after branch merging.
   */
  void commit_transaction() {
    if (transaction_logs.empty())
      return;
    auto committed = std::move(transaction_logs.back());
    transaction_logs.pop_back();
    if (!transaction_logs.empty()) {
      auto &parent = transaction_logs.back();
      for (auto &entry : committed) {
        bool present = false;
        for (auto &p : parent) {
          if (p.scope_level == entry.scope_level && p.name == entry.name) {
            present = true;
            break;
          }
        }
        if (!present)
          parent.push_back(std::move(entry));
      }
    }
  }

  /**
   * @brief Rollback the current transaction, restoring all modified variables
   * to their state before the transaction began
   */
  void rollback_transaction() {
    if (transaction_logs.empty())
      return;
    auto &log = transaction_logs.back();
    for (auto it = log.rbegin(); it != log.rend(); ++it) {
      if (it->scope_level >= 0 && it->scope_level < (int)scopes.size()) {
        auto &scope = scopes[it->scope_level];
        auto var = scope.vars.find(it->name);
        if (var != scope.vars.end()) {
          var->second = it->old_state;
        }
      }
    }
    transaction_logs.pop_back();
  }

  /**
   * @brief Log a variable modification for the transaction log
   * @details Only when currently in a transaction, the variable is in an outer
   * scope (not newly created within the transaction), and not yet recorded,
   * save the current state as the old state.
   */
  void log_var_modification(VarState *state) {
    if (transaction_logs.empty())
      return;
    if (state->scope_level < 0)
      return;
    if ((size_t)state->scope_level >= transaction_baseline_depth)
      return;
    auto &current_log = transaction_logs.back();
    for (auto &entry : current_log) {
      if (entry.scope_level == state->scope_level && entry.name == state->name)
        return;
    }
    current_log.push_back({state->scope_level, state->name, *state});
  }

  /**
   * @brief Capture the state of all currently visible variables (for if
   * baseline)
   */
  void
  capture_all_variables(std::unordered_map<std::string, BranchState> &out) {
    for (auto &scope : scopes) {
      for (auto &entry : scope.vars) {
        auto &var = entry.second;
        BranchState bs;
        bs.moved = var.moved;
        bs.whole_borrowed_shared = var.whole_borrowed_shared;
        bs.whole_borrowed_mut = var.whole_borrowed_mut;
        bs.borrow_sources = var.borrow_sources;
        bs.field_borrows = var.field_borrows;
        out[var.name] = std::move(bs);
      }
    }
  }

  /**
   * @brief Capture variable states in outer scopes (outside the transaction
   * baseline), used for post-branch snapshots
   */
  void capture_outer_branch_state(
      std::unordered_map<std::string, BranchState> &out) {
    for (auto &scope : scopes) {
      for (auto &entry : scope.vars) {
        auto &var = entry.second;
        if (var.scope_level < 0 ||
            (size_t)var.scope_level >= transaction_baseline_depth)
          continue;
        BranchState bs;
        bs.moved = var.moved;
        bs.whole_borrowed_shared = var.whole_borrowed_shared;
        bs.whole_borrowed_mut = var.whole_borrowed_mut;
        bs.borrow_sources = var.borrow_sources;
        bs.field_borrows = var.field_borrows;
        out[var.name] = std::move(bs);
      }
    }
  }

  void declare_var(const std::string &name, const TypeInfo *type,
                   bool is_mutable, bool is_global,
                   const std::unordered_set<std::string> &borrow_sources = {}) {
    if (scopes.empty())
      push_scope();
    VarState state;
    state.name = name;
    state.scope_level = (int)scopes.size() - 1;
    state.type = type;
    state.is_mutable = is_mutable;
    state.is_global = is_global;
    state.borrow_sources = borrow_sources;
    scopes.back().vars[name] = std::move(state);
  }

  /**
   * @brief Get the field name from a member access expression
   * @return The field name string, or empty if not a field access
   */
  static std::string field_name(ASTNode *node) {
    if (!node || node->type != AST_MEMBER_ACCESS)
      return {};
    ASTNode *field = node->data.member_access.field;
    if (!field)
      return {};
    if (field->type == AST_IDENTIFIER && field->data.identifier.name)
      return field->data.identifier.name;
    return {};
  }

  /**
   * @brief Check if a variable has any active field-level borrows
   */
  static bool has_active_field_borrows(const VarState &state) {
    for (auto &[fname, fb] : state.field_borrows) {
      if (fb.borrowed_shared || fb.borrowed_mut)
        return true;
    }
    return false;
  }

  /**
   * @brief Cleanup borrow flags on all variables.
   * @details Scan for active borrow sources; clear whole-struct and field-level
   * borrow flags on any variable not actively borrowed from.
   */
  void cleanup_borrows() {
    std::unordered_set<std::string> active_borrow_sources;
    for (auto &scope : scopes) {
      for (auto &[vname, state] : scope.vars) {
        for (const auto &src : state.borrow_sources) {
          active_borrow_sources.insert(src);
        }
      }
    }
    for (auto &scope : scopes) {
      for (auto &[vname, state] : scope.vars) {
        if (active_borrow_sources.find(vname) ==
            active_borrow_sources.end()) {
          log_var_modification(&state);
          state.whole_borrowed_shared = false;
          state.whole_borrowed_mut = false;
          state.field_borrows.clear();
        }
      }
    }
  }

  /**
   * @brief NLL pre-statement cleanup: release borrows held by dead variables,
   * then clean up borrow flags.
   */
  void nll_cleanup() {
    if (!last_use_seq.empty()) {
      for (auto &scope : scopes) {
        for (auto &[name, state] : scope.vars) {
          if (!state.borrow_sources.empty()) {
            auto it = last_use_seq.find(name);
            if (it != last_use_seq.end() && current_seq > it->second) {
              state.borrow_sources.clear();
            }
          }
        }
      }
    }
    cleanup_borrows();
  }

  void check_stmt(ASTNode *stmt);

  void check_block(ASTNode *block, bool new_scope);

  void check_function(ASTNode *fn);

  void check_global(ASTNode *node);

  void check_return(ASTNode *node);

  void check_assign(ASTNode *node);

  ExprInfo check_expr(ASTNode *node, ExprUse use);

  ExprInfo check_identifier(ASTNode *node, ExprUse use);

  ExprInfo check_unary(ASTNode *node);

  ExprInfo check_call(ASTNode *node);

  ExprInfo check_expression_list(ASTNode *node, ExprUse use = ExprUse::Move);

  ExprInfo check_struct_literal(ASTNode *node);

  /**
   * @brief Count the number of dereference levels of an expression
   */
  static int count_derefs(ASTNode *node);

  /**
   * @brief Expand a variable name through pointer borrow_sources.
   *
   * @param name The variable name to start from.
   * @param max_levels Number of pointer dereferences to expand; -1 means
   * unlimited (until fixed point).
   * @return Set of all ultimate variable names after `max_levels` dereferences.
   */
  std::unordered_set<std::string>
  resolve_pointer_targets(const std::string &name, int max_levels = -1);

  /**
   * @brief Get the underlying variable name of an expression (stripping member
   * access, subscript, dereference etc.)
   * @return the bottommost identifier name, or empty if cannot be extracted
   */
  std::string lvalue_base(ASTNode *node);

  void check_node(ASTNode *node);
};

/**
 * @brief Expand a variable name through pointer borrow_sources.
 *
 * Starting from `name`, follow `borrow_sources` chains up to `max_levels`
 * times. If `max_levels` is -1, expand until no further pointer indirection
 * with known borrow sources is found. Cycles are prevented via a visited set.
 *
 * @return The set of all possible final variable names after the expansion.
 */
std::unordered_set<std::string>
OwnershipChecker::resolve_pointer_targets(const std::string &name,
                                          int max_levels) {
  std::unordered_set<std::string> current;
  if (name.empty())
    return {};
  current.insert(name);
  int level = 0;
  while ((max_levels == -1 || level < max_levels) && !current.empty()) {
    std::unordered_set<std::string> next;
    for (const auto &var : current) {
      VarState *state = lookup(var);
      if (!state) {
        next.insert(var);
        continue;
      }
      if (state->type && state->type->kind == TYPEINFO_PTR &&
          !state->borrow_sources.empty()) {
        next.insert(state->borrow_sources.begin(), state->borrow_sources.end());
      } else {
        next.insert(var);
      }
    }
    if (next == current)
      break;
    current = std::move(next);
    ++level;
  }
  // Safety fallback: if expansion returned nothing, fall back to the original
  // name so callers never get an ambiguous empty target set.
  if (current.empty())
    current.insert(name);
  return current;
}

/**
 * @brief Check a single statement
 */
void OwnershipChecker::check_stmt(ASTNode *stmt) {
  if (!stmt)
    return;
  current_seq++;

  // NLL: Before checking this statement, release borrows held by dead
  // variables and clean up borrow flags.
  nll_cleanup();

  switch (stmt->type) {
  case AST_PROGRAM:
    check_block(stmt, true);
    return;
  case AST_FUNCTION:
    check_function(stmt);
    return;
  case AST_ASSIGN:
  case AST_CONST:
    check_assign(stmt);
    cleanup_borrows();
    return;
  case AST_PRINT:
    check_expr(stmt->data.print.expr, ExprUse::Read);
    cleanup_borrows();
    return;
  case AST_RETURN:
    check_return(stmt);
    cleanup_borrows();
    return;
  case AST_IF: {
    // Check condition expression (read-only)
    check_expr(stmt->data.if_stmt.condition, ExprUse::Read);
    cleanup_borrows();

    bool has_else = (stmt->data.if_stmt.else_body != nullptr);

    // Save state before entering the branch as baseline
    std::unordered_map<std::string, BranchState> baseline;
    capture_all_variables(baseline);

    // Check then branch, use transaction for rollback
    begin_transaction();
    check_block(stmt->data.if_stmt.then_body, true);
    std::unordered_map<std::string, BranchState> then_state;
    capture_outer_branch_state(then_state);
    rollback_transaction();

    // Check else branch (if any), also use transaction
    std::unordered_map<std::string, BranchState> else_state;
    if (has_else) {
      begin_transaction();
      check_block(stmt->data.if_stmt.else_body, true);
      capture_outer_branch_state(else_state);
      rollback_transaction();
    }

    // Collect all variables that appear in then/else
    std::unordered_set<std::string> all_keys;
    for (auto &p : then_state)
      all_keys.insert(p.first);
    if (has_else)
      for (auto &p : else_state)
        all_keys.insert(p.first);

    // Merge branch states: use "or" logic (if moved or borrowed in any branch,
    // keep after merge)
    for (const auto &var_name : all_keys) {
      auto base_it = baseline.find(var_name);
      BranchState base_bs;
      if (base_it != baseline.end())
        base_bs = base_it->second;

      BranchState then_bs = base_bs;
      auto then_it = then_state.find(var_name);
      if (then_it != then_state.end())
        then_bs = then_it->second;

      BranchState else_bs = base_bs;
      if (has_else) {
        auto else_it = else_state.find(var_name);
        if (else_it != else_state.end())
          else_bs = else_it->second;
      }

      // Merge whole-struct borrow flags
      bool merged_moved =
          has_else ? (then_bs.moved || else_bs.moved) : then_bs.moved;
      bool merged_whole_shared = has_else
                                     ? (then_bs.whole_borrowed_shared ||
                                        else_bs.whole_borrowed_shared)
                                     : then_bs.whole_borrowed_shared;
      bool merged_whole_mut =
          has_else ? (then_bs.whole_borrowed_mut || else_bs.whole_borrowed_mut)
                   : then_bs.whole_borrowed_mut;

      // Merge borrow_sources
      auto merged_sources = then_bs.borrow_sources;
      if (has_else)
        merged_sources.insert(else_bs.borrow_sources.begin(),
                              else_bs.borrow_sources.end());

      // Merge field-level borrows
      auto merged_field_borrows = then_bs.field_borrows;
      if (has_else) {
        for (auto &[fname, fb] : else_bs.field_borrows) {
          auto &target = merged_field_borrows[fname];
          target.borrowed_shared =
              target.borrowed_shared || fb.borrowed_shared;
          target.borrowed_mut = target.borrowed_mut || fb.borrowed_mut;
        }
      }

      VarState *state = lookup(var_name);
      if (state) {
        state->moved = merged_moved;
        state->whole_borrowed_shared = merged_whole_shared;
        state->whole_borrowed_mut = merged_whole_mut;
        state->borrow_sources = std::move(merged_sources);
        state->field_borrows = std::move(merged_field_borrows);
        // If variable was moved and is non-Copy, clear borrow sources (can no
        // longer be borrowed)
        if (state->moved && !is_copy_type(state->type)) {
          state->borrow_sources.clear();
          state->field_borrows.clear();
        }
      }
    }
    return;
  }
  case AST_WHILE:
    check_expr(stmt->data.while_stmt.condition, ExprUse::Read);
    cleanup_borrows();
    check_block(stmt->data.while_stmt.body, true);
    return;
  case AST_FOR:
    check_expr(stmt->data.for_stmt.start, ExprUse::Read);
    check_expr(stmt->data.for_stmt.end, ExprUse::Read);
    cleanup_borrows();
    {
      push_scope();
      if (stmt->data.for_stmt.var &&
          stmt->data.for_stmt.var->type == AST_IDENTIFIER &&
          stmt->data.for_stmt.var->data.identifier.name) {
        declare_var(stmt->data.for_stmt.var->data.identifier.name,
                    stmt->data.for_stmt.var->inferred_type, false, false);
      }
      check_block(stmt->data.for_stmt.body, false);
      pop_scope();
      cleanup_borrows();
    }
    return;
  case AST_BREAK:
  case AST_CONTINUE:
    cleanup_borrows();
    return;
  default:
    check_expr(stmt, ExprUse::Read);
    cleanup_borrows();
    return;
  }
}

/**
 * @brief Check a code block
 * @param block code block node (could be AST_PROGRAM or a single statement)
 * @param new_scope whether it is a new scope
 */
void OwnershipChecker::check_block(ASTNode *block, bool new_scope) {
  if (!block)
    return;
  if (new_scope)
    push_scope();

  if (block->type == AST_PROGRAM) {
    for (int i = 0; i < block->data.program.statement_count; i++) {
      check_stmt(block->data.program.statements[i]);
    }
  } else {
    check_stmt(block);
  }

  if (new_scope) {
    pop_scope();
    cleanup_borrows();
  }
}

/**
 * @brief Check a function definition
 * @details Process parameter declarations and check the function body.
 *          Before checking, perform an NLL pre-scan to compute last-use
 *          lines for each variable in the function.
 */
void OwnershipChecker::check_function(ASTNode *fn) {
  if (!fn || fn->data.function.is_extern)
    return;

  // NLL pre-scan: compute last-use sequence for every variable in this function
  last_use_seq.clear();
  current_seq = 0;
  {
    int seq = 0;
    scan_last_use_seq(fn, seq, last_use_seq);
  }

  push_scope();
  ASTNode *params = fn->data.function.params;
  if (params && params->type == AST_EXPRESSION_LIST) {
    for (int i = 0; i < params->data.expression_list.expression_count; i++) {
      ASTNode *param = params->data.expression_list.expressions[i];
      if (!param)
        continue;
      ASTNode *id = param;
      const TypeInfo *type = param->inferred_type;
      bool mut = param->mutability == MUTABILITY_MUTABLE;
      if (param->type == AST_ASSIGN) { // Parameter with default value
        id = param->data.assign.left;
        type = param->inferred_type
                   ? param->inferred_type
                   : (param->data.assign.right
                          ? param->data.assign.right->inferred_type
                          : nullptr);
        mut = param->data.assign.mutability == MUTABILITY_MUTABLE ||
              (id && id->mutability == MUTABILITY_MUTABLE);
        if (param->data.assign.right) {
          push_scope();
          check_expr(param->data.assign.right, ExprUse::Read);
          pop_scope();
          cleanup_borrows();
        }
      }
      if (id && id->type == AST_IDENTIFIER && id->data.identifier.name) {
        declare_var(id->data.identifier.name, type, mut, false);
      }
    }
  }
  check_block(fn->data.function.body, false);
  pop_scope();
  cleanup_borrows();
  last_use_seq.clear(); // Clear after leaving the function
}

/**
 * @brief Check a global variable declaration
 */
void OwnershipChecker::check_global(ASTNode *node) {
  if (!node || !node->data.global_decl.identifier ||
      node->data.global_decl.identifier->type != AST_IDENTIFIER) {
    return;
  }
  ExprInfo init = check_expr(node->data.global_decl.initializer, ExprUse::Read);
  const char *name = node->data.global_decl.identifier->data.identifier.name;
  if (name)
    declare_var(name, node->inferred_type ? node->inferred_type : init.type,
                true, true, init.borrow_sources);
}

/**
 * @brief Check a return statement
 * @details Ensure the return value does not reference local variables (prevent
 * dangling references)
 */
void OwnershipChecker::check_return(ASTNode *node) {
  ExprInfo value = check_expr(node->data.return_stmt.expr, ExprUse::Move);
  for (const auto &src : value.borrow_sources) {
    if (is_local_name(src)) {
      report(node->data.return_stmt.expr ? node->data.return_stmt.expr : node,
             "cannot return reference to local variable '" + src + "'");
      break;
    }
  }
}

/**
 * @brief Check an assignment statement (including declarations)
 * @details Handle variable declarations, move semantics, and borrow checking.
 *
 * Features:
 *  - Dereference assignments (*ptr = ...) no longer mutate the pointer's
 *    ownership state (moved flag, borrow_sources).
 *  - Field-level assignments (p.x = val) only check the specific field
 *    for borrow conflicts, leaving other fields usable.
 *  - Full rebinding of a moved variable is allowed (reinitialization).
 */
void OwnershipChecker::check_assign(ASTNode *node) {
  ASTNode *left = node->data.assign.left;
  ASTNode *right = node->data.assign.right;
  bool is_decl = node->data.assign.is_declaration != 0;
  ExprInfo rhs = check_expr(right, ExprUse::Move);

  // Variable declaration
  if (is_decl && left && left->type == AST_IDENTIFIER &&
      left->data.identifier.name) {
    if (!rhs.borrow_sources.empty() &&
        rhs.borrow_sources.count(left->data.identifier.name)) {
      report(left, "cannot borrow value of variable being declared");
    }
    bool mut = left->mutability == MUTABILITY_MUTABLE ||
               node->data.assign.mutability == MUTABILITY_MUTABLE;
    const TypeInfo *type = right ? right->inferred_type : nullptr;
    if (!type || type->kind == TYPEINFO_VOID)
      type = rhs.type;
    declare_var(left->data.identifier.name, type ? type : rhs.type, mut,
                scopes.size() == 1, rhs.borrow_sources);
    return;
  }

  // Assignment to existing variable (or through a pointer)
  std::string base = lvalue_base(left);
  if (!base.empty()) {
    int deref_count = count_derefs(left);
    auto target_set = resolve_pointer_targets(base, deref_count);

    // Ambiguous assignment target (multiple possible lvalues) – always reject.
    if (target_set.size() > 1) {
      report(left,
             "cannot assign through pointer with multiple possible targets");
      return;
    }

    std::string target;
    if (!target_set.empty()) {
      target = *target_set.begin();
    } else {
      target = base;
    }

    VarState *target_state = lookup(target);
    if (target_state) {
      log_var_modification(target_state);

      // --- Determine field-level vs whole-variable write ---
      // Strip leading deref nodes to check if the lvalue is a member access
      // (this is hoisted before the is_indirect fork so BOTH paths can use it)
      ASTNode *lv = left;
      while (lv && lv->type == AST_UNARYOP &&
             lv->data.unaryop.op == OP_DEREF) {
        lv = lv->data.unaryop.expr;
      }
      bool is_field_assign = (lv && lv->type == AST_MEMBER_ACCESS);
      std::string assign_field = field_name(lv);

      // --- Distinguish direct assignment from indirect (dereference)
      // --- assignment ---
      bool is_indirect = (deref_count > 0);

      if (is_indirect) {
        // Indirect assignment: *ptr = expr, or @ptr.field = expr.
        // The pointer itself is only read; its ownership must NOT change.
        //
        // CRITICAL: whole_borrowed_mut is set BY the mutable borrow we are
        // now exercising — it is the grant of exclusive access, not a
        // prohibition.  Only whole_borrowed_shared blocks indirect writes
        // (writing through a shared borrow is illegal).
        if (target_state->moved) {
          report(left, "use of moved value '" + target + "'");
          return;
        }
        if (target_state->whole_borrowed_shared) {
          report(left, "cannot assign through '" + target +
                           "' while it is immutably borrowed");
          return;
        }
        // Field-level borrow check for writes through a pointer.
        if (is_field_assign && !assign_field.empty()) {
          auto fit = target_state->field_borrows.find(assign_field);
          if (fit != target_state->field_borrows.end()) {
            if (fit->second.borrowed_mut ||
                fit->second.borrowed_shared) {
              report(left, "cannot assign to '" + target + "." +
                               assign_field +
                               "' through pointer while it is borrowed");
              return;
            }
          }
        }
        // No state mutation for the pointer.
      } else {
        // Direct assignment to a variable or its field (e.g. x = ...,
        // x.field = ...)

        // --- Borrow conflict check ---
        if (is_field_assign && !assign_field.empty()) {
          // Field assignment: only check the specific field + whole-struct
          // borrows
          if (target_state->whole_borrowed_mut) {
            report(left, "cannot assign to '" + target + "." +
                             assign_field +
                             "' while it is mutably borrowed");
          } else if (target_state->whole_borrowed_shared) {
            report(left, "cannot assign to '" + target + "." +
                             assign_field +
                             "' while it is immutably borrowed");
          } else {
            auto fit = target_state->field_borrows.find(assign_field);
            if (fit != target_state->field_borrows.end()) {
              if (fit->second.borrowed_mut ||
                  fit->second.borrowed_shared) {
                report(left,
                       "cannot assign to '" + target + "." + assign_field +
                           "' while it is borrowed");
              }
            }
          }
        } else {
          // Whole-variable assignment: check whole-struct AND any field
          // borrows
          if (target_state->whole_borrowed_shared ||
              target_state->whole_borrowed_mut) {
            report(left, "cannot assign to '" + target +
                             "' while it is borrowed");
          }
          for (auto &[fname, fb] : target_state->field_borrows) {
            if (fb.borrowed_shared || fb.borrowed_mut) {
              report(left, "cannot assign to '" + target +
                               "' while its fields are borrowed");
              break;
            }
          }
        }

        // Determine if partial assignment (field / element of a struct/array)
        bool is_partial = is_field_assign || (lv && lv->type == AST_INDEX);

        // Handle moved state: full rebinding allowed, partial on moved is error
        if (target_state->moved) {
          if (is_partial) {
            report(left, "use of moved value '" + target + "'");
            return; // Do not modify state
          }
          // else: full rebinding of a moved variable is allowed
          // (reinitialization)
        }

        // Update ownership: rebinding clears moved, replaces borrow_sources
        target_state->moved = false;
        if (is_partial) {
          target_state->borrow_sources.insert(rhs.borrow_sources.begin(),
                                              rhs.borrow_sources.end());
        } else {
          target_state->borrow_sources = rhs.borrow_sources;
        }
      }
    }
  }

  // Validate the left-hand side expression path
  check_expr(left, ExprUse::AssignTarget);
}

/**
 * @brief Check an expression, returning expression information
 */
ExprInfo OwnershipChecker::check_expr(ASTNode *node, ExprUse use) {
  ExprInfo info;
  if (!node)
    return info;
  info.type = node->inferred_type;
  info.copy = is_copy_type(node->inferred_type);

  switch (node->type) {
  case AST_IDENTIFIER:
    return check_identifier(node, use);
  case AST_NUM_INT:
  case AST_NUM_FLOAT:
  case AST_CHAR:
  case AST_NIL:
    info.copy = true;
    return info;
  case AST_STRING:
    info.copy = false;
    return info;
  case AST_UNARYOP:
    return check_unary(node);
  case AST_BINOP:
    check_expr(node->data.binop.left, ExprUse::Read);
    check_expr(node->data.binop.right, ExprUse::Read);
    return info;
  case AST_CALL:
    return check_call(node);
  case AST_EXPRESSION_LIST:
    return check_expression_list(node, use);
  case AST_INDEX: {
    ExprInfo target = check_expr(node->data.index.target, ExprUse::Read);
    check_expr(node->data.index.index, ExprUse::Read);
    info.borrow_sources = target.borrow_sources;
    return info;
  }
  case AST_MEMBER_ACCESS: {
    // Check the object with relaxed ReadField context
    ExprInfo obj =
        check_expr(node->data.member_access.object, ExprUse::ReadField);
    std::string fname = field_name(node);
    if (!fname.empty()) {
      // Look up the underlying base variable to check field-level borrows
      std::string base = lvalue_base(node);
      VarState *state = lookup(base);
      if (state) {
        auto fit = state->field_borrows.find(fname);
        if (fit != state->field_borrows.end()) {
          if (fit->second.borrowed_mut) {
            report(node, "cannot use '" + base + "." + fname +
                             "' while it is mutably borrowed");
          }
          // Note: shared borrow on the field is OK for reading (checked on
          // write in check_assign)
        }
      }
      info.borrow_sources = obj.borrow_sources;
    } else {
      info.borrow_sources = obj.borrow_sources;
    }
    return info;
  }
  case AST_STRUCT_LITERAL:
    return check_struct_literal(node);
  case AST_IF:
    check_stmt(node); // if expression handled as statement
    return info;
  case AST_FUNCTION:
    check_function(node);
    info.copy = true;
    return info;
  case AST_TOINT:
    check_expr(node->data.toint.expr, ExprUse::Read);
    return info;
  case AST_TOFLOAT:
    check_expr(node->data.tofloat.expr, ExprUse::Read);
    return info;
  case AST_INPUT:
    check_expr(node->data.input.prompt, ExprUse::Read);
    return info;
  default:
    return info;
  }
}

/**
 * @brief Check an identifier expression
 * @details Handle variable read, move, borrow semantics.
 *
 * **Field-level precision**: When called with `ExprUse::ReadField` (i.e., the
 * identifier is the object of a member access), only `whole_borrowed_mut` and
 * `moved` are checked. Per-field borrows are validated at the
 * `AST_MEMBER_ACCESS` level.
 *
 * **NLL**: A mutably borrowed variable cannot be used in any non‑assignment
 * context. Moves are rejected while any borrow (shared or mutable) is active.
 */
ExprInfo OwnershipChecker::check_identifier(ASTNode *node, ExprUse use) {
  ExprInfo info;
  info.type = node->inferred_type;
  info.copy = is_copy_type(node->inferred_type);
  const char *cname = node->data.identifier.name;
  if (!cname)
    return info;
  VarState *state = lookup(cname);
  if (!state)
    return info;
  info.type = state->type ? state->type : node->inferred_type;
  info.copy = is_copy_type(info.type);
  // Convention: names starting with uppercase are considered constants/types,
  // default copyable (simplified handling)
  if (std::isupper(static_cast<unsigned char>(cname[0])))
    info.copy = true;
  info.borrow_sources = state->borrow_sources;

  // --- Borrow conflict checks ---

  if (use == ExprUse::ReadField) {
    // Field access context: only whole-struct mutable borrow and moved state
    // block the access. Per-field borrows are checked at the MEMBER_ACCESS level.
    if (state->whole_borrowed_mut) {
      report(node, "cannot use '" + std::string(cname) +
                       "' while it is mutably borrowed");
      return info;
    }
  } else if (use != ExprUse::AssignTarget && use != ExprUse::Address) {
    // Full variable access (Read, Move): check whole-struct mutable
    // borrow and field-level borrows.
    // Note: ExprUse::Address is excluded because borrow conflicts are already
    // checked in check_unary().
    if (state->whole_borrowed_mut) {
      report(node, "cannot use '" + std::string(cname) +
                       "' while it is mutably borrowed");
      return info;
    }
    // If any fields are borrowed, the whole variable cannot be used directly
    if (has_active_field_borrows(*state)) {
      report(node, "cannot use '" + std::string(cname) +
                       "' while its fields are borrowed");
      return info;
    }
  }

  // Check if using a moved value (unless as assignment target)
  if (use != ExprUse::AssignTarget && state->moved) {
    std::string msg = std::string("use of moved value '") + cname + "'";
    if (state->moved_at) {
      msg += "\n  note: '" + std::string(cname) +
             "' was moved here by passing to " +
             std::string(node_file(state->moved_at)) + ":" +
             std::to_string(node_line(state->moved_at));
    }
    adjust_column_to_identifier(cname);
    report(node, msg);
    return info;
  }
  // Move semantics: if non-Copy type used with Move, mark as moved
  if (use == ExprUse::Move && !info.copy) {
    log_var_modification(state);
    if (state->whole_borrowed_shared || state->whole_borrowed_mut ||
        has_active_field_borrows(*state)) {
      report(node, std::string("cannot move '") + cname +
                       "' while it is borrowed");
    } else {
      state->moved = true;
      state->moved_at = node;
      state->borrow_sources.clear(); // After move borrow relationships disappear
      state->field_borrows.clear();
    }
  }
  return info;
}

/**
 * @brief Check a unary operation
 * @details Focus on handling borrow rules for take-address (&) and dereference
 * (*).
 *
 * **Field-level borrows**: `ref p.x` marks only `field_borrows["x"]`, not the
 * whole struct. `ref p` marks the whole struct. Conflicts are checked
 * accordingly: a whole-struct mutable borrow conflicts with any field borrow;
 * a field-level borrow only conflicts with the same field.
 */
ExprInfo OwnershipChecker::check_unary(ASTNode *node) {
  ExprInfo info;
  info.type = node->inferred_type;
  info.copy = is_copy_type(node->inferred_type);
  UnaryOpType op = node->data.unaryop.op;
  if (op == OP_ADDRESS) {
    // Take address operation: check if borrowable
    //
    // Order of operations (critical for correctness):
    //   1. Resolve root variable(s) from the operand
    //   2. Check for conflicts (moved, existing borrows) — READ-ONLY, no flags
    //   3. Call check_expr(operand, Address) to validate the inner expression
    //      — must happen BEFORE flags are set, so check_identifier on the
    //         operand doesn't see freshly-set borrow flags
    //   4. Apply borrow flags on the root(s) (only if no error)
    std::string base = lvalue_base(node->data.unaryop.expr);
    if (!base.empty()) {
      std::unordered_set<std::string> roots;
      bool is_deref_operand =
          (node->data.unaryop.expr &&
           node->data.unaryop.expr->type == AST_UNARYOP &&
           node->data.unaryop.expr->data.unaryop.op == OP_DEREF);
      if (is_deref_operand) {
        roots = resolve_pointer_targets(base, -1);
      } else {
        roots = {base};
      }

      // --- Step 1: conflict checking (read-only) ---
      bool error_reported = false;
      bool wants_mut = node->mutability == MUTABILITY_MUTABLE;
      std::string field_borrow_name; // non-empty = field-level borrow

      for (const auto &root : roots) {
        VarState *root_state = lookup(root);
        if (!root_state)
          continue;
        log_var_modification(root_state);

        if (root_state->moved) {
          if (!error_reported) {
            report(node, "cannot borrow moved value '" + root + "'");
            error_reported = true;
          }
          continue;
        }

        std::string fname = field_name(node->data.unaryop.expr);
        bool is_field_borrow =
            !is_deref_operand && !fname.empty() &&
            node->data.unaryop.expr->type == AST_MEMBER_ACCESS;

        if (is_field_borrow) {
          field_borrow_name = fname;
          auto &fb = root_state->field_borrows[fname];
          if (wants_mut) {
            if (root_state->whole_borrowed_shared ||
                root_state->whole_borrowed_mut ||
                fb.borrowed_shared || fb.borrowed_mut) {
              if (!error_reported) {
                report(node, "cannot mutably borrow '" + root + "." + fname +
                                 "' more than once");
                error_reported = true;
              }
            }
          } else {
            if (root_state->whole_borrowed_mut || fb.borrowed_mut) {
              if (!error_reported) {
                report(node, "cannot immutably borrow '" + root + "." +
                                 fname +
                                 "' while it is mutably borrowed");
                error_reported = true;
              }
            }
          }
        } else {
          if (wants_mut) {
            if (root_state->whole_borrowed_shared ||
                root_state->whole_borrowed_mut ||
                has_active_field_borrows(*root_state)) {
              if (!error_reported) {
                report(node, "cannot mutably borrow '" + root +
                                 "' more than once");
                error_reported = true;
              }
            }
          } else {
            if (root_state->whole_borrowed_mut) {
              if (!error_reported) {
                report(node, "cannot immutably borrow '" + root +
                                 "' while it is mutably borrowed");
                error_reported = true;
              }
            }
          }
        }
      }

      // --- Step 2: validate the inner expression ---
      check_expr(node->data.unaryop.expr, ExprUse::Address);

      // --- Step 3: apply borrow flags (only if no error) ---
      if (!error_reported) {
        for (const auto &root : roots) {
          VarState *root_state = lookup(root);
          if (!root_state)
            continue;
          if (!field_borrow_name.empty()) {
            auto &fb = root_state->field_borrows[field_borrow_name];
            if (wants_mut)
              fb.borrowed_mut = true;
            else
              fb.borrowed_shared = true;
          } else {
            if (wants_mut)
              root_state->whole_borrowed_mut = true;
            else
              root_state->whole_borrowed_shared = true;
          }
        }
        info.borrow_sources = roots;
      }
    }
    info.is_ref = true;
    info.copy = true;
    return info;
  }
  if (op == OP_DEREF) {
    // Dereference: do NOT propagate borrow_sources from the inner expression.
    // Dereferencing produces a value, not a borrow — the borrow chain is
    // already tracked through the pointer variable's VarState directly
    // (used by resolve_pointer_targets() for pointer chasing).
    check_expr(node->data.unaryop.expr, ExprUse::Read);
    info.copy = is_copy_type(node->inferred_type);
    return info;
  }
  check_expr(node->data.unaryop.expr, ExprUse::Read);
  return info;
}

/**
 * @brief Check a function call
 * @details Handle argument passing (move semantics); if the function returns a
 * reference, collect borrow sources
 */
ExprInfo OwnershipChecker::check_call(ASTNode *node) {
  ExprInfo info;
  info.type = node->inferred_type;
  info.copy = is_copy_type(node->inferred_type);
  if (node->data.call.func && node->data.call.func->type == AST_IDENTIFIER &&
      node->data.call.func->data.identifier.name &&
      std::string(node->data.call.func->data.identifier.name) == VIX_STRING_SLICE_INTRINSIC) {
    ASTNode *args = node->data.call.args;
    if (args && args->type == AST_EXPRESSION_LIST) {
      for (int i = 0; i < args->data.expression_list.expression_count; i++) {
        check_expr(args->data.expression_list.expressions[i], ExprUse::Read);
      }
    }
    return info;
  }
  check_expr(node->data.call.func, ExprUse::Read);
  ASTNode *args = node->data.call.args;
  bool returns_ref =
      node->inferred_type && node->inferred_type->kind == TYPEINFO_PTR;
  if (args && args->type == AST_EXPRESSION_LIST) {
    for (int i = 0; i < args->data.expression_list.expression_count; i++) {
      ExprInfo arg_info =
          check_expr(args->data.expression_list.expressions[i], ExprUse::Move);
      if (returns_ref) {
        info.borrow_sources.insert(arg_info.borrow_sources.begin(),
                                   arg_info.borrow_sources.end());
      }
    }
  }
  return info;
}

/**
 * @brief Check an expression list (e.g., argument list, struct field list)
 */
ExprInfo OwnershipChecker::check_expression_list(ASTNode *node, ExprUse use) {
  ExprInfo info;
  info.type = node->inferred_type;
  info.copy = is_copy_type(node->inferred_type);
  for (int i = 0; i < node->data.expression_list.expression_count; i++) {
    ExprInfo elem = check_expr(node->data.expression_list.expressions[i], use);
    info.borrow_sources.insert(elem.borrow_sources.begin(),
                               elem.borrow_sources.end());
  }
  return info;
}

/**
 * @brief Check a struct literal
 * @details Each field is processed as move, collecting all borrow sources
 */
ExprInfo OwnershipChecker::check_struct_literal(ASTNode *node) {
  ExprInfo info;
  info.type = node->inferred_type;
  info.copy = false;
  ASTNode *fields = node->data.struct_literal.fields;
  if (fields && fields->type == AST_EXPRESSION_LIST) {
    for (int i = 0; i < fields->data.expression_list.expression_count; i++) {
      ASTNode *field = fields->data.expression_list.expressions[i];
      ExprInfo elem;
      if (field && field->type == AST_ASSIGN) {
        elem = check_expr(field->data.assign.right, ExprUse::Move);
      } else {
        elem = check_expr(field, ExprUse::Move);
      }
      info.borrow_sources.insert(elem.borrow_sources.begin(),
                                 elem.borrow_sources.end());
    }
  }
  return info;
}

/**
 * @brief Compute the number of dereference levels of an expression
 */
int OwnershipChecker::count_derefs(ASTNode *node) {
  int count = 0;
  while (node && node->type == AST_UNARYOP &&
         node->data.unaryop.op == OP_DEREF) {
    ++count;
    node = node->data.unaryop.expr;
  }
  return count;
}

/**
 * @brief Extract the underlying variable name of an expression
 */
std::string OwnershipChecker::lvalue_base(ASTNode *node) {
  if (!node)
    return {};
  switch (node->type) {
  case AST_IDENTIFIER:
    return node->data.identifier.name ? node->data.identifier.name : "";
  case AST_MEMBER_ACCESS:
    return lvalue_base(node->data.member_access.object);
  case AST_INDEX:
    return lvalue_base(node->data.index.target);
  case AST_UNARYOP:
    if (node->data.unaryop.op == OP_DEREF)
      return lvalue_base(node->data.unaryop.expr);
    return {};
  default:
    return {};
  }
}

/**
 * @brief Traverse the AST root node
 */
void OwnershipChecker::check_node(ASTNode *node) {
  if (!node)
    return;
  switch (node->type) {
  case AST_PROGRAM:
    // NLL pre-scan for top-level statements (globals, top-level expressions)
    last_use_seq.clear();
    current_seq = 0;
    {
      int seq = 0;
      scan_last_use_seq(node, seq, last_use_seq);
    }
    check_block(node, false);
    cleanup_borrows();
    last_use_seq.clear();
    break;
  case AST_FUNCTION:
    check_function(node);
    break;
  case AST_STRUCT_DEF:
  case AST_IMPORT:
    break;
  case AST_GLOBAL:
    check_global(node);
    cleanup_borrows();
    break;
  default:
    check_stmt(node);
    break;
  }
}

} // namespace

/**
 * @brief C interface: exposed ownership checking entry point
 * @param root AST root node
 * @return 0 success, 1 failure
 */
extern "C" int ownership_check_program(ASTNode *root) {
  OwnershipChecker checker;
  return checker.check(root) == 0 ? 0 : 1;
}
