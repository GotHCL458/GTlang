//! Codegen 的调用/转换/辅助方法。

use super::*;

impl<'a> Codegen<'a> {

    // ---------- 低层辅助 ----------
    pub(crate) fn new_reg(&mut self) -> String {
        let r = format!("%t{}", self.reg);
        self.reg += 1;
        r
    }
    pub(crate) fn new_label(&mut self) -> String {
        let l = format!("L{}", self.label);
        self.label += 1;
        l
    }
    pub(crate) fn emit_label(&mut self, l: &str) {
        let trimmed = self.body.trim_end(); if trimmed.ends_with(":") { self.body.push_str(&format!("  br label %{}\n", l)); } let trimmed = self.body.trim_end(); if trimmed.ends_with(":") { self.body.push_str(&format!("  br label %{}\n", l)); } self.terminated = false; self.body.push_str(&format!("{}:\n", l));
        // 新基本块：块内 SSA 缓存失效（perm_cache 跨块，保留）
        self.var_cache.clear();
    }
    pub(crate) fn new_alloca(&mut self, ty: &Ty) -> String {
        let slot = format!("%v{}", self.slot);
        self.slot += 1;
        self.entry_allocas.push(format!("  {} = alloca {}", slot, ty.llvm()));
        slot
    }
    pub(crate) fn new_alloca_raw(&mut self, lty: &str) -> String {
        let slot = format!("%v{}", self.slot);
        self.slot += 1;
        self.entry_allocas.push(format!("  {} = alloca {}", slot, lty));
        slot
    }
    pub(crate) fn declare(&mut self, d: &str) {
        self.declares.insert(d.to_string());
    }
    pub(crate) fn intern(&mut self, bytes: &[u8]) -> String {
        if let Some(g) = self.string_cache.get(bytes) { return g.clone(); }
        let name = format!("@.str{}", self.string_cache.len());
        self.string_cache.insert(bytes.to_vec(), name.clone());
        let mut s = String::new();
        s.push_str(&format!("{} = private unnamed_addr constant [{} x i8] c\"", name, bytes.len() + 1));
        s.push_str(&escape_bytes(bytes));
        s.push_str("\\00\"\n");
        self.globals.push_str(&s);
        name
    }
    pub(crate) fn coerce(&mut self, v: &Val, to: &Ty) -> Result<Val, String> {
        // 形态对齐：类型相同但 LLVM 表示（ptr / slot）不一致时也要转换。
        // 容器/字符串/结构体在 `to` 侧期望 ptr；若 v 是 slot 形态，先 inttoptr。
        if v.ty == *to {
            let want_ptr = to.llvm() == "ptr";
            if want_ptr && !v.is_ptr {
                let p = self.as_ptr(v);
                return Ok(Val::new_ptr(to, p));
            }
            if !want_ptr && v.is_ptr {
                let s = self.to_slot(v);
                return Ok(Val::new_slot(to, s));
            }
            return Ok(v.clone());
        }
        if to == &Ty::Unknown || v.ty == Ty::Unknown { return Ok(v.clone()); }
        match (to, &v.ty) {
            (Ty::F64, t) if t.is_int() => { let r = self.new_reg(); self.body.push_str(&format!("  {} = sitofp i64 {} to double\n", r, v.s)); Ok(Val::new(&Ty::F64, r)) }
            (t, Ty::F64) if t.is_int() => { let r = self.new_reg(); self.body.push_str(&format!("  {} = fptosi double {} to i64\n", r, v.s)); Ok(Val::new(&Ty::I64, r)) }
            _ => Ok(v.clone()),
        }
    }
    pub(crate) fn as_i64(&mut self, v: &Val) -> String {
        match v.ty {
            Ty::I64 => v.s.clone(),
            Ty::Bool => { let r = self.new_reg(); self.body.push_str(&format!("  {} = zext i1 {} to i64\n", r, v.s)); r }
            Ty::F64 => { let r = self.new_reg(); self.body.push_str(&format!("  {} = fptosi double {} to i64\n", r, v.s)); r }
            // 值是 LLVM ptr 时做 ptrtoint；已是 i64（slot）则原样返回
            _ if v.is_ptr => {
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = ptrtoint ptr {} to i64\n", r, v.s));
                r
            }
            _ => v.s.clone(),
        }
    }
    /// 确保 Val 是 LLVM `ptr` 值：若已是 ptr 直接返回，否则 inttoptr（容器句柄常以 i64 流转）。
    pub(crate) fn as_ptr(&mut self, v: &Val) -> String {
        if v.is_ptr { return v.s.clone(); }
        // 不变式：类型是 ptr 类却标成 slot，说明上游构造有误（本可 inttoptr，但多半是 bug）。
        debug_assert!(
            v.ty.llvm() != "ptr" || matches!(v.ty, Ty::Unknown),
            "as_ptr: 类型 {:?} 是 ptr 类但值被标为 slot —— 上游应已转成 ptr 形态",
            v.ty
        );
        let r = self.new_reg();
        self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", r, v.s));
        r
    }
    pub(crate) fn to_slot(&mut self, v: &Val) -> String {
        if v.is_ptr { let r = self.new_reg(); self.body.push_str(&format!("  {} = ptrtoint ptr {} to i64\n", r, v.s)); r }
        else if v.ty == Ty::F64 { let r = self.new_reg(); self.body.push_str(&format!("  {} = bitcast double {} to i64\n", r, v.s)); r }
        else if v.ty == Ty::Bool { let r = self.new_reg(); self.body.push_str(&format!("  {} = zext i1 {} to i64\n", r, v.s)); r }
        else { v.s.clone() }
    }
    pub(crate) fn from_slot(&mut self, s: &str, ty: &Ty) -> String {
        match ty {
            Ty::Str | Ty::List(..) | Ty::Set(..) | Ty::Map(..) | Ty::Array(..) | Ty::Struct(_) | Ty::Dyn(_) | Ty::Enum(_) | Ty::Tuple(_) | Ty::Closure(..) => { let r = self.new_reg(); self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", r, s)); r }
            Ty::F64 => { let r = self.new_reg(); self.body.push_str(&format!("  {} = bitcast i64 {} to double\n", r, s)); r }
            Ty::Bool => { let r = self.new_reg(); self.body.push_str(&format!("  {} = trunc i64 {} to i1\n", r, s)); r }
            _ => s.to_string(),
        }
    }
    pub(crate) fn lookup(&self, n: &str) -> Option<Local> {
        for sc in self.scopes.iter().rev() {
            if let Some(l) = sc.get(n) { return Some(l.clone()); }
        }
        None
    }
    pub(crate) fn load(&mut self, loc: &Local) -> Result<Val, String> {
        // 1) 本基本块内已存的值（var_cache，emit_label 时清空）
        if let Some(v) = self.var_cache.get(&loc.ptr) { return Ok(v.clone()); }
        // 2) immutable 变量（声明后从不重新赋值）：跨基本块保持其唯一 SSA 值，
        //    不必每个分支都重新 load —— 也让 IR 更接近 SSA，便于 LLVM 优化。
        if self.perm_ptrs.contains(&loc.ptr) {
            if let Some(v) = self.perm_cache.get(&loc.ptr) { return Ok(v.clone()); }
        }
        let r = self.new_reg();
        self.body.push_str(&format!("  {} = load {}, ptr {}\n", r, loc.ty.llvm(), loc.ptr));
        Ok(Val::new(&loc.ty, r))
    }
    pub(crate) fn store(&mut self, loc: &Local, v: &Val) -> Result<(), String> {
        let v = self.coerce(v, &loc.ty)?;
        self.var_cache.insert(loc.ptr.clone(), v.clone());
        if self.perm_ptrs.contains(&loc.ptr) {
            self.perm_cache.insert(loc.ptr.clone(), v.clone());
        }
        self.body.push_str(&format!("  store {} {}, ptr {}\n", loc.ty.llvm(), v.s, loc.ptr));
        Ok(())
    }
    pub(crate) fn cond(&mut self, e: &Expr) -> Result<String, String> {
        let v = self.expr(e)?;
        let iv = self.as_i64(&v);
        let r = self.new_reg();
        self.body.push_str(&format!("  {} = icmp ne i64 {}, 0\n", r, iv));
        Ok(r)
    }
    pub(crate) fn emit_print(&mut self, e: &Expr, newline: bool) -> Result<(), String> {
        self.emit_print_val(e)?;
        if newline { let f = self.intern(b"\n"); self.declare("declare i32 @gt_printf(ptr, ...)"); self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {})\n", f)); }
        Ok(())
    }

    /// 打印一个表达式（求值后按类型递归格式化）。
    pub(crate) fn emit_print_val(&mut self, e: &Expr) -> Result<(), String> {
        let v = self.expr(e)?;
        let ty = v.ty.clone();
        self.emit_print_v(&v, &ty)
    }

    /// 打印一个字面量字符串片段。
    fn emit_puts_lit(&mut self, s: &str) {
        let g = self.intern(s.as_bytes());
        self.declare("declare i32 @gt_printf(ptr, ...)");
        self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {})\n", g));
    }

