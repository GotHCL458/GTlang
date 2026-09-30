//! 所有权与借用检查（**流敏感**，NLL 风格）。
//!
//! 与"实用子集"的区别：
//!   - **流敏感**：沿控制流传播「移动状态」与「活跃借用」，合并点保守取交集；
//!   - **借用区间（NLL 核心）**：借用在其**最后一次使用**后即失效，
//!     而非持续到作用域结束——因此 `let r = &x; ...使用 r...; x = 1;` 合法；
//!   - **精确冲突**：move 已借用值 / 借用已 move 值 / `&mut` 与其它借用并存，
//!     均报错；
//!   - **循环安全**：循环体内产生、循环体外的借用视为错误（借用逃逸）。
//!
//! 检查在 sema 之后、代码生成之前运行，仅产出诊断，不改变代码生成。

use std::collections::{HashMap, HashSet};

use crate::ast::*;

/// 是否 **Copy**（传递时不转移所有权）：
///   - 标量（int/f64/bool/void）与借用 → Copy；
///   - **容器（list/set/map）是引用语义**（句柄共享）→ 视为 Copy，不触发 move；
///   - `str` / `struct` / 定长数组 / 闭包 → **非 Copy**（值语义，传递即 move）。
fn is_copy(t: &Ty) -> bool {
    matches!(
        t,
        Ty::I64 | Ty::F64 | Ty::Bool | Ty::Void
        | Ty::Ref(_) | Ty::RefMut(_) | Ty::Unknown
        | Ty::List(_) | Ty::Set(_) | Ty::Map(_, _)
    )
}

/// 一个借用的记录。
#[derive(Clone, PartialEq, Eq)]
struct Loan {
    /// 被借用的变量
    var: String,
    /// 借用的具体字段（`&p.x` 的 `x`）；None 表示整个变量
    field: Option<String>,
    /// 是否 `&mut`（独占）
    mutable: bool,
    /// 触发点（源码行，用于诊断）
    line: usize,
    /// 持有该借用的引用变量名（`r := &x` 中的 `r`）；None 表示临时借用（随用随止）。
    holder: Option<String>,
}

/// 某个程序点的所有权状态。
#[derive(Clone)]
struct State {
    /// 已 move（失效）的变量
    moved: HashSet<String>,
    /// 当前活跃的借用（去重集合）
    loans: Vec<Loan>,
}

impl State {
    fn empty() -> State {
        State { moved: HashSet::new(), loans: Vec::new() }
    }
    /// 合并两个分支状态：保守地取**并集**（只要某分支 move/借用，合并后就视为已发生）。
    fn join(&self, other: &State) -> State {
        let mut moved = self.moved.clone();
        moved.extend(other.moved.iter().cloned());
        let mut loans = self.loans.clone();
        for l in &other.loans {
            if !loans.iter().any(|x| x.var == l.var && x.field == l.field && x.mutable == l.mutable) {
                loans.push(l.clone());
            }
        }
        State { moved, loans }
    }
}

struct Ctx<'a> {
    /// 变量类型表（名字 → Ty），由 sema 回填 + 参数收集
    types: &'a HashMap<String, Ty>,
    errors: Vec<String>,
}

/// 对整份程序做所有权检查；返回诊断（空 = 通过）。
pub fn check(prog: &Program) -> Vec<String> {
    // 收集全程序变量类型（顶层 + 函数形参），供 is_copy 判断
    let mut types: HashMap<String, Ty> = HashMap::new();
    for item in &prog.items {
        if let Item::Fn(f) = item {
            for p in &f.params {
                types.insert(p.name.clone(), p.ty.clone().unwrap_or(Ty::Unknown));
            }
        }
    }

    let mut errors = Vec::new();
    for item in &prog.items {
        match item {
            Item::Fn(f) => {
                let mut ctx = Ctx { types: &types, errors: Vec::new() };
                ctx.check_block(&f.body, &State::empty(), 0);
                errors.extend(ctx.errors.drain(..));
            }
            Item::Impl { methods, .. } | Item::TraitImpl { methods, .. } => {
                for m in methods {
                    let mut local = types.clone();
                    for p in &m.params {
                        local.insert(p.name.clone(), p.ty.clone().unwrap_or(Ty::Unknown));
                    }
                    let mut ctx = Ctx { types: &local, errors: Vec::new() };
                    ctx.check_block(&m.body, &State::empty(), 0);
                    errors.extend(ctx.errors.drain(..));
                }
            }
            _ => {}
        }
    }
    errors
}

