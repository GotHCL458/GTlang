//! AST -> .gt 源码打印器（unparse）。
//!
//! 目标：把（优化后的）AST 打印成**语义等价、可再编译**的 GTLang 源码。
//! 输出刻意扁平、无注释、极少缩进——面向机器而非人类（类似 clang 的优化 IR）。

use crate::ast::*;

/// 把名字里单态化产生的 `$`（如 `f$i`）替换为合法标识符字符 `_`，
/// 使打印出的源码可再编译（编译器会重新单态化，但这里按具体实例输出）。
fn san(n: &str) -> String { n.replace('$', "_") }

pub fn program(p: &Program) -> String {
    let mut s = String::new();
    for m in &p.imported_gtlib {
        s.push_str(&format!("import {}\n", m));
    }
    for imp in &p.imports {
        s.push_str("import ");
        if imp.is_c_header { s.push_str("c "); }
        if imp.is_file { s.push('"'); s.push_str(&imp.path.join(".")); s.push('"'); }
        else { s.push_str(&imp.path.join(".")); }
        if let Some(a) = &imp.alias { s.push_str(&format!(" as {}", a)); }
        s.push('\n');
    }
    if !p.imports.is_empty() { s.push('\n'); }
    for it in &p.items {
        item(&mut s, it, 0);
        s.push('\n');
    }
    if !p.cblock.trim().is_empty() {
        s.push_str("C {\n");
        s.push_str(&p.cblock);
        s.push_str("\n}\n");
    }
    s
}

fn indent(s: &mut String, n: usize) {
    for _ in 0..n { s.push(' '); }
}

/// 类型的**源码注解名**（int/str/list[...]/...），用于打印可再编译的源码。
fn ty_str(t: &Ty) -> String {
    match t {
        Ty::Unknown => "int".into(),
        Ty::Void => "void".into(),
        Ty::Bool => "bool".into(),
        Ty::I64 => "int".into(),
        Ty::F64 => "f64".into(),
        Ty::Decimal => "decimal".into(),
        Ty::Str => "str".into(),
        Ty::Array(el, n) => format!("[{}; {}]", ty_str(el), n),
        Ty::List(el) => format!("list[{}]", ty_str(el)),
        Ty::Set(el) => format!("set[{}]", ty_str(el)),
        Ty::Map(k, v) => format!("map[{}, {}]", ty_str(k), ty_str(v)),
        Ty::Struct(n) | Ty::Enum(n) | Ty::Generic(n) | Ty::Dyn(n) => san(n),
        Ty::Closure(ps, r) => format!("|{}| -> {}", ps.iter().map(|p| ty_str(p)).collect::<Vec<_>>().join(", "), ty_str(r)),
        Ty::Result(a, b) => format!("Result[{}, {}]", ty_str(a), ty_str(b)),
        Ty::Option(a) => format!("?{}", ty_str(a)),
        Ty::Tuple(ts) => format!("({})", ts.iter().map(|x| ty_str(x)).collect::<Vec<_>>().join(", ")),
        Ty::Ref(a) => format!("&{}", ty_str(a)),
        Ty::RefMut(a) => format!("&mut {}", ty_str(a)),
        Ty::Var(_) => "int".into(),
    }
}

fn opt_ty(t: &Option<Ty>) -> String {
    match t { Some(t) => format!(": {}", ty_str(t)), None => String::new() }
}