    /// 按类型打印一个已算好的值（支持递归：容器元素按其 `Ty` 分派）。
    pub(crate) fn emit_print_v(&mut self, v: &Val, ty: &Ty) -> Result<(), String> {
        self.declare("declare i32 @gt_printf(ptr, ...)");
        match ty {
            Ty::F64 => { let f = self.intern(b"%g"); self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {}, double {})\n", f, v.s)); }
            Ty::Str => { let f = self.intern(b"%s"); self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {}, ptr {})\n", f, v.s)); }
            Ty::Bool => {
                let t = self.intern(b"true"); let fa = self.intern(b"false");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = select i1 {}, ptr {}, ptr {}\n", r, v.s, t, fa));
                let f = self.intern(b"%s");
                self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {}, ptr {})\n", f, r));
            }
            Ty::Array(elem, n) => {
                self.emit_puts_lit("[");
                let n = *n;
                for i in 0..n {
                    if i > 0 { self.emit_puts_lit(", "); }
                    let ptr = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr inbounds [{} x i64], ptr {}, i64 0, i64 {}\n", ptr, n, v.s, i));
                    let ev = self.new_reg();
                    self.body.push_str(&format!("  {} = load i64, ptr {}\n", ev, ptr));
                    let sub = Val::new_slot(elem, ev);
                    self.emit_print_v(&sub, elem)?;
                }
                self.emit_puts_lit("]");
            }
            Ty::List(elem) | Ty::Set(elem) => {
                let is_set = matches!(ty, Ty::Set(_));
                self.emit_puts_lit(if is_set { "{" } else { "[" });
                self.declare("declare i64 @gt_list_len(i64)");
                self.declare("declare i64 @gt_list_at(i64, i64)");
                let vslot = self.as_i64(v);
                let n = self.new_reg();
                self.body.push_str(&format!("  {} = call i64 @gt_list_len(i64 {})\n", n, vslot));
                let i = self.new_alloca(&Ty::I64);
                self.body.push_str(&format!("  store i64 0, ptr {}\n", i));
                let lt = self.new_label(); let le = self.new_label(); let lc = self.new_label();
                self.body.push_str(&format!("  br label %{}\n", lt));
                self.emit_label(&lt);
                let iv = self.new_reg();
                self.body.push_str(&format!("  {} = load i64, ptr {}\n", iv, i));
                let cnd = self.new_reg();
                self.body.push_str(&format!("  {} = icmp slt i64 {}, {}\n", cnd, iv, n));
                self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", cnd, lc, le));
                self.emit_label(&lc);
                let pos = self.new_reg();
                self.body.push_str(&format!("  {} = icmp sgt i64 {}, 0\n", pos, iv));
                let l_sep = self.new_label(); let l_nosep = self.new_label();
                self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", pos, l_sep, l_nosep));
                self.emit_label(&l_sep);
                self.emit_puts_lit(", ");
                self.body.push_str(&format!("  br label %{}\n", l_nosep));
                self.emit_label(&l_nosep);
                let ev = self.new_reg();
                self.body.push_str(&format!("  {} = call i64 @gt_list_at(i64 {}, i64 {})\n", ev, vslot, iv));
                let sub = Val::new_slot(elem, ev);
                self.emit_print_v(&sub, elem)?;
                let nx = self.new_reg();
                self.body.push_str(&format!("  {} = add i64 {}, 1\n", nx, iv));
                self.body.push_str(&format!("  store i64 {}, ptr {}\n", nx, i));
                self.body.push_str(&format!("  br label %{}\n", lt));
                self.emit_label(&le);
                self.emit_puts_lit(if is_set { "}" } else { "]" });
            }
            Ty::Struct(sname) => {
                let fields = self.structs.get(sname).cloned().unwrap_or_default();
                self.emit_puts_lit(&format!("{} {{", sname));
                let sp = self.as_ptr(v);
                for (i, (fname, fty)) in fields.iter().enumerate() {
                    if i > 0 { self.emit_puts_lit(", "); }
                    self.emit_puts_lit(&format!("{}: ", fname));
                    let fp = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr i8, ptr {}, i64 {}\n", fp, sp, i * 8));
                    let fv = self.new_reg();
                    self.body.push_str(&format!("  {} = load i64, ptr {}\n", fv, fp));
                    let sub = Val::new_slot(fty, fv);
                    self.emit_print_v(&sub, fty)?;
                }
                self.emit_puts_lit("}");
            }
            Ty::Map(_, _) => {
                self.emit_puts_lit("{");
                self.declare("declare i64 @gt_map_len(i64)");
                self.declare("declare i64 @gt_map_key_at(i64, i64)");
                self.declare("declare i64 @gt_map_val_at(i64, i64)");
                let vslot = self.as_i64(v);
                let n = self.new_reg();
                self.body.push_str(&format!("  {} = call i64 @gt_map_len(i64 {})\n", n, vslot));
                let i = self.new_alloca(&Ty::I64);
                self.body.push_str(&format!("  store i64 0, ptr {}\n", i));
                let lt = self.new_label(); let le = self.new_label(); let lc = self.new_label();
                self.body.push_str(&format!("  br label %{}\n", lt));
                self.emit_label(&lt);
                let iv = self.new_reg();
                self.body.push_str(&format!("  {} = load i64, ptr {}\n", iv, i));
                let cnd = self.new_reg();
                self.body.push_str(&format!("  {} = icmp slt i64 {}, {}\n", cnd, iv, n));
                self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", cnd, lc, le));
                self.emit_label(&lc);
                let pos = self.new_reg();
                self.body.push_str(&format!("  {} = icmp sgt i64 {}, 0\n", pos, iv));
                let l_sep = self.new_label(); let l_nosep = self.new_label();
                self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", pos, l_sep, l_nosep));
                self.emit_label(&l_sep);
                self.emit_puts_lit(", ");
                self.body.push_str(&format!("  br label %{}\n", l_nosep));
                self.emit_label(&l_nosep);
                let kv = self.new_reg();
                self.body.push_str(&format!("  {} = call i64 @gt_map_key_at(i64 {}, i64 {})\n", kv, vslot, iv));
                let kf = self.intern(b"%lld: ");
                self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {}, i64 {})\n", kf, kv));
                let vv = self.new_reg();
                self.body.push_str(&format!("  {} = call i64 @gt_map_val_at(i64 {}, i64 {})\n", vv, vslot, iv));
                let vf = self.intern(b"%lld");
                self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {}, i64 {})\n", vf, vv));
                let nx = self.new_reg();
                self.body.push_str(&format!("  {} = add i64 {}, 1\n", nx, iv));
                self.body.push_str(&format!("  store i64 {}, ptr {}\n", nx, i));
                self.body.push_str(&format!("  br label %{}\n", lt));
                self.emit_label(&le);
                self.emit_puts_lit("}");
            }
            Ty::Option(_) => {
                let sp = self.as_ptr(v);
                let tag = self.new_reg();
                self.body.push_str(&format!("  {} = load i64, ptr {}\n", tag, sp));
                let isz = self.new_reg();
                self.body.push_str(&format!("  {} = icmp eq i64 {}, 0\n", isz, tag));
                let ls = self.new_label(); let ln = self.new_label(); let lend = self.new_label();
                self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", isz, ls, ln));
                self.emit_label(&ls);
                self.emit_puts_lit("Some(");
                let pv = self.new_reg();
                self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 1\n", pv, sp));
                let pvv = self.new_reg();
                self.body.push_str(&format!("  {} = load i64, ptr {}\n", pvv, pv));
                let pf = self.intern(b"%lld");
                self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {}, i64 {})\n", pf, pvv));
                self.emit_puts_lit(")");
                self.body.push_str(&format!("  br label %{}\n", lend));
                self.emit_label(&ln);
                self.emit_puts_lit("None");
                self.body.push_str(&format!("  br label %{}\n", lend));
                self.emit_label(&lend);
            }
            _ => {
                let iv = self.as_i64(v);
                let f = self.intern(b"%lld");
                self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {}, i64 {})\n", f, iv));
            }
        }
        Ok(())
    }

    pub(crate) fn push_scope(&mut self) { self.scopes.push(HashMap::new()); }
    pub(crate) fn pop_scope(&mut self) { self.scopes.pop(); }
    pub(crate) fn const_val(&mut self, c: &ConstVal) -> Val {
        match &c.val {
            CVal::Int(i) => Val::new(&Ty::I64, i.to_string()),
            CVal::Float(f) => Val::new(&Ty::F64, fmt_double(*f)),
            CVal::Bool(v) => Val::new(&Ty::Bool, if *v { "true" } else { "false" }),
            CVal::Str(s) => Val::new(&Ty::Str, self.intern(s.as_bytes())),
        }
    }
    pub(crate) fn llvm_sym(name: &str) -> String {
        if name.chars().all(|c| c.is_ascii() && (c.is_alphanumeric() || c == '_')) { name.to_string() }
        else { format!("\"{name}\"") }
    }
}