impl<'a> Ctx<'a> {
    /// 带参数的诊断（中英双语）：`{}` 占位被 `arg` 替换。
    fn errf(&mut self, line: usize, en: &str, zh: &str, arg: &str) {
        let msg = crate::lang::tr(en, zh).replace("{}", arg);
        self.errors.push(crate::lb!(line, "{}", "{}", msg));
    }

    /// 检查一个块；返回「块结束后」的状态。`depth` 是循环嵌套深度（>0 表示在循环内）。
    ///
    /// 借用生命周期（NLL 核心）：先扫描本块，算出每个**持有借用的变量**最后一次被使用
    /// 的语句下标；某个借用在「其持有者的最后一次使用」之后即视为失效，不再参与冲突检查。
    fn check_block(&mut self, b: &Block, st: &State, depth: usize) -> State {
        // 预扫描：ref_var -> 该引用变量的最后一次使用下标
        let mut last_use: HashMap<String, usize> = HashMap::new();
        for (i, s) in b.iter().enumerate() {
            let mut used = Vec::new();
            collect_used(s, &mut used);
            for u in used {
                last_use.insert(u, i);
            }
        }
        let mut cur = st.clone();
        let mut terminated = false;
        for (i, s) in b.iter().enumerate() {
            // return/break/continue 之后的语句不可达，不再检查（避免误报）
            if terminated { break; }
            // 让「持有者已到最后使用点」的借用失效（NLL：借用止于最后一次使用）
            cur.loans.retain(|l| match &l.holder {
                Some(h) => last_use.get(h).map(|&lu| i <= lu).unwrap_or(false),
                None => true,
            });
            cur = self.check_stmt(s, &cur, depth);
            terminated = matches!(s, Stmt::Return(..) | Stmt::Break(..) | Stmt::Continue(..));
        }
        cur
    }