fn item(s: &mut String, it: &Item, depth: usize) {
    match it {
        Item::Fn(f) => fn_def(s, f, depth),
        Item::Const { name, ty, value, is_pub, .. } => { indent(s, depth); if *is_pub { s.push_str("pub "); } s.push_str(&format!("const {}{} = ", name, opt_ty(ty))); expr(s, value, depth); s.push('\n'); }
        Item::Struct(sd) => {
            indent(s, depth); if sd.is_pub { s.push_str("pub "); }
            s.push_str(&format!("struct {}{} {{\n", san(&sd.name), if sd.type_params.is_empty() { String::new() } else { format!("[{}]", sd.type_params.join(", ")) }));
            for (fname, fty, _) in &sd.fields { indent(s, depth + 4); s.push_str(fname); if let Some(t) = fty { s.push_str(&format!(": {}", ty_str(t))); } s.push('\n'); }
            indent(s, depth); s.push_str("}\n");
        }
        Item::Macro { name, params, body, .. } => { indent(s, depth); s.push_str(&format!("macro {}({}) {{\n", name, params.join(", "))); indent(s, depth + 4); expr(s, body, depth + 4); s.push('\n'); indent(s, depth); s.push_str("}\n"); }
        Item::Enum(en) => {
            indent(s, depth); if en.is_pub { s.push_str("pub "); }
            s.push_str(&format!("enum {}{} {{\n", san(&en.name), if en.type_params.is_empty() { String::new() } else { format!("[{}]", en.type_params.join(", ")) }));
            for (vname, tys) in &en.variants { indent(s, depth + 4); s.push_str(vname); if !tys.is_empty() { s.push('('); s.push_str(&tys.iter().map(|t| ty_str(t)).collect::<Vec<_>>().join(", ")); s.push(')'); } s.push('\n'); }
            indent(s, depth); s.push_str("}\n");
        }
        Item::Impl { type_params, ty, methods, .. } => {
            indent(s, depth); s.push_str("impl"); if !type_params.is_empty() { s.push('['); s.push_str(&type_params.join(", ")); s.push(']'); }
            s.push_str(&format!(" {} {{\n", san(ty)));
            for m in methods { fn_def(s, m, depth + 4); }
            indent(s, depth); s.push_str("}\n");
        }
        Item::ExternC(fns) => {
            indent(s, depth); s.push_str("extern \"C\" {\n");
            for f in fns { indent(s, depth + 4); s.push_str(&format!("fn {}(", san(&f.name))); s.push_str(&f.params.iter().enumerate().map(|(i, t)| format!("a{}: {}", i, ty_str(t))).collect::<Vec<_>>().join(", ")); s.push_str(&format!(") -> {}\n", ty_str(&f.ret))); }
            indent(s, depth); s.push_str("}\n");
        }
        Item::Trait(td) => {
            indent(s, depth); if td.is_pub { s.push_str("pub "); }
            s.push_str(&format!("trait {} {{\n", san(&td.name)));
            for (mname, tys, rt) in &td.methods { indent(s, depth + 4); s.push_str(&format!("fn {}(", san(mname))); let ps: Vec<String> = if tys.is_empty() { Vec::new() } else { let mut v = vec!["self".to_string()]; v.extend(tys.iter().skip(1).map(|t| ty_str(t))); v }; s.push_str(&ps.join(", ")); s.push_str(&format!(") -> {}\n", ty_str(rt))); }
            for (mname, ps, rt, body) in &td.defaults { indent(s, depth + 4); s.push_str(&format!("fn {}(", san(mname))); s.push_str(&ps.iter().enumerate().map(|(i, (n, t))| if i == 0 && n == "self" { "self".to_string() } else { format!("{}: {}", san(n), ty_str(t)) }).collect::<Vec<_>>().join(", ")); s.push_str(&format!(") -> {} {{\n", ty_str(rt))); block(s, body, depth + 8); indent(s, depth + 4); s.push_str("}\n"); }
            indent(s, depth); s.push_str("}\n");
        }
        Item::TraitImpl { type_params, trait_name, ty, methods, .. } => {
            indent(s, depth); s.push_str("impl"); if !type_params.is_empty() { s.push('['); s.push_str(&type_params.join(", ")); s.push(']'); }
            s.push_str(&format!(" {} for {} {{\n", trait_name, ty));
            for m in methods { fn_def(s, m, depth + 4); }
            indent(s, depth); s.push_str("}\n");
        }
    }
}

fn fn_def(s: &mut String, f: &FnDef, depth: usize) {
    indent(s, depth); if f.is_pub { s.push_str("pub "); }
    s.push_str(&format!("fn {}", san(&f.name)));
    if !f.type_params.is_empty() { s.push('['); s.push_str(&f.type_params.join(", ")); s.push(']'); }
    let self_ty: Option<String> = if let Some(pos) = f.name.rfind("__") { let ty = &f.name[..pos]; if !ty.is_empty() { Some(san(ty)) } else { None } } else { None };
    let ps: Vec<String> = f.params.iter().enumerate().map(|(i, p)| { if i == 0 && p.name == "self" { return match (&p.ty, &self_ty) { (Some(t), _) if !matches!(t, Ty::Unknown) => format!("self: {}", ty_str(t)), (_, Some(st)) => format!("self: {}", st), _ => "self".to_string() }; } let mut x = san(&p.name); if let Some(t) = &p.ty { if !matches!(t, Ty::Closure(..)) { x.push_str(&format!(": {}", ty_str(t))); } } if let Some(d) = &p.default { let mut ds = String::new(); expr(&mut ds, d, 0); x.push_str(&format!(" = {}", ds)); } x }).collect();
    s.push_str(&format!("({})", ps.join(", ")));
    if let Some(rt) = &f.ret { s.push_str(&format!(" -> {}", ty_str(rt))); }
    s.push_str(" {\n");
    block(s, &f.body, depth + 4);
    indent(s, depth); s.push_str("}\n");
}