pub(crate) fn mangle(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'_' { out.push(b as char); }
        else { out.push_str(&format!("_x{:02X}", b)); }
    }
    out
}

pub(crate) fn escape_bytes(bytes: &[u8]) -> String {
    let mut s = String::new();
    for &b in bytes {
        match b {
            b'"' => s.push_str("\\22"),
            b'\\' => s.push_str("\\5C"),
            0x20..=0x7E => s.push(b as char),
            _ => s.push_str(&format!("\\{:02X}", b)),
        }
    }
    s
}

pub(crate) fn fmt_double(v: f64) -> String {
    if v.is_nan() { return "0x7FF8000000000000".into(); }
    if v.is_infinite() { return if v > 0.0 { "0x7FF0000000000000".into() } else { "0xFFF0000000000000".into() }; }
    format!("0x{:016X}", v.to_bits())
}

pub(crate) fn is_pure(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Ident(_) => true,
        ExprKind::Unary(_, a) => is_pure(a),
        ExprKind::Binary(_, a, b) => is_pure(a) && is_pure(b),
        ExprKind::ArrayLit(items) => items.iter().all(is_pure),
        ExprKind::Index(a, b) => is_pure(a) && is_pure(b),
        ExprKind::Field(base, _) => is_pure(base),
        ExprKind::StructLit(_, fs) => fs.iter().all(|(_, v)| is_pure(v)),
        _ => false,
    }
}