    fn check_stmt(&mut self, s: &Stmt, st: &State, depth: usize) -> State {
        let mut cur = st.clone();
        match s {
            Stmt::Labeled { inner, .. } => {
                let inner_blk: Block = vec![(**inner).clone()];
                cur = self.check_block(&inner_blk, &cur, depth);
            }
            Stmt::Throw(e, _) => {
                self.use_expr(e, st, depth);
            }
            Stmt::Asm { .. } => {}
            Stmt::Go { args, .. } => for a in args.iter() { self.use_expr(a, st, depth); },
            Stmt::Try { body, catches, fin, .. } => {
                let after = self.check_block(body, st, depth);
                for ca in catches {
                    let mut s2 = st.clone();
                    s2.moved = after.moved.clone();
                    if let Some(g) = &ca.guard { self.use_expr(g, &s2, depth); }
                    let _ = self.check_block(&ca.body, &s2, depth);
                }
                if let Some(f) = fin { let _ = self.check_block(f, &after, depth); }
                cur = after;
            }
            Stmt::Let { name, value, .. } | Stmt::Const { name, value, .. } => {
                self.use_expr(value, &cur, depth);
                cur = self.after_value_use(value, &cur);
                // `r := &x`：把新创建的借用绑定到持有者 r
                if let ExprKind::Borrow { mutable, inner } = &value.kind {
                    match &inner.kind {
                        ExprKind::Ident(v) => {
                            cur.loans.push(Loan { var: v.clone(), field: None, mutable: *mutable, line: value.line, holder: Some(name.clone()) });
                        }
                        ExprKind::Field(base, fname) => {
                            if let ExprKind::Ident(v) = &base.kind {
                                cur.loans.push(Loan { var: v.clone(), field: Some(fname.clone()), mutable: *mutable, line: value.line, holder: Some(name.clone()) });
                            }
                        }
                        _ => {}
                    }
                }
                cur.moved.remove(name);
            }
            Stmt::Assign { name, index, value, .. } => {
                // 若目标已被借用，禁止赋值
                self.forbid_borrowed_write(name, stmt_line(s), &cur);
                if let Some(ix) = index {
                    self.use_expr(ix, &cur, depth);
                }
                self.use_expr(value, &cur, depth);
                cur = self.after_value_use(value, &cur);
                cur.moved.remove(name);
            }
            Stmt::FieldAssign { obj, field, value, .. } => {
                self.forbid_borrowed_field_write(obj, field, stmt_line(s), &cur);
                self.use_expr(value, &cur, depth);
                cur = self.after_value_use(value, &cur);
            }
            Stmt::Expr(e) => {
                self.use_expr(e, &cur, depth);
                cur = self.after_value_use(e, &cur);
            }
            Stmt::Return(Some(e), _) => {
                self.use_expr(e, &cur, depth);
                cur = self.after_value_use(e, &cur);
            }
            Stmt::Return(None, _) => {}
            Stmt::If { cond, then, els, .. } => {
                self.use_expr(cond, &cur, depth);
                let pre = self.after_value_use(cond, &cur);
                let after_then = self.check_block(then, &pre, depth);
                let after_els = match els {
                    Some(e) => self.check_block(e, &pre, depth),
                    None => pre.clone(),
                };
                cur = after_then.join(&after_els);
            }
            Stmt::While { cond, body, .. } => {
                self.use_expr(cond, &cur, depth);
                let pre = self.after_value_use(cond, &cur);
                // 循环体可能执行 0 次：体外状态 = 体前 ∪ 体后
                let after_body = self.check_block(body, &pre, depth + 1);
                cur = pre.join(&after_body);
            }
            Stmt::DoWhile { body, cond, .. } => {
                // body 至少执行一次
                let after_body = self.check_block(body, &cur, depth + 1);
                self.use_expr(cond, &after_body, depth);
                cur = after_body;
            }
            Stmt::ForRange { from, to, body, .. } => {
                self.use_expr(from, &cur, depth);
                self.use_expr(to, &cur, depth);
                let after_from = self.after_value_use(from, &cur);
                let pre = self.after_value_use(to, &after_from);
                let after_body = self.check_block(body, &pre, depth + 1);
                cur = pre.join(&after_body);
            }
            Stmt::ForEach { iter, body, .. } => {
                // 遍历是**借用**：不 move 被遍历的值
                self.use_expr(iter, &cur, depth);
                let after_body = self.check_block(body, &cur, depth + 1);
                cur = cur.join(&after_body);
            }
            Stmt::Block(inner) => {
                cur = self.check_block(inner, &cur, depth);
            }
            Stmt::Break(..) | Stmt::Continue(..) | Stmt::LocalFn(_) => {}
        }
        cur
    }

    /// 若 `name`（或其字段）当前被借用，则禁止写它。
    fn forbid_borrowed_write(&mut self, name: &str, line: usize, st: &State) {
        if st.loans.iter().any(|l| l.var == name) {
            self.errf(line,
                "cannot assign to '{}' because it is borrowed",
                "不能给 '{}' 赋值：它正被借用", name);
        }
    }

    /// 若 `name.field` 当前被借用，则禁止写该字段。
    fn forbid_borrowed_field_write(&mut self, name: &str, field: &str, line: usize, st: &State) {
        let probe = Loan { var: name.to_string(), field: Some(field.to_string()), mutable: true, line: 0, holder: None };
        let what = format!("{}.{}", name, field);
        if st.loans.iter().any(|l| loans_conflict(l, &probe)) {
            self.errf(line,
                "cannot assign to '{}' because it is borrowed",
                "不能给 '{}' 赋值：它正被借用", &what);
        }
    }