fn block(s: &mut String, b: &Block, depth: usize) {
    for st in b { indent(s, depth); stmt(s, st, depth); s.push('\n'); }
}

fn stmt(s: &mut String, st: &Stmt, depth: usize) {
    match st {
        Stmt::Let { name, ty, value, mutable, .. } => {
            if *mutable { s.push_str("let mut "); } else { s.push_str("let "); }
            s.push_str(name); s.push_str(&opt_ty(ty)); s.push_str(" = "); expr(s, value, depth);
        }
        Stmt::Assign { name, index, op, value, .. } => {
            s.push_str(name);
            if let Some(i) = index { s.push('['); expr(s, i, depth); s.push(']'); }
            if let Some(o) = op { s.push_str(&format!(" {}=", o.sym())); } else { s.push_str(" ="); }
            s.push(' '); expr(s, value, depth);
        }
        Stmt::FieldAssign { obj, field, op, value, .. } => {
            s.push_str(&format!("{}.{}", obj, field));
            if let Some(o) = op { s.push_str(&format!(" {}=", o.sym())); } else { s.push_str(" ="); }
            s.push(' '); expr(s, value, depth);
        }
        Stmt::Expr(e) => expr(s, e, depth),
        Stmt::Defer(e, _) => { s.push_str("defer "); expr(s, e, depth); }
        Stmt::If { cond, then, els, .. } => {
            s.push_str("if "); expr(s, cond, depth); s.push_str(" {\n");
            block(s, then, depth + 4);
            indent(s, depth);
            if let Some(e) = els {
                s.push_str("} else {\n"); block(s, e, depth + 4); indent(s, depth);
            }
            s.push('}');
        }
        Stmt::While { cond, body, .. } => { s.push_str("while "); expr(s, cond, depth); s.push_str(" {\n"); block(s, body, depth + 4); indent(s, depth); s.push('}'); }
        Stmt::DoWhile { body, cond, .. } => { s.push_str("do {\n"); block(s, body, depth + 4); indent(s, depth); s.push_str("} while "); expr(s, cond, depth); }
        Stmt::ForRange { var, from, to, body, els, .. } => {
            s.push_str(&format!("for {} in ", var)); expr(s, from, depth); s.push_str(".."); expr(s, to, depth); s.push_str(" {\n");
            block(s, body, depth + 4); indent(s, depth);
            if let Some(e) = els { s.push_str("} else {\n"); block(s, e, depth + 4); indent(s, depth); }
            s.push('}');
        }
        Stmt::ForEach { var, iter, body, els, .. } => {
            s.push_str(&format!("for {} in ", var)); expr(s, iter, depth); s.push_str(" {\n");
            block(s, body, depth + 4); indent(s, depth);
            if let Some(e) = els { s.push_str("} else {\n"); block(s, e, depth + 4); indent(s, depth); }
            s.push('}');
        }
        Stmt::Return(v, _) => { s.push_str("return"); if let Some(e) = v { s.push(' '); expr(s, e, depth); } }
        Stmt::Labeled { label, inner, .. } => { s.push_str(&format!("{}: ", label)); stmt(s, inner, depth); }
        Stmt::Break(l, _) => { s.push_str("break"); if let Some(l) = l { s.push_str(&format!(" {}", l)); } }
        Stmt::Continue(l, _) => { s.push_str("continue"); if let Some(l) = l { s.push_str(&format!(" {}", l)); } }
        Stmt::Block(inner) => { s.push_str("{\n"); block(s, inner, depth + 4); indent(s, depth); s.push('}'); }
        Stmt::LocalFn(fd) => fn_def(s, fd, depth),
        Stmt::Const { name, ty, value, .. } => { s.push_str(&format!("const {}{} = ", name, opt_ty(ty))); expr(s, value, depth); }
        Stmt::Go { func, args, .. } => { s.push_str(&format!("go {}(", func)); let a: Vec<String> = args.iter().map(|e| { let mut t = String::new(); expr(&mut t, e, 0); t }).collect(); s.push_str(&a.join(", ")); s.push(')'); }
        Stmt::Try { body, catches, fin, .. } => {
            s.push_str("try {\n"); block(s, body, depth + 4); indent(s, depth);
            for ca in catches {
                s.push_str("} expt");
                if let Some(b) = &ca.binding { s.push_str(&format!(" {}", b)); }
                if let Some(l) = &ca.label { s.push_str(&format!(" {}", l)); }
                if let Some(g) = &ca.guard { s.push_str(" if "); expr(s, g, depth); }
                s.push_str(" {\n"); block(s, &ca.body, depth + 4); indent(s, depth);
            }
            if let Some(f) = fin { s.push_str("} fily {\n"); block(s, f, depth + 4); indent(s, depth); }
            s.push('}');
        }
        Stmt::Throw(e, _) => { s.push_str("throw "); expr(s, e, depth); }
        Stmt::Asm { lines, .. } => { s.push_str("asm {\n"); for l in lines { indent(s, depth + 4); s.push_str(l); s.push('\n'); } indent(s, depth); s.push('}'); }
    }
}