pub(crate) fn immutable_vars(b: &Block) -> std::collections::HashSet<String> {
    let mut assigned = std::collections::HashSet::new();
    let mut declared = std::collections::HashSet::new();
    scan_mutation(b, false, &mut assigned, &mut declared);
    declared.difference(&assigned).cloned().collect()
}

pub(crate) fn scan_mutation(b: &Block, in_loop: bool, assigned: &mut std::collections::HashSet<String>, declared: &mut std::collections::HashSet<String>) {
    for s in b {
        // 只在本层识别"声明/赋值"；子块递归交给 Stmt::each_block（遍历驱动，
        // 新增含块变体只需在 each_block 补一处，这里不会漏分支）。
        let loop_like = matches!(s, Stmt::While { .. } | Stmt::DoWhile { .. } | Stmt::ForRange { .. } | Stmt::ForEach { .. });
        match s {
            Stmt::Let { name, .. } => { if !in_loop { declared.insert(name.clone()); } }
            Stmt::Assign { name, .. } => { assigned.insert(name.clone()); }
            Stmt::FieldAssign { obj, .. } => { assigned.insert(obj.clone()); }
            _ => {}
        }
        let child_in_loop = in_loop || loop_like;
        s.each_block(&mut |blk| scan_mutation(blk, child_in_loop, assigned, declared));
    }
}