    /// 遍历表达式，报告已移动值的使用、借用冲突、以及表达式内的「按值使用」移动。
    fn use_expr(&mut self, e: &Expr, st: &State, depth: usize) {
        match &e.kind {
            ExprKind::CallNamed(_, named) => { for (_, v) in named.iter() { self.use_expr(v, st, depth); } }
            ExprKind::TupleLit(items) => for v in items.iter() { self.use_expr(v, st, depth); },
            ExprKind::Slice(b, lo, hi) => { self.use_expr(b, st, depth); self.use_expr(lo, st, depth); self.use_expr(hi, st, depth); },
            ExprKind::ListComp { expr, iter, cond, .. } => { self.use_expr(expr, st, depth); self.use_expr(iter, st, depth); if let Some(c) = cond { self.use_expr(c, st, depth); } },
            ExprKind::EnumLit(_, _, args) => for a in args.iter() { self.use_expr(a, st, depth); },
            ExprKind::DynBox { value, .. } => self.use_expr(value, st, depth),
            ExprKind::Ident(n) => {
                if st.moved.contains(n) {
                    self.errf(e.line, "use of moved value '{}'", "使用了已移动的值 '{}'", n);
                }
            }
            ExprKind::Borrow { mutable, inner } => {
                // 目标：整变量或某字段（字段级借用精度）
                let (base, field) = match &inner.kind {
                    ExprKind::Ident(n) => (Some(n.clone()), None),
                    ExprKind::Field(b, fname) => match &b.kind {
                        ExprKind::Ident(n) => (Some(n.clone()), Some(fname.clone())),
                        _ => (None, None),
                    },
                    _ => (None, None),
                };
                if let Some(n) = &base {
                    if st.moved.contains(n) {
                        self.errf(e.line, "cannot borrow moved value '{}'", "不能借用已移动的值 '{}'", n);
                    }
                    let probe = Loan { var: n.clone(), field: field.clone(), mutable: *mutable, line: e.line, holder: None };
                    let conflict = st.loans.iter().any(|l| {
                        if !loans_conflict(l, &probe) { return false; }
                        // & 与 &（同字段/整变量）共享可共存；只有涉及 &mut 才冲突
                        if !*mutable && !l.mutable { return false; }
                        true
                    });
                    if conflict {
                        let what = match &field { Some(f) => format!("{}.{}", n, f), None => n.clone() };
                        if *mutable {
                            self.errf(e.line,
                                "cannot borrow '{}' as mutable: already borrowed",
                                "不能对 '{}' 做 &mut 借用：它已被借用", &what);
                        } else {
                            self.errf(e.line,
                                "cannot borrow '{}' as shared: already mutably borrowed",
                                "不能对 '{}' 做 & 借用：它已被 &mut 独占借用", &what);
                        }
                    }
                } else {
                    // 借用复杂左值（如 a[i]）：检查基座是否已 move
                    self.use_expr(inner, st, depth);
                }
            }
            ExprKind::Unary(_, a) => self.use_expr(a, st, depth),
            ExprKind::Binary(_, a, b) => { self.use_expr(a, st, depth); self.use_expr(b, st, depth); }
            ExprKind::Call(_, args) => for a in args { self.use_expr(a, st, depth); },
            ExprKind::CallValue { callee, args } => {
                self.use_expr(callee, st, depth);
                for a in args { self.use_expr(a, st, depth); }
            }
            ExprKind::Index(a, b) => { self.use_expr(a, st, depth); self.use_expr(b, st, depth); }
            ExprKind::ArrayLit(items) => for a in items { self.use_expr(a, st, depth); },
            ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { self.use_expr(i, st, depth); } },
            ExprKind::If { cond, then, els } => {
                self.use_expr(cond, st, depth);
                let pre = self.after_value_use(cond, st);
                self.check_block(then, &pre, depth);
                if let Some(e) = els { self.check_block(e, &pre, depth); }
            }
            ExprKind::Match { subject, arms } => {
                self.use_expr(subject, st, depth);
                let pre = self.after_value_use(subject, st);
                for arm in arms {
                    if let Some(p) = &arm.pat { self.use_expr(p, &pre, depth); }
                    if let Some(g) = &arm.guard { self.use_expr(g, &pre, depth); }
                    self.check_block(&arm.body, &pre, depth);
                }
            }
            ExprKind::Field(base, _) => self.use_expr(base, st, depth),
            ExprKind::StructLit(_, fields) => for (_, v) in fields { self.use_expr(v, st, depth); },
            ExprKind::Closure { body, .. } => self.use_expr(body, st, depth),
            ExprKind::ClosureNew { captures, .. } => {
                // 闭包按值捕获 → 视为移动这些变量
                for c in captures { self.use_expr(c, st, depth); }
            }
            ExprKind::Ok(inner) | ExprKind::Err(inner) | ExprKind::Some(inner) | ExprKind::Try(inner) => {
                self.use_expr(inner, st, depth);
            }
            ExprKind::None => {}
            ExprKind::TryOr { inner, default } => {
                self.use_expr(inner, st, depth);
                self.use_expr(default, st, depth);
            }
            ExprKind::TryBlock { body, catches, fin } => {
                let _ = self.check_block(body, st, depth);
                for ca in catches {
                    if let Some(g) = &ca.guard { self.use_expr(g, st, depth); }
                    let _ = self.check_block(&ca.body, st, depth);
                }
                if let Some(f) = fin { let _ = self.check_block(f, st, depth); }
            }
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) => {}
        }
    }

    /// 计算「按值使用表达式 `e` 之后」的状态：
    ///   - 顶层 Ident 且非 Copy → move 它；
    ///   - 顶层 Borrow → 新增一个借用；
    ///   - 其它 → 递归处理子表达式的按值使用。
    fn after_value_use(&mut self, e: &Expr, st: &State) -> State {
        let mut s = st.clone();
        match &e.kind {
            ExprKind::Ident(n) => {
                // 优先用 sema 回填的表达式类型；其次查变量类型表。
                let ty = if e.ty != Ty::Unknown {
                    e.ty.clone()
                } else {
                    self.types.get(n).cloned().unwrap_or(Ty::Unknown)
                };
                if !is_copy(&ty) {
                    // move 一个正被借用的值 → 错误（借用未失效）
                    if s.loans.iter().any(|l| l.var == *n) {
                        self.errf(e.line,
                            "cannot move '{}' because it is borrowed",
                            "不能移动 '{}'：它正被借用", n);
                    }
                    s.moved.insert(n.clone());
                }
            }
            // 临时借用（`f(&x)`）随语句结束即失效，不持久化；冲突在 use_expr 中检查。
            ExprKind::Borrow { .. } => {}
            // 运算/比较/索引/字段/调用 的操作数都视为**读取**，不 move
            // （GTLang 的按值 move 只发生在"赋值绑定"这类显式转移场景）
            ExprKind::Binary(..) | ExprKind::Index(..) | ExprKind::Unary(..) | ExprKind::Field(..)
            | ExprKind::Call(..) | ExprKind::CallValue { .. } => {}
            ExprKind::StructLit(_, fields) => { for (_, v) in fields { s = self.after_value_use(v, &s); } }
            ExprKind::ArrayLit(items) => { for it in items { s = self.after_value_use(it, &s); } }
            ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { s = self.after_value_use(i, &s); } },
            ExprKind::ClosureNew { captures, .. } => { for c in captures { s = self.after_value_use(c, &s); } }
            _ => {}
        }
        s
    }
}

