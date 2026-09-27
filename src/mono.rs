//! 泛型单态化：把泛型函数按调用点的具体类型实例化为独立函数。
//!
//! 例：`fn 恒等[T](x: T) -> T { x }` 被 `恒等(1)` 与 `恒等("a")` 调用，
//! 生成 `恒等$i` 与 `恒等$s` 两个具体函数，调用点改写为对应实例名。
//!
//! 实例名编码：i64→i, f64→f, str→s, bool→b, Struct(X)→X, 其它→名字。
//! 支持泛型调用泛型（迭代到不动点）。

use std::collections::{HashMap, HashSet};

use crate::ast::*;

/// 对整份程序做单态化（就地修改）；返回 `where` 约束违规的诊断。
pub fn monomorphize(prog: &mut Program) -> Vec<String> {
    // 收集泛型函数
    let mut generics: HashMap<String, FnDef> = HashMap::new();
    for item in &prog.items {
        if let Item::Fn(f) = item {
            if !f.type_params.is_empty() {
                generics.insert(f.name.clone(), f.clone());
            }
        }
    }
    // trait 表：(trait 名) → { 实现了它的类型名 }
    // blanket 表：(trait 名) 被 `impl[T] Trait for T` 覆盖 → 任意类型都满足
    let mut trait_impls: HashMap<String, HashSet<String>> = HashMap::new();
    let mut blanket_traits: HashSet<String> = HashSet::new();
    for item in &prog.items {
        if let Item::TraitImpl { type_params, trait_name, ty, .. } = item {
            if !type_params.is_empty() && type_params.iter().any(|t| t == ty) {
                // `impl[T] Trait for T`：blanket，任意 T 满足
                blanket_traits.insert(trait_name.clone());
            } else {
                trait_impls
                    .entry(trait_name.clone())
                    .or_default()
                    .insert(ty.clone());
            }
        }
    }
    let mut errors: Vec<String> = Vec::new();

    // ---------- 泛型 struct 单态化 ----------
    // 扫描所有 StructLit，按字段类型推导类型参数，生成 `名$编码` 具体 struct。
    // 例：`struct 盒[T] { v: T }` + `盒 { v: 42 }` → `盒$i { v: i64 }`，字面量改名为 `盒$i`
    errors.extend(monomorphize_structs(prog));

    if generics.is_empty() {
        return errors;
    }

    // 拆分：非泛型函数、其它顶层项、泛型函数（丢弃原泛型）
    let mut plain_fns: Vec<FnDef> = Vec::new();
    let mut others: Vec<Item> = Vec::new();
    for item in prog.items.drain(..) {
        match item {
            Item::Fn(f) if f.type_params.is_empty() => plain_fns.push(f),
            Item::Fn(_) => {} // 泛型原函数丢弃，稍后以实例形式加入
            other => others.push(other),
        }
    }

    // 迭代扫描：从 plain_fns + 已生成实例中收集泛型调用，生成实例
    let mut instances: Vec<FnDef> = Vec::new();
    let mut emitted: HashSet<String> = HashSet::new();
    let mut subst_map: HashMap<String, String> = HashMap::new();

    loop {
        let mut sites: Vec<(String, Vec<Ty>)> = Vec::new();
        for f in &plain_fns {
            collect_generic_calls(&f.body, &generics, &mut sites);
        }
        for f in &instances {
            collect_generic_calls(&f.body, &generics, &mut sites);
        }
        let mut changed = false;
        for (gname, tys) in sites {
            let inst = instance_name(&gname, &tys);
            subst_map.insert(format!("{}<{}>", gname, tys_key(&tys)), inst.clone());
            if emitted.insert(inst.clone()) {
                if let Some(gf) = generics.get(&gname) {
                    // 校验 where 约束：每个 (类型参数, trait) 的实参类型须 impl 该 trait
                    for (tp, tr) in &gf.bounds {
                        if let Some(actual) = gf.type_params.iter().position(|t| t == tp) {
                            if let Some(aty) = tys.get(actual) {
                                let tname = type_name_for_trait(aty);
                                let ok = blanket_traits.contains(tr)
                                    || trait_impls.get(tr).map(|s| s.contains(&tname)).unwrap_or(false);
                                if !ok {
                                    errors.push(crate::lb!(
                                        0,
                                        "type '{}' does not satisfy bound '{}: {}'",
                                        "类型 '{}' 不满足约束 '{}: {}'",
                                        tname, tp, tr
                                    ));
                                }
                            }
                        }
                    }
                    let mut inst_fn = instantiate(gf, &tys, &inst);
                    // 实例体内的**自递归调用** `gname(...)` 改写为实例名
                    rewrite_self_calls(&mut inst_fn.body, &gname, &inst);
                    instances.push(inst_fn);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    // 改写所有非泛型函数体内的泛型调用
    for f in plain_fns.iter_mut() {
        rewrite_calls(&mut f.body, &generics, &subst_map);
    }

    // 组装最终 items：非函数项 + 非泛型函数 + 实例函数
    let mut out: Vec<Item> = others;
    for f in plain_fns {
        out.push(Item::Fn(f));
    }
    for f in instances {
        out.push(Item::Fn(f));
    }
    prog.items = out;
    errors
}

/// 取类型的"trait 实现名"（用于匹配 `impl Trait for 类型` 里的 `类型`）。
fn type_name_for_trait(t: &Ty) -> String {
    match t {
        Ty::Struct(n) => n.clone(),
        Ty::I64 => "int".into(),
        Ty::F64 => "f64".into(),
        Ty::Str => "str".into(),
        Ty::Bool => "bool".into(),
        other => other.to_string(),
    }
}

// 占位（保留以兼容旧签名）
#[allow(dead_code)]
fn worklist_sites(changed: &mut bool) -> Vec<(String, Vec<Ty>)> {
    *changed = true;
    Vec::new()
}

/// 收集函数体内对泛型函数的调用及其类型实参
fn collect_generic_calls(b: &Block, generics: &HashMap<String, FnDef>, out: &mut Vec<(String, Vec<Ty>)>) {
    for s in b {
        match s {
            Stmt::Let { value, .. } => collect_expr(value, generics, out),
            Stmt::Assign { value, .. } => collect_expr(value, generics, out),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => collect_expr(e, generics, out),
            Stmt::If { cond, then, els, .. } => {
                collect_expr(cond, generics, out);
                collect_generic_calls(then, generics, out);
                if let Some(e) = els { collect_generic_calls(e, generics, out); }
            }
            Stmt::While { cond, body, .. } => {
                collect_expr(cond, generics, out);
                collect_generic_calls(body, generics, out);
            }
            Stmt::ForRange { from, to, body, .. } => {
                collect_expr(from, generics, out);
                collect_expr(to, generics, out);
                collect_generic_calls(body, generics, out);
            }
            Stmt::ForEach { iter, body, .. } => {
                collect_expr(iter, generics, out);
                collect_generic_calls(body, generics, out);
            }
            Stmt::Block(inner) => collect_generic_calls(inner, generics, out),
            Stmt::FieldAssign { value, .. } => collect_expr(value, generics, out),
            _ => {}
        }
    }
}

fn collect_expr(e: &Expr, generics: &HashMap<String, FnDef>, out: &mut Vec<(String, Vec<Ty>)>) {
    if let ExprKind::Call(name, args) = &e.kind {
        if let Some(gf) = generics.get(name) {
            // 从实参类型推断类型实参
            if let Some(tys) = infer_type_args(gf, args) {
                out.push((name.clone(), tys));
            }
        }
    }
    // 递归
    match &e.kind {
        ExprKind::Call(_, args) => for a in args { collect_expr(a, generics, out); },
        ExprKind::Unary(_, a) => collect_expr(a, generics, out),
        ExprKind::Binary(_, a, b) => { collect_expr(a, generics, out); collect_expr(b, generics, out); }
        ExprKind::Index(a, b) => { collect_expr(a, generics, out); collect_expr(b, generics, out); }
        ExprKind::ArrayLit(xs) => for x in xs { collect_expr(x, generics, out); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { collect_expr(i, generics, out); } },
        ExprKind::If { cond, then, els } => {
            collect_expr(cond, generics, out);
            collect_generic_calls(then, generics, out);
            if let Some(e) = els { collect_generic_calls(e, generics, out); }
        }
        ExprKind::Match { subject, arms } => {
            collect_expr(subject, generics, out);
            for arm in arms {
                if let Some(p) = &arm.pat { collect_expr(p, generics, out); }
                if let Some(g) = &arm.guard { collect_expr(g, generics, out); }
                collect_generic_calls(&arm.body, generics, out);
            }
        }
        ExprKind::Field(base, _) => collect_expr(base, generics, out),
        ExprKind::StructLit(_, fields) => for (_, v) in fields { collect_expr(v, generics, out); },
        _ => {}
    }
}

/// 从实参类型推断泛型参数（按出现顺序，支持嵌套）
fn infer_type_args(gf: &FnDef, args: &[Expr]) -> Option<Vec<Ty>> {
    let mut map: HashMap<String, Ty> = HashMap::new();
    for (i, p) in gf.params.iter().enumerate() {
        let at = args.get(i)?.ty.clone();
        if let Some(pty) = &p.ty {
            unify(pty, &at, &mut map);
        }
    }
    // 按声明顺序输出
    let mut out = Vec::new();
    for tp in &gf.type_params {
        out.push(map.get(tp).cloned().unwrap_or(Ty::I64));
    }
    Some(out)
}

fn unify(param: &Ty, actual: &Ty, map: &mut HashMap<String, Ty>) {
    match param {
        Ty::Generic(n) => {
            map.entry(n.clone()).or_insert_with(|| actual.clone());
        }
        Ty::List(e) => { if let Ty::List(a) = actual { unify(e, a, map); } }
        Ty::Array(e, _) => { if let Ty::Array(a, _) = actual { unify(e, a, map); } }
        _ => {}
    }
}

/// 泛型 struct 单态化：扫描 `StructLit`，推导类型参数，生成具体 struct 定义并改名引用。
fn monomorphize_structs(prog: &mut Program) -> Vec<String> {
    // 收集泛型 struct：名字 → StructDef
    let mut gen_structs: HashMap<String, StructDef> = HashMap::new();
    for item in &prog.items {
        if let Item::Struct(s) = item {
            if !s.type_params.is_empty() {
                gen_structs.insert(s.name.clone(), s.clone());
            }
        }
    }
    if gen_structs.is_empty() {
        return Vec::new();
    }

    // 扫描所有函数体，收集 (泛型名, 推导出的类型实参)
    let mut sites: Vec<(String, Vec<Ty>)> = Vec::new();
    for item in &prog.items {
        if let Item::Fn(f) = item {
            collect_struct_sites(&f.body, &gen_structs, &mut sites);
        }
    }
    // 去重 + 生成实例
    let mut emitted: HashSet<String> = HashSet::new();
    let mut new_structs: Vec<Item> = Vec::new();
    let mut rename: HashMap<(String, String), String> = HashMap::new(); // (名, 类型key) → 实例名
    for (sname, tys) in &sites {
        let gs = match gen_structs.get(sname) { Some(g) => g, None => continue };
        // 类型参数顺序对应结构体声明的 type_params；tys 来自字段推导
        let inst = format!("{}$", sname) + &tys.iter().map(ty_code).collect::<Vec<_>>().join("_");
        rename.insert((sname.clone(), tys.iter().map(ty_code).collect::<Vec<_>>().join("_")), inst.clone());
        if emitted.insert(inst.clone()) {
            let mut sd = gs.clone();
            let map: HashMap<String, Ty> = gs.type_params.iter().cloned().zip(tys.iter().cloned()).collect();
            sd.fields = sd.fields.iter().map(|(n, t, l)| {
                (n.clone(), t.as_ref().map(|tt| subst_ty(tt, &map)), *l)
            }).collect();
            sd.type_params = Vec::new();
            sd.name = inst;
            new_structs.push(Item::Struct(sd));
        }
    }
    // 改写函数体里的 StructLit 名（按字段类型选实例）
    // 每个 (sname) → (实例名列表, 对应类型实参列表)
    let mut to_rename: HashMap<String, (Vec<String>, Vec<Vec<Ty>>)> = HashMap::new();
    for (sname, tys) in &sites {
        if let Some(insts) = to_rename.get_mut(sname) {
            insts.0.push(format!("{}$", sname) + &tys.iter().map(ty_code).collect::<Vec<_>>().join("_"));
            insts.1.push(tys.clone());
        } else {
            to_rename.insert(
                sname.clone(),
                (vec![format!("{}$", sname) + &tys.iter().map(ty_code).collect::<Vec<_>>().join("_")], vec![tys.clone()]),
            );
        }
    }
    for item in prog.items.iter_mut() {
        if let Item::Fn(f) = item {
            rewrite_struct_lits(&mut f.body, &to_rename);
        }
    }
    // 追加新 struct 定义
    prog.items.extend(new_structs);
    Vec::new()
}

/// 收集函数体里泛型 struct 字面量的类型实参（按字段类型推导）
fn collect_struct_sites(b: &Block, gen: &HashMap<String, StructDef>, out: &mut Vec<(String, Vec<Ty>)>) {
    for s in b {
        match s {
            Stmt::Let { value, .. } | Stmt::Const { value, .. } => collect_struct_sites_expr(value, gen, out),
            Stmt::Assign { value, .. } | Stmt::FieldAssign { value, .. } => collect_struct_sites_expr(value, gen, out),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => collect_struct_sites_expr(e, gen, out),
            Stmt::If { cond, then, els, .. } => {
                collect_struct_sites_expr(cond, gen, out);
                collect_struct_sites(then, gen, out);
                if let Some(e) = els { collect_struct_sites(e, gen, out); }
            }
            Stmt::While { cond, body, .. } => { collect_struct_sites_expr(cond, gen, out); collect_struct_sites(body, gen, out); }
            Stmt::ForRange { from, to, body, .. } => { collect_struct_sites_expr(from, gen, out); collect_struct_sites_expr(to, gen, out); collect_struct_sites(body, gen, out); }
            Stmt::ForEach { iter, body, .. } => { collect_struct_sites_expr(iter, gen, out); collect_struct_sites(body, gen, out); }
            Stmt::Block(inner) => collect_struct_sites(inner, gen, out),
            _ => {}
        }
    }
}

fn collect_struct_sites_expr(e: &Expr, gen: &HashMap<String, StructDef>, out: &mut Vec<(String, Vec<Ty>)>) {
    if let ExprKind::StructLit(name, fields) = &e.kind {
        if let Some(gs) = gen.get(name) {
            // 按结构体字段顺序，从字面量取对应字段的类型
            let mut tys: Vec<Ty> = Vec::new();
            for tp in &gs.type_params {
                // 找结构体里类型为 Generic(tp) 的字段，取字面量对应值的类型
                let mut found = Ty::Unknown;
                for (fname, fty, _) in &gs.fields {
                    if fty.as_ref() == Some(&Ty::Generic(tp.clone())) {
                        if let Some((_, v)) = fields.iter().find(|(n, _)| n == fname) {
                            found = if v.ty == Ty::Unknown { Ty::I64 } else { v.ty.clone() };
                        }
                    }
                }
                tys.push(if found == Ty::Unknown { Ty::I64 } else { found });
            }
            out.push((name.clone(), tys));
        }
    }
    // 递归子表达式
    match &e.kind {
        ExprKind::Unary(_, a) => collect_struct_sites_expr(a, gen, out),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => { collect_struct_sites_expr(a, gen, out); collect_struct_sites_expr(b, gen, out); }
        ExprKind::Call(_, args) => for a in args { collect_struct_sites_expr(a, gen, out); },
        ExprKind::CallValue { callee, args } => { collect_struct_sites_expr(callee, gen, out); for a in args { collect_struct_sites_expr(a, gen, out); } }
        ExprKind::ArrayLit(xs) => for x in xs { collect_struct_sites_expr(x, gen, out); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { collect_struct_sites_expr(i, gen, out); } },
        ExprKind::Field(base, _) => collect_struct_sites_expr(base, gen, out),
        ExprKind::StructLit(_, fs) => for (_, v) in fs { collect_struct_sites_expr(v, gen, out); },
        _ => {}
    }
}

/// 把函数体里的 `盒`（泛型 struct 字面量）改名为 `盒$i`（按字段类型选实例）
fn rewrite_struct_lits(b: &mut Block, map: &HashMap<String, (Vec<String>, Vec<Vec<Ty>>)>) {
    for s in b.iter_mut() {
        match s {
            Stmt::Let { value, .. } | Stmt::Const { value, .. } => rewrite_struct_lit_expr(value, map),
            Stmt::Assign { value, .. } | Stmt::FieldAssign { value, .. } => rewrite_struct_lit_expr(value, map),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => rewrite_struct_lit_expr(e, map),
            Stmt::If { cond, then, els, .. } => { rewrite_struct_lit_expr(cond, map); rewrite_struct_lits(then, map); if let Some(e) = els { rewrite_struct_lits(e, map); } }
            Stmt::While { cond, body, .. } => { rewrite_struct_lit_expr(cond, map); rewrite_struct_lits(body, map); }
            Stmt::ForRange { from, to, body, .. } => { rewrite_struct_lit_expr(from, map); rewrite_struct_lit_expr(to, map); rewrite_struct_lits(body, map); }
            Stmt::ForEach { iter, body, .. } => { rewrite_struct_lit_expr(iter, map); rewrite_struct_lits(body, map); }
            Stmt::Block(inner) => rewrite_struct_lits(inner, map),
            _ => {}
        }
    }
}

fn rewrite_struct_lit_expr(e: &mut Expr, map: &HashMap<String, (Vec<String>, Vec<Vec<Ty>>)>) {
    if let ExprKind::StructLit(name, fields) = &mut e.kind {
        if let Some((insts, tys_list)) = map.get(name) {
            let mut sel = insts.first().cloned();
            // 用实例的类型编码与字面量字段类型编码前缀匹配
            let ftys: Vec<Ty> = fields.iter().filter_map(|(_, v)| {
                if v.ty != Ty::Unknown { Some(v.ty.clone()) } else { None }
            }).collect();
            for (i, tys) in tys_list.iter().enumerate() {
                if !tys.is_empty() && tys.iter().all(|t| ftys.iter().any(|f| std::mem::discriminant(t) == std::mem::discriminant(f))) {
                    sel = insts.get(i).cloned();
                    break;
                }
            }
            if let Some(inst) = sel { *name = inst; }
        }
        for (_, v) in fields.iter_mut() { rewrite_struct_lit_expr(v, map); }
        return;
    }
    match &mut e.kind {
        ExprKind::Unary(_, a) => rewrite_struct_lit_expr(a, map),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => { rewrite_struct_lit_expr(a, map); rewrite_struct_lit_expr(b, map); }
        ExprKind::Call(_, args) => for a in args { rewrite_struct_lit_expr(a, map); },
        ExprKind::CallValue { callee, args } => { rewrite_struct_lit_expr(callee, map); for a in args { rewrite_struct_lit_expr(a, map); } }
        ExprKind::ArrayLit(xs) => for x in xs { rewrite_struct_lit_expr(x, map); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { rewrite_struct_lit_expr(i, map); } },
        ExprKind::Field(base, _) => rewrite_struct_lit_expr(base, map),
        _ => {}
    }
}

/// 实例名：泛型名 + 类型编码
fn instance_name(gname: &str, tys: &[Ty]) -> String {
    let mut s = gname.to_string();
    for t in tys {
        s.push('$');
        s.push_str(&ty_code(t));
    }
    s
}

fn tys_key(tys: &[Ty]) -> String {
    tys.iter().map(ty_code).collect::<Vec<_>>().join("_")
}

fn ty_code(t: &Ty) -> String {
    match t {
        Ty::I64 => "i".into(),
        Ty::F64 => "f".into(),
        Ty::Str => "s".into(),
        Ty::Bool => "b".into(),
        Ty::Void => "v".into(),
        Ty::Struct(n) => n.clone(),
        Ty::List(e) => format!("L{}", ty_code(e)),
        Ty::Array(e, n) => format!("A{}_{}", ty_code(e), n),
        Ty::Generic(n) => n.clone(),
        _ => "u".into(),
    }
}

/// 实例化：克隆泛型函数，替换 Ty::Generic 为具体类型，重命名为 inst
fn instantiate(gf: &FnDef, tys: &[Ty], inst: &str) -> FnDef {
    let map: HashMap<String, Ty> = gf
        .type_params
        .iter()
        .cloned()
        .zip(tys.iter().cloned())
        .collect();
    let mut f = gf.clone();
    f.name = inst.to_string();
    f.type_params = Vec::new();
    // 参数类型替换
    for p in f.params.iter_mut() {
        if let Some(t) = &p.ty {
            p.ty = Some(subst_ty(t, &map));
        }
    }
    if let Some(r) = &f.ret {
        f.ret = Some(subst_ty(r, &map));
    }
    // 体内类型替换（数组字面量等已回填的类型）
    subst_block_ty(&mut f.body, &map);
    f
}

fn subst_ty(t: &Ty, map: &HashMap<String, Ty>) -> Ty {
    match t {
        Ty::Generic(n) => map.get(n).cloned().unwrap_or(Ty::I64),
        Ty::List(e) => Ty::List(Box::new(subst_ty(e, map))),
        Ty::Set(e) => Ty::Set(Box::new(subst_ty(e, map))),
        Ty::Map(k, v) => Ty::Map(Box::new(subst_ty(k, map)), Box::new(subst_ty(v, map))),
        Ty::Result(t, e) => Ty::Result(Box::new(subst_ty(t, map)), Box::new(subst_ty(e, map))),
        Ty::Array(e, n) => Ty::Array(Box::new(subst_ty(e, map)), *n),
        other => other.clone(),
    }
}

fn subst_block_ty(_b: &mut Block, _map: &HashMap<String, Ty>) {
    // 类型替换主要在签名层；体内的 Ty 由 sema 重新推断，这里不深入
}

/// 改写函数体内的泛型调用为实例名
fn rewrite_calls(b: &mut Block, generics: &HashMap<String, FnDef>, subst: &HashMap<String, String>) {
    for s in b.iter_mut() {
        match s {
            Stmt::Let { value, .. } => rewrite_expr(value, generics, subst),
            Stmt::Assign { value, .. } => rewrite_expr(value, generics, subst),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => rewrite_expr(e, generics, subst),
            Stmt::If { cond, then, els, .. } => {
                rewrite_expr(cond, generics, subst);
                rewrite_calls(then, generics, subst);
                if let Some(e) = els { rewrite_calls(e, generics, subst); }
            }
            Stmt::While { cond, body, .. } => {
                rewrite_expr(cond, generics, subst);
                rewrite_calls(body, generics, subst);
            }
            Stmt::ForRange { from, to, body, .. } => {
                rewrite_expr(from, generics, subst);
                rewrite_expr(to, generics, subst);
                rewrite_calls(body, generics, subst);
            }
            Stmt::ForEach { iter, body, .. } => {
                rewrite_expr(iter, generics, subst);
                rewrite_calls(body, generics, subst);
            }
            Stmt::Block(inner) => rewrite_calls(inner, generics, subst),
            Stmt::FieldAssign { value, .. } => rewrite_expr(value, generics, subst),
            _ => {}
        }
    }
}

fn rewrite_expr(e: &mut Expr, generics: &HashMap<String, FnDef>, subst: &HashMap<String, String>) {
    if let ExprKind::Call(name, args) = &mut e.kind {
        if let Some(gf) = generics.get(name.as_str()) {
            if let Some(tys) = infer_type_args(gf, args) {
                let key = format!("{}<{}>", name, tys_key(&tys));
                if let Some(inst) = subst.get(&key) {
                    *name = inst.clone();
                }
            }
        }
    }
    match &mut e.kind {
        ExprKind::Call(_, args) => for a in args { rewrite_expr(a, generics, subst); },
        ExprKind::Unary(_, a) => rewrite_expr(a, generics, subst),
        ExprKind::Binary(_, a, b) => { rewrite_expr(a, generics, subst); rewrite_expr(b, generics, subst); }
        ExprKind::Index(a, b) => { rewrite_expr(a, generics, subst); rewrite_expr(b, generics, subst); }
        ExprKind::ArrayLit(xs) => for x in xs { rewrite_expr(x, generics, subst); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { rewrite_expr(i, generics, subst); } },
        ExprKind::If { cond, then, els } => {
            rewrite_expr(cond, generics, subst);
            rewrite_calls(then, generics, subst);
            if let Some(e) = els { rewrite_calls(e, generics, subst); }
        }
        ExprKind::Match { subject, arms } => {
            rewrite_expr(subject, generics, subst);
            for arm in arms {
                if let Some(p) = &mut arm.pat { rewrite_expr(p, generics, subst); }
                if let Some(g) = &mut arm.guard { rewrite_expr(g, generics, subst); }
                rewrite_calls(&mut arm.body, generics, subst);
            }
        }
        ExprKind::Field(base, _) => rewrite_expr(base, generics, subst),
        ExprKind::StructLit(_, fields) => for (_, v) in fields { rewrite_expr(v, generics, subst); },
        _ => {}
    }
}


// ============================================================
// 方法调用降级：`obj.方法(args)` → `类型__方法(obj, args)`
// 与 `obj.字段` 区分：字段访问读/写字段；方法调用传 self。
// 依据：obj 的静态类型是 Struct(T)，且 T 有方法 `方法`。
// ============================================================

/// 对整份程序做方法调用降级（就地修改）。
pub fn lower_method_calls(prog: &mut Program, methods: &HashMap<String, Vec<String>>) {
    // methods: 类型名 → 方法名列表
    let struct_types: HashMap<String, ()> = prog
        .items
        .iter()
        .filter_map(|it| if let Item::Struct(s) = it { Some((s.name.clone(), ())) } else { None })
        .collect();
    // 变量名 → 结构体类型（作用域内）
    for item in &mut prog.items {
        if let Item::Fn(f) = item {
            lower_block(&mut f.body, methods, &struct_types, &mut HashMap::new());
        }
    }
}

fn lower_block(
    b: &mut Block,
    methods: &HashMap<String, Vec<String>>,
    structs: &HashMap<String, ()>,
    vars: &mut HashMap<String, String>,
) {
    for s in b.iter_mut() {
        match s {
            Stmt::Let { name, value, .. } => {
                lower_expr(value, methods, structs, vars);
                if let Some(t) = expr_struct_type(value) {
                    vars.insert(name.clone(), t);
                }
            }
            Stmt::Assign { value, .. } => lower_expr(value, methods, structs, vars),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => lower_expr(e, methods, structs, vars),
            Stmt::If { cond, then, els, .. } => {
                lower_expr(cond, methods, structs, vars);
                lower_block(then, methods, structs, &mut vars.clone());
                if let Some(e) = els { lower_block(e, methods, structs, &mut vars.clone()); }
            }
            Stmt::While { cond, body, .. } => {
                lower_expr(cond, methods, structs, vars);
                lower_block(body, methods, structs, &mut vars.clone());
            }
            Stmt::ForRange { from, to, body, .. } => {
                lower_expr(from, methods, structs, vars);
                lower_expr(to, methods, structs, vars);
                lower_block(body, methods, structs, &mut vars.clone());
            }
            Stmt::ForEach { iter, body, .. } => {
                lower_expr(iter, methods, structs, vars);
                lower_block(body, methods, structs, &mut vars.clone());
            }
            Stmt::Block(inner) => lower_block(inner, methods, structs, vars),
            Stmt::FieldAssign { value, .. } => lower_expr(value, methods, structs, vars),
            _ => {}
        }
    }
}

fn expr_struct_type(e: &Expr) -> Option<String> {
    if let Ty::Struct(n) = &e.ty {
        Some(n.clone())
    } else {
        None
    }
}

fn lower_expr(
    e: &mut Expr,
    methods: &HashMap<String, Vec<String>>,
    structs: &HashMap<String, ()>,
    vars: &HashMap<String, String>,
) {
    // 先递归
    match &mut e.kind {
        ExprKind::Call(_, args) => for a in args { lower_expr(a, methods, structs, vars); },
        ExprKind::Unary(_, a) => lower_expr(a, methods, structs, vars),
        ExprKind::Binary(_, a, b) => { lower_expr(a, methods, structs, vars); lower_expr(b, methods, structs, vars); }
        ExprKind::Index(a, b) => { lower_expr(a, methods, structs, vars); lower_expr(b, methods, structs, vars); }
        ExprKind::ArrayLit(xs) => for x in xs { lower_expr(x, methods, structs, vars); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { lower_expr(i, methods, structs, vars); } },
        ExprKind::If { cond, then, els } => {
            lower_expr(cond, methods, structs, vars);
            lower_block(then, methods, structs, &mut vars.clone());
            if let Some(x) = els { lower_block(x, methods, structs, &mut vars.clone()); }
        }
        ExprKind::Field(base, _) => lower_expr(base, methods, structs, vars),
        ExprKind::StructLit(_, fields) => for (_, v) in fields { lower_expr(v, methods, structs, vars); },
        _ => {}
    }
    // 运算符重载：`a + b`（a 是结构体且有 add 方法）→ `类型__add(a, b)`
    if let ExprKind::Binary(op, a, b) = &e.kind {
        if let Some(m) = crate::types::op_method(*op) {
            if let ExprKind::Ident(v) = &a.kind {
                if let Some(ty) = vars.get(v) {
                    if methods.get(ty).map_or(false, |ms| ms.contains(&m.to_string())) {
                        let lhs = (**a).clone();
                        let rhs = (**b).clone();
                        e.kind = ExprKind::Call(format!("{}__{}", ty, m), vec![lhs, rhs]);
                    }
                }
            }
        }
    }
    // 一元运算符重载：`-a` → `类型__neg(a)`
    if let ExprKind::Unary(op, a) = &e.kind {
        if let Some(m) = crate::types::unary_op_method(*op) {
            if let ExprKind::Ident(v) = &a.kind {
                if let Some(ty) = vars.get(v) {
                    if methods.get(ty).map_or(false, |ms| ms.contains(&m.to_string())) {
                        let operand = (**a).clone();
                        e.kind = ExprKind::Call(format!("{}__{}", ty, m), vec![operand]);
                    }
                }
            }
        }
    }
    // 方法调用：Call(name) 其中 name = "obj.方法"
    if let ExprKind::Call(name, args) = &mut e.kind {
        if let Some(dot) = name.find('.') {
            let obj = name[..dot].to_string();
            let method = name[dot + 1..].to_string();
            // obj 是变量且其类型有该方法
            if let Some(ty) = vars.get(&obj) {
                if let Some(ms) = methods.get(ty) {
                    if ms.contains(&method) {
                        let self_expr = Expr::new(ExprKind::Ident(obj.clone()), e.line);
                        let mut new_args = vec![self_expr];
                        new_args.extend(args.drain(..));
                        *args = new_args;
                        *name = format!("{}__{}", ty, method);
                    }
                }
            }
        }
    }
}

/// 把函数体内对 `gname` 的调用改写为 `inst`（用于泛型自递归）
fn rewrite_self_calls(b: &mut Block, gname: &str, inst: &str) {
    for s in b.iter_mut() {
        match s {
            Stmt::Let { value, .. } => rewrite_self_expr(value, gname, inst),
            Stmt::Assign { value, .. } => rewrite_self_expr(value, gname, inst),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => rewrite_self_expr(e, gname, inst),
            Stmt::If { cond, then, els, .. } => {
                rewrite_self_expr(cond, gname, inst);
                rewrite_self_calls(then, gname, inst);
                if let Some(e) = els { rewrite_self_calls(e, gname, inst); }
            }
            Stmt::While { cond, body, .. } => { rewrite_self_expr(cond, gname, inst); rewrite_self_calls(body, gname, inst); }
            Stmt::ForRange { from, to, body, .. } => { rewrite_self_expr(from, gname, inst); rewrite_self_expr(to, gname, inst); rewrite_self_calls(body, gname, inst); }
            Stmt::ForEach { iter, body, .. } => { rewrite_self_expr(iter, gname, inst); rewrite_self_calls(body, gname, inst); }
            Stmt::Block(inner) => rewrite_self_calls(inner, gname, inst),
            Stmt::FieldAssign { value, .. } => rewrite_self_expr(value, gname, inst),
            _ => {}
        }
    }
}

fn rewrite_self_expr(e: &mut Expr, gname: &str, inst: &str) {
    if let ExprKind::Call(name, args) = &mut e.kind {
        if name == gname { *name = inst.to_string(); }
        for a in args.iter_mut() { rewrite_self_expr(a, gname, inst); }
    }
    match &mut e.kind {
        ExprKind::Call(_, args) => for a in args { rewrite_self_expr(a, gname, inst); },
        ExprKind::CallValue { callee, args } => { rewrite_self_expr(callee, gname, inst); for a in args { rewrite_self_expr(a, gname, inst); } }
        ExprKind::Unary(_, a) => rewrite_self_expr(a, gname, inst),
        ExprKind::Binary(_, a, b) => { rewrite_self_expr(a, gname, inst); rewrite_self_expr(b, gname, inst); }
        ExprKind::Index(a, b) => { rewrite_self_expr(a, gname, inst); rewrite_self_expr(b, gname, inst); }
        ExprKind::ArrayLit(xs) => for x in xs { rewrite_self_expr(x, gname, inst); },
        ExprKind::Interp(parts) => for p in parts { if let StrPart::Expr(i) = p { rewrite_self_expr(i, gname, inst); } },
        ExprKind::If { cond, then, els } => { rewrite_self_expr(cond, gname, inst); rewrite_self_calls(then, gname, inst); if let Some(e) = els { rewrite_self_calls(e, gname, inst); } }
        ExprKind::Match { subject, arms } => {
            rewrite_self_expr(subject, gname, inst);
            for arm in arms {
                if let Some(p) = &mut arm.pat { rewrite_self_expr(p, gname, inst); }
                if let Some(g) = &mut arm.guard { rewrite_self_expr(g, gname, inst); }
                rewrite_self_calls(&mut arm.body, gname, inst);
            }
        }
        ExprKind::Field(base, _) => rewrite_self_expr(base, gname, inst),
        ExprKind::StructLit(_, fields) => for (_, v) in fields { rewrite_self_expr(v, gname, inst); },
        ExprKind::ClosureNew { captures, .. } => for c in captures { rewrite_self_expr(c, gname, inst); },
        _ => {}
    }
}