pub(crate) fn push_fmt_literal(fmt: &mut Vec<u8>, s: &[u8]) {
    for &b in s {
        if b == b'%' { fmt.push(b'%'); }
        fmt.push(b);
    }
}

impl<'a> Codegen<'a> {
    /// 构造 printf 格式串 + 实参（用于插值字符串作为值）
    pub(crate) fn fmt_of(&mut self, e: &Expr) -> Result<(Vec<u8>, Vec<(Ty, String)>), String> {
        let mut fmt: Vec<u8> = Vec::new();
        let mut args: Vec<(Ty, String)> = Vec::new();
        match &e.kind {
            ExprKind::Interp(parts) => {
                for p in parts {
                    match p {
                        StrPart::Lit(s) => push_fmt_literal(&mut fmt, s.as_bytes()),
                        StrPart::Expr(inner) => { let (spec, op) = self.printf_arg(inner)?; fmt.extend_from_slice(spec.as_bytes()); args.push(op); }
                    }
                }
            }
            _ => { let (spec, op) = self.printf_arg(e)?; fmt.extend_from_slice(spec.as_bytes()); args.push(op); }
        }
        Ok((fmt, args))
    }
    pub(crate) fn printf_arg(&mut self, e: &Expr) -> Result<(String, (Ty, String)), String> {
        let v = self.expr(e)?;
        match v.ty {
            Ty::F64 => Ok(("%g".into(), (Ty::F64, v.s))),
            Ty::Str => Ok(("%s".into(), (Ty::Str, v.s))),
            Ty::Bool => { let t = self.intern(b"true"); let f = self.intern(b"false"); let r = self.new_reg(); self.body.push_str(&format!("  {} = select i1 {}, ptr {}, ptr {}\n", r, v.s, t, f)); Ok(("%s".into(), (Ty::Str, r))) }
            _ => Ok(("%lld".into(), (Ty::I64, v.s))),
        }
    }