/// 收集一条语句中**使用到**的变量名（用于 NLL 的最后使用点分析）。
fn collect_used(s: &Stmt, out: &mut Vec<String>) {
    match s {
        Stmt::Let { value, .. } | Stmt::Const { value, .. } => collect_used_expr(value, out),
        Stmt::Assign { name, index, value, .. } => {
            out.push(name.clone());
            if let Some(ix) = index { collect_used_expr(ix, out); }
            collect_used_expr(value, out);
        }
        Stmt::FieldAssign { obj, value, .. } => { out.push(obj.clone()); collect_used_expr(value, out); }
        Stmt::Expr(e) => collect_used_expr(e, out),
        Stmt::Return(Some(e), _) => collect_used_expr(e, out),
        Stmt::If { cond, then, els, .. } => {
            collect_used_expr(cond, out);
            for st in then { collect_used(st, out); }
            if let Some(e) = els { for st in e { collect_used(st, out); } }
        }
        Stmt::While { cond, body, .. } => { collect_used_expr(cond, out); for st in body { collect_used(st, out); } }
        Stmt::ForRange { from, to, body, .. } => {
            collect_used_expr(from, out); collect_used_expr(to, out);
            for st in body { collect_used(st, out); }
        }
        Stmt::ForEach { iter, body, .. } => { collect_used_expr(iter, out); for st in body { collect_used(st, out); } }
        Stmt::Block(inner) => for st in inner { collect_used(st, out); }
        Stmt::Throw(e, _) => collect_used_expr(e, out),
        Stmt::Asm { .. } => {}
        Stmt::Try { body, catches, fin, .. } => {
            for st in body { collect_used(st, out); }
            for ca in catches {
                if let Some(g) = &ca.guard { collect_used_expr(g, out); }
                for st in &ca.body { collect_used(st, out); }
            }
            if let Some(f) = fin { for st in f { collect_used(st, out); } }
        }
        _ => {}
    }
}