fn expr(s: &mut String, e: &Expr, depth: usize) {
    match &e.kind {
        ExprKind::Int(v) => s.push_str(&format!("{}", v)),
        ExprKind::Float(v) => { let t = format!("{:?}", v); s.push_str(&t); if !t.contains('.') && !t.contains('e') { s.push_str(".0"); } }
        ExprKind::Decimal(t) => { s.push_str(t); s.push('d'); }
        ExprKind::Bool(b) => s.push_str(if *b { "true" } else { "false" }),
        ExprKind::Str(t) => { s.push('"'); for c in t.chars() { match c { '"' => s.push_str("\\\""), '\\' => s.push_str("\\\\"), '\n' => s.push_str("\\n"), '\t' => s.push_str("\\t"), '$' => s.push_str("\\$"), _ => s.push(c) } } s.push('"'); }
        ExprKind::Ident(n) => s.push_str(&san(n)),
        ExprKind::Unary(op, a) => { s.push('('); s.push_str(match op { UnOp::Neg => "-", UnOp::Not => "!", UnOp::BitNot => "~" }); expr(s, a, depth); s.push(')'); }
        ExprKind::Binary(op, a, b) => { s.push('('); expr(s, a, depth); s.push(' '); s.push_str(op.sym()); s.push(' '); expr(s, b, depth); s.push(')'); }
        ExprKind::Call(name, args) => { s.push_str(&san(name)); s.push('('); args_list(s, args, depth); s.push(')'); }
        ExprKind::CallNamed(name, named) => { s.push_str(&san(name)); s.push('('); let xs: Vec<String> = named.iter().map(|(n, v)| { let mut t = String::new(); expr(&mut t, v, 0); format!("{}: {}", n, t) }).collect(); s.push_str(&xs.join(", ")); s.push(')'); }
        ExprKind::Index(a, b) => { expr(s, a, depth); s.push('['); expr(s, b, depth); s.push(']'); }
        ExprKind::Slice(a, lo, hi) => { expr(s, a, depth); s.push('['); expr(s, lo, depth); s.push_str(".."); expr(s, hi, depth); s.push(']'); }
        ExprKind::ArrayLit(xs) => { s.push('['); args_list(s, xs, depth); s.push(']'); }
        ExprKind::TupleLit(xs) => { s.push('('); args_list(s, xs, depth); if xs.len() == 1 { s.push(','); } s.push(')'); }
        ExprKind::ListComp { expr: ex, var, iter, cond } => { s.push('['); expr(s, ex, depth); s.push_str(&format!(" for {} in ", var)); expr(s, iter, depth); if let Some(c) = cond { s.push_str(" if "); expr(s, c, depth); } s.push(']'); }
        ExprKind::Field(base, f) => { expr(s, base, depth); s.push('.'); s.push_str(f); }
        ExprKind::StructLit(name, fs) => { s.push_str(name); s.push_str(" {"); let xs: Vec<String> = fs.iter().map(|(n, v)| { let mut t = String::new(); expr(&mut t, v, 0); format!("{}: {}", n, t) }).collect(); s.push_str(&xs.join(", ")); s.push('}'); }
        ExprKind::EnumLit(ty, var, args) => { s.push_str(&format!("{}::{}", san(ty), san(var))); if !args.is_empty() { s.push('('); args_list(s, args, depth); s.push(')'); } }
        ExprKind::DynBox { trait_name, value } => { s.push_str(&format!("dyn {}(", trait_name)); expr(s, value, depth); s.push(')'); }
        ExprKind::If { cond, then, els } => { s.push_str("if "); expr(s, cond, depth); s.push_str(" {\n"); block(s, then, depth + 4); indent(s, depth); if let Some(e) = els { s.push_str("} else {\n"); block(s, e, depth + 4); indent(s, depth); } s.push('}'); }
        ExprKind::Match { subject, arms } => { s.push_str("match "); expr(s, subject, depth); s.push_str(" {\n"); for arm in arms { indent(s, depth + 4); if let Some(p) = &arm.pat { expr(s, p, depth + 4); } else if let Some((lo, hi)) = &arm.range { expr(s, lo, depth + 4); s.push_str(".."); expr(s, hi, depth + 4); } else { s.push('_'); } if let Some(g) = &arm.guard { s.push_str(" if "); expr(s, g, depth + 4); } s.push_str(" => {\n"); block(s, &arm.body, depth + 8); indent(s, depth + 4); s.push_str("}\n"); } indent(s, depth); s.push('}'); }
        ExprKind::Closure { params, param_tys, ret_ty, body, .. } => { s.push('|'); let xs: Vec<String> = params.iter().enumerate().map(|(i, p)| { match param_tys.get(i).and_then(|t| t.as_ref()) { Some(t) => format!("{}: {}", p, ty_str(t)), None => p.clone() } }).collect(); s.push_str(&xs.join(", ")); s.push('|'); if let Some(rt) = ret_ty { s.push_str(&format!(" -> {} ", ty_str(rt))); } expr(s, body, depth); }
        ExprKind::CallValue { callee, args } => { expr(s, callee, depth); s.push('('); args_list(s, args, depth); s.push(')'); }
        ExprKind::MethodOn { recv, method, args } => { expr(s, recv, depth); s.push('.'); s.push_str(&san(method)); s.push('('); args_list(s, args, depth); s.push(')'); }
        ExprKind::ClosureNew { fn_name, captures } => {
            // 无捕获闭包 = 函数作一等值：直接打印函数名（可再编译）。
            if captures.is_empty() { s.push_str(&san(fn_name)); }
            else { s.push_str(&format!("{}(", san(fn_name))); args_list(s, captures, depth); s.push(')'); }
        }
        ExprKind::Borrow { mutable, inner } => { if *mutable { s.push_str("&mut ") } else { s.push('&') } expr(s, inner, depth); }
        ExprKind::Ok(v) => { s.push_str("Ok("); expr(s, v, depth); s.push(')'); }
        ExprKind::Err(v) => { s.push_str("Err("); expr(s, v, depth); s.push(')'); }
        ExprKind::Try(v) => { expr(s, v, depth); s.push('?'); }
        ExprKind::Some(v) => { s.push_str("Some("); expr(s, v, depth); s.push(')'); }
        ExprKind::None => s.push_str("None"),
        ExprKind::TryBlock { body, catches, fin } => { s.push_str("try {\n"); block(s, body, depth + 4); indent(s, depth); for ca in catches { s.push_str("} expt"); if let Some(b) = &ca.binding { s.push_str(&format!(" {}", b)); } if let Some(l) = &ca.label { s.push_str(&format!(" {}", l)); } if let Some(g) = &ca.guard { s.push_str(" if "); expr(s, g, depth); } s.push_str(" {\n"); block(s, &ca.body, depth + 4); indent(s, depth); } if let Some(f) = fin { s.push_str("} fily {\n"); block(s, f, depth + 4); indent(s, depth); } s.push('}'); }
        ExprKind::Comptime(b) => { s.push_str("comptime {\n"); block(s, b, depth + 4); indent(s, depth); s.push('}'); }
        ExprKind::Interp(parts) => { let mut flat: Vec<StrPart> = Vec::new(); flatten_parts(parts, &mut flat); s.push('"'); for p in &flat { match p { StrPart::Lit(t) => for c in t.chars() { match c { '"' => s.push_str("\\\""), '\\' => s.push_str("\\\\"), '\n' => s.push_str("\\n"), '\t' => s.push_str("\\t"), '$' => s.push_str("\\$"), _ => s.push(c) } }, StrPart::Expr(x) => { s.push_str("${"); let mut t = String::new(); expr(&mut t, x, depth); for c in t.chars() { match c { '"' => s.push_str("\\\""), '\\' => s.push_str("\\\\"), _ => s.push(c) } } s.push('}'); } } } s.push('"'); }
    }
}

fn flatten_parts(parts: &[StrPart], out: &mut Vec<StrPart>) {
    for p in parts {
        match p {
            StrPart::Lit(t) => out.push(StrPart::Lit(t.clone())),
            StrPart::Expr(x) => match &x.kind {
                ExprKind::Interp(inner) => flatten_parts(inner, out),
                ExprKind::Str(t) => out.push(StrPart::Lit(t.clone())),
                _ => out.push(StrPart::Expr(x.clone())),
            },
        }
    }
}

fn args_list(s: &mut String, xs: &[Expr], depth: usize) {
    let parts: Vec<String> = xs.iter().map(|e| { let mut t = String::new(); expr(&mut t, e, depth); t }).collect();
    s.push_str(&parts.join(", "));
}