    /// 构造 trait 对象：堆块 [data, addr0, addr1, ...]（vtable 内联）。
    pub(crate) fn dyn_box(&mut self, trait_name: &str, value: &Expr, e: &Expr) -> Result<Val, String> {
        let v = self.expr(value)?;
        let conc = match &v.ty {
            Ty::Struct(n) => n.clone(),
            Ty::Enum(n) => n.clone(),
            _ => String::new(),
        };
        let methods = self.trait_impls.get(&(conc.clone(), trait_name.to_string())).cloned().unwrap_or_default();
        let n = methods.len() + 1;
        self.declare("declare ptr @gt_mem_alloc(i64)");
        let base = self.new_reg();
        self.body.push_str(&format!("  {} = call ptr @gt_mem_alloc(i64 {})\n", base, n * 8));
        // data 槽：struct/enum 是 ptr，需 ptrtoint 成 i64
        let data_i = if v.ty.llvm() == "ptr" {
            let r = self.new_reg();
            self.body.push_str(&format!("  {} = ptrtoint ptr {} to i64\n", r, v.s));
            r
        } else {
            self.to_slot(&v)
        };
        let dp = self.new_reg();
        self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 0\n", dp, base));
        self.body.push_str(&format!("  store i64 {}, ptr {}\n", data_i, dp));
        for (i, mn) in methods.iter().enumerate() {
            let addr = if mn.is_empty() { "0".to_string() } else {
                let cname = self.fns.get(mn).map(|i| i.cname.clone());
                match cname {
                    Some(cn) => {
                        let r = self.new_reg();
                        self.body.push_str(&format!("  {} = ptrtoint ptr @{} to i64\n", r, cn));
                        r
                    }
                    None => "0".to_string(),
                }
            };
            let p = self.new_reg();
            self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 {}\n", p, base, i + 1));
            self.body.push_str(&format!("  store i64 {}, ptr {}\n", addr, p));
        }
        Ok(Val::new(&e.ty.clone(), base))
    }
}