/// 收集表达式中使用到的变量名。
fn collect_used_expr(e: &Expr, out: &mut Vec<String>) {
    match &e.kind {
        ExprKind::Ident(n) => out.push(n.clone()),
        ExprKind::Borrow { inner, .. } => collect_used_expr(inner, out),
        ExprKind::Unary(_, a) => collect_used_expr(a, out),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => { collect_used_expr(a, out); collect_used_expr(b, out); }
        ExprKind::Call(_, args) | ExprKind::CallValue { args, .. } => for a in args { collect_used_expr(a, out); },
        ExprKind::Field(base, _) => collect_used_expr(base, out),
        ExprKind::StructLit(_, fs) => for (_, v) in fs { collect_used_expr(v, out); },
        ExprKind::ArrayLit(items) => for it in items { collect_used_expr(it, out); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { collect_used_expr(i, out); } },
        ExprKind::ClosureNew { captures, .. } => for c in captures { collect_used_expr(c, out); },
        ExprKind::If { cond, then, els } => {
            collect_used_expr(cond, out);
            for st in then { collect_used(st, out); }
            if let Some(e) = els { for st in e { collect_used(st, out); } }
        }
        _ => {}
    }
}

/// 取得语句的行号（用于诊断）。
fn stmt_line(s: &Stmt) -> usize {
    match s {
        Stmt::Let { line, .. } => *line,
        Stmt::Go { line, .. } => *line,
        Stmt::Const { line, .. } => *line,
        Stmt::Assign { line, .. } => *line,
        Stmt::FieldAssign { line, .. } => *line,
        Stmt::Expr(e) => e.line,
        Stmt::If { line, .. } => *line,
        Stmt::While { line, .. } => *line,
        Stmt::Labeled { line, .. } => *line,
        Stmt::DoWhile { line, .. } => *line,
        Stmt::ForRange { line, .. } => *line,
        Stmt::ForEach { line, .. } => *line,
        Stmt::Return(_, line) => *line,
        Stmt::Break(_, line) | Stmt::Continue(_, line) => *line,
        Stmt::Block(b) => b.first().map(stmt_line).unwrap_or(0),
        Stmt::LocalFn(f) => f.line,
        Stmt::Try { line, .. } => *line,
        Stmt::Throw(_, line) => *line,
        Stmt::Asm { line, .. } => *line,
    }
}

/// 两个借用是否冲突（字段级精度）：
///   - 变量不同 → 不冲突
///   - 任一为"整变量借用"（field=None）→ 冲突
///   - 字段不同 → 不冲突
///   - 字段相同且任一为 &mut → 冲突；两个 &mut 也冲突
///   - 字段相同且都是共享 → 不冲突
fn loans_conflict(a: &Loan, b: &Loan) -> bool {
    if a.var != b.var { return false; }
    match (&a.field, &b.field) {
        (None, _) | (_, None) => true, // 整变量借用与任意借用冲突
        (Some(fa), Some(fb)) if fa == fb => a.mutable || b.mutable,
        _ => false, // 不同字段，互不影响
    }
}
