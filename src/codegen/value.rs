//! Codegen 的值转换与控制流（if/match/eq）。

use super::*;

impl<'a> Codegen<'a> {
    /// 转字符串。注意：返回的 buf 是**当前函数的栈数组**，
    /// 在循环内多次调用会各自 alloca（LLVM 可能复用栈槽），
    /// 因此**不要把多次 `str()` 的返回值长期保存**（如 push 进 list 后再用）。
    /// 需要长期保存时，先在循环外拼接或改用插值。
    pub(crate) fn to_str(&mut self, v: &Val, _line: usize) -> Result<Val, String> {
        let fmt = match v.ty {
            Ty::F64 => "%g",
            Ty::Bool => {
                let t = self.intern(b"true");
                let f = self.intern(b"false");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = select i1 {}, ptr {}, ptr {}\n", r, v.s, t, f));
                self.declare("declare i32 @gt_sprintf(ptr, i64, ptr, ...)");
                let buf = self.new_alloca_raw(&format!("[{} x i8]", INTERP_BUF));
                let sv = self.new_reg();
                let fmts = self.intern(b"%s"); self.body.push_str(&format!("  {} = call i32 (ptr, i64, ptr, ...) @gt_sprintf(ptr {}, i64 {}, ptr {}, ptr {})\n", sv, buf, INTERP_BUF, fmts, r));
                return Ok(Val::new(&Ty::Str, buf));
            }
            _ => "%lld",
        };
        let f = self.intern(fmt.as_bytes());
        self.declare("declare i32 @gt_sprintf(ptr, i64, ptr, ...)");
        let buf = self.new_alloca_raw(&format!("[{} x i8]", INTERP_BUF));
        let sv = self.new_reg();
        self.body.push_str(&format!("  {} = call i32 (ptr, i64, ptr, ...) @gt_sprintf(ptr {}, i64 {}, ptr {}, {} {})\n", sv, buf, INTERP_BUF, f, v.ty.llvm(), v.s));
        Ok(Val::new(&Ty::Str, buf))
    }

    pub(crate) fn to_i64(&mut self, v: &Val, _line: usize) -> Result<Val, String> {
        match v.ty {
            Ty::Str => { self.declare("declare i64 @gt_to_i64(ptr)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @gt_to_i64(ptr {})\n", r, v.s)); Ok(Val::new(&Ty::I64, r)) }
            Ty::F64 => { let r = self.new_reg(); self.body.push_str(&format!("  {} = fptosi double {} to i64\n", r, v.s)); Ok(Val::new(&Ty::I64, r)) }
            Ty::Bool => { let r = self.new_reg(); self.body.push_str(&format!("  {} = zext i1 {} to i64\n", r, v.s)); Ok(Val::new(&Ty::I64, r)) }
            _ => Ok(Val::new(&Ty::I64, v.s.clone())),
        }
    }

    pub(crate) fn to_f64(&mut self, v: &Val, _line: usize) -> Result<Val, String> {
        match v.ty {
            Ty::Str => { self.declare("declare double @gt_to_f64(ptr)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call double @gt_to_f64(ptr {})\n", r, v.s)); Ok(Val::new(&Ty::F64, r)) }
            Ty::F64 => Ok(Val::new(&Ty::F64, v.s.clone())),
            Ty::Bool => { let z = self.new_reg(); self.body.push_str(&format!("  {} = zext i1 {} to i64\n", z, v.s)); let r = self.new_reg(); self.body.push_str(&format!("  {} = sitofp i64 {} to double\n", r, z)); Ok(Val::new(&Ty::F64, r)) }
            _ => { let r = self.new_reg(); self.body.push_str(&format!("  {} = sitofp i64 {} to double\n", r, v.s)); Ok(Val::new(&Ty::F64, r)) }
        }
    }

    pub(crate) fn to_bool(&mut self, v: &Val, _line: usize) -> Result<Val, String> {
        match v.ty {
            Ty::Str => { self.declare("declare i64 @strlen(ptr)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @strlen(ptr {})\n", r, v.s)); let b = self.new_reg(); self.body.push_str(&format!("  {} = icmp ne i64 {}, 0\n", b, r)); Ok(Val::new(&Ty::Bool, b)) }
            Ty::Bool => Ok(Val::new(&Ty::Bool, v.s.clone())),
            Ty::F64 => { let z = self.new_reg(); self.body.push_str(&format!("  {} = fcmp une double {}, 0.0\n", z, v.s)); Ok(Val::new(&Ty::Bool, z)) }
            _ => { let b = self.new_reg(); self.body.push_str(&format!("  {} = icmp ne i64 {}, 0\n", b, v.s)); Ok(Val::new(&Ty::Bool, b)) }
        }
    }

    // ---------- if / match 作为值 ----------
    pub(crate) fn if_value(&mut self, cond: &Expr, then: &Block, els: Option<&Block>, want: &Ty, _line: usize) -> Result<Val, String> {
        let c = self.cond(cond)?;
        let lthen = self.new_label();
        let lelse = self.new_label();
        let lend = self.new_label();
        let slot = if want != &Ty::Void { Some(self.new_alloca(want)) } else { None };
        self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", c, lthen, lelse));
        self.emit_label(&lthen);
        self.push_scope();
        let tv = self.block_ret(then, want)?;
        self.pop_scope();
        if !self.terminated {
            if let (Some(slot), Some(v)) = (&slot, tv) { let v = self.coerce(&v, want)?; self.body.push_str(&format!("  store {} {}, ptr {}\n", want.llvm(), v.s, slot)); }
            self.body.push_str(&format!("  br label %{}\n", lend));
        }
        self.emit_label(&lelse);
        match els {
            Some(eb) => { self.push_scope(); let ev = self.block_ret(eb, want)?; self.pop_scope(); if !self.terminated { if let (Some(slot), Some(v)) = (&slot, ev) { let v = self.coerce(&v, want)?; self.body.push_str(&format!("  store {} {}, ptr {}\n", want.llvm(), v.s, slot)); } self.body.push_str(&format!("  br label %{}\n", lend)); } }
            None => { if let Some(slot) = &slot { self.body.push_str(&format!("  store {} {}, ptr {}\n", want.llvm(), want.zero(), slot)); } self.body.push_str(&format!("  br label %{}\n", lend)); }
        }
        self.emit_label(&lend);
        match slot { Some(slot) => { let r = self.new_reg(); self.body.push_str(&format!("  {} = load {}, ptr {}\n", r, want.llvm(), slot)); Ok(Val::new(want, r)) } None => Ok(Val::new(&Ty::Void, "0")) }
    }

    /// 递归绑定解构模式（Result/Option 嵌套）：把 `val`（i64 槽）按模式写入 `pending_binds`。
    /// - `Ident(b)` → 绑定 b = val
    /// - `Some(inner)` / `Ok(inner)` / `Err(inner)` → 取内层载荷，递归
    /// 递归绑定解构模式，并返回"内层 tag 检查"的额外条件（用于嵌套 Some(Ok(v)) 等）。
    fn bind_pat_deep(&mut self, ty: &Ty, val: &str, pat: &Expr, pending_binds: &mut Vec<(String, Local)>) -> Option<String> {
        match &pat.kind {
            ExprKind::Ident(bn) => {
                let slot = self.new_alloca(ty);
                if *ty == Ty::F64 {
                    let d = self.new_reg();
                    self.body.push_str(&format!("  {} = bitcast i64 {} to double
", d, val));
                    self.body.push_str(&format!("  store double {}, ptr {}
", d, slot));
                } else if ty.llvm() == "ptr" {
                    let p = self.new_reg();
                    self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr
", p, val));
                    self.body.push_str(&format!("  store ptr {}, ptr {}
", p, slot));
                } else {
                    self.body.push_str(&format!("  store i64 {}, ptr {}
", val, slot));
                }
                pending_binds.push((bn.clone(), Local { ptr: slot, ty: ty.clone() }));
                None
            }
            ExprKind::Some(a) | ExprKind::Ok(a) | ExprKind::Err(a) => {
                let is_ok = matches!(pat.kind, ExprKind::Ok(_));
                let is_none = false;
                let inner_ty = match ty {
                    Ty::Result(t, e) => if is_ok { (**t).clone() } else { (**e).clone() },
                    Ty::Option(t) => (**t).clone(),
                    _ => Ty::I64,
                };
                let _ = is_none;
                // 内层若仍是 Option/Result 模式，先比对内层 tag
                let is_or_ctor = |k: &ExprKind| -> bool {
                    match k {
                        ExprKind::Some(_) | ExprKind::Ok(_) | ExprKind::Err(_) | ExprKind::None => true,
                        ExprKind::Call(n, _) => matches!(n.as_str(), "Some" | "Ok" | "Err" | "None"),
                        _ => false,
                    }
                };
                // 当前模式（Ok/Some/Err）自身也要比对 val 的 tag
                let self_cond = {
                    self.declare("declare i64 @gt_result_tag(ptr)");
                    let p = self.new_reg();
                    self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", p, val));
                    let it = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_result_tag(ptr {})\n", it, p));
                    let want: i64 = if matches!(pat.kind, ExprKind::Err(_)) { 1 } else { 0 };
                    let c = self.new_reg();
                    self.body.push_str(&format!("  {} = icmp eq i64 {}, {}\n", c, it, want));
                    Some(c)
                };
                let inner_cond = match &a.kind {
                    k if is_or_ctor(k) => {
                        if matches!(inner_ty, Ty::Option(_) | Ty::Result(..)) {
                            self.declare("declare i64 @gt_result_tag(ptr)");
                            let p = self.new_reg();
                            self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", p, val));
                            let it = self.new_reg();
                            self.body.push_str(&format!("  {} = call i64 @gt_result_tag(ptr {})\n", it, p));
                            let want: i64 = match &a.kind {
                                ExprKind::None | ExprKind::Err(_) => 1,
                                ExprKind::Call(n, _) if n == "None" || n == "Err" => 1,
                                _ => 0,
                            };
                            let c = self.new_reg();
                            self.body.push_str(&format!("  {} = icmp eq i64 {}, {}\n", c, it, want));
                            Some(c)
                        } else { None }
                    }
                    _ => None,
                };
                let v2 = self.recv_val_load(val);
                let deeper = self.bind_pat_deep(&inner_ty, &v2, a, pending_binds);
                let mut acc = self_cond;
                let _ = &mut acc;
                for c in [inner_cond, deeper].into_iter().flatten() {
                    acc = Some(match acc {
                        None => c,
                        Some(prev) => { let r = self.new_reg(); self.body.push_str(&format!("  {} = and i1 {}, {}\n", r, prev, c)); r }
                    });
                }
                acc
            }
            ExprKind::EnumLit(en, var, binds) => {
                // 内层 enum 解构（如 E::Has(Shape::Circle(r))）：val 是内层 enum 的堆块句柄，
                // 逐子模式从偏移 8 起取载荷并递归绑定。
                let ptys: Vec<Ty> = self.enum_variants.get(en).and_then(|vs| vs.iter().find(|(n, _)| n == var).map(|(_, ts)| ts.clone())).unwrap_or_default();
                // val 是内层 enum 的堆块句柄（i64 槽），先转成 ptr
                let vp = self.new_reg();
                self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr
", vp, val));
                for (i, bd) in binds.iter().enumerate() {
                    let bty = ptys.get(i).cloned().unwrap_or(Ty::I64);
                    let lp = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 {}
", lp, vp, i + 1));
                    let lv = self.new_reg();
                    self.body.push_str(&format!("  {} = load i64, ptr {}
", lv, lp));
                    self.bind_pat_deep(&bty, &lv, bd, pending_binds);
                }
                None
            }
            ExprKind::Call(n, args) if matches!(n.as_str(), "Some" | "Ok" | "Err") && args.len() == 1 => {
                let inner_ty = match ty {
                    Ty::Result(t, e) => if n == "Ok" { (**t).clone() } else { (**e).clone() },
                    Ty::Option(t) => (**t).clone(),
                    _ => Ty::I64,
                };
                // 当前构造器（Ok/Some/Err）的 tag 检查
                let self_cond = {
                    self.declare("declare i64 @gt_result_tag(ptr)");
                    let p = self.new_reg();
                    self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", p, val));
                    let it = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_result_tag(ptr {})\n", it, p));
                    let want: i64 = if n == "Err" { 1 } else { 0 };
                    let c = self.new_reg();
                    self.body.push_str(&format!("  {} = icmp eq i64 {}, {}\n", c, it, want));
                    Some(c)
                };
                let inner_cond = match &args[0].kind {
                    ExprKind::Some(_) | ExprKind::Ok(_) | ExprKind::Err(_) | ExprKind::None => {
                        if matches!(inner_ty, Ty::Option(_) | Ty::Result(..)) {
                            self.declare("declare i64 @gt_result_tag(ptr)");
                            let p = self.new_reg();
                            self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", p, val));
                            let it = self.new_reg();
                            self.body.push_str(&format!("  {} = call i64 @gt_result_tag(ptr {})\n", it, p));
                            let want: i64 = match &args[0].kind {
                                ExprKind::None | ExprKind::Err(_) => 1,
                                ExprKind::Call(n, _) if n == "None" || n == "Err" => 1,
                                _ => 0,
                            };
                            let c = self.new_reg();
                            self.body.push_str(&format!("  {} = icmp eq i64 {}, {}\n", c, it, want));
                            Some(c)
                        } else { None }
                    }
                    _ => None,
                };
                let v2 = self.recv_val_load(val);
                let deeper = self.bind_pat_deep(&inner_ty, &v2, &args[0], pending_binds);
                let mut acc = self_cond;
                for c in [inner_cond, deeper].into_iter().flatten() {
                    acc = Some(match acc {
                        None => c,
                        Some(prev) => { let r = self.new_reg(); self.body.push_str(&format!("  {} = and i1 {}, {}\n", r, prev, c)); r }
                    });
                }
                acc
            }
            _ => None,
        }
    }

    /// 嵌套解构：从"内层 Result/Option（i64 句柄）"取载荷（i64 槽）。
    fn recv_val_load(&mut self, val: &str) -> String {
        self.declare("declare i64 @gt_result_val(ptr)");
        let p = self.new_reg();
        self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr
", p, val));
        let v = self.new_reg();
        self.body.push_str(&format!("  {} = call i64 @gt_result_val(ptr {})
", v, p));
        v
    }

    pub(crate) fn match_value(&mut self, subject: &Expr, arms: &[MatchArm], want: &Ty, line: usize) -> Result<Val, String> {
        let subj = self.expr(subject)?;
        let slot = if want != &Ty::Void { Some(self.new_alloca(want)) } else { None };
        let lend = self.new_label();
        for (i, arm) in arms.iter().enumerate() {
            let larm = self.new_label();
            let lnext = self.new_label();
            let mut pending_binds: Vec<(String, Local)> = Vec::new();
            let cond = if let Some((lo, hi)) = &arm.range {
                // 范围模式 lo..hi（含 lo，不含 hi）
                let lov = self.expr(lo)?;
                let hiv = self.expr(hi)?;
                let ge = self.new_reg();
                self.body.push_str(&format!("  {} = icmp sge i64 {}, {}\n", ge, subj.s, lov.s));
                let lt = self.new_reg();
                self.body.push_str(&format!("  {} = icmp slt i64 {}, {}\n", lt, subj.s, hiv.s));
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = and i1 {}, {}\n", r, ge, lt));
                r
            } else {
                match &arm.pat {
                    None => match &arm.guard { None => "true".to_string(), Some(g) => self.cond(g)? },
                    Some(p) => {
                        // Result/Option 变体绑定：Ok(v) / Err(e) / Some(v)
                        let ctor: Option<(&str, Option<&Expr>)> = match &p.kind {
                            ExprKind::Ok(a) => Some(("Ok", Some(a))),
                            ExprKind::Err(a) => Some(("Err", Some(a))),
                            ExprKind::Some(a) => Some(("Some", Some(a))),
                            // `None`：Option 的空值，tag=1，无绑定
                            ExprKind::None => Some(("None", None)),
                            // `Ok(v)` 在 parser 中是 Call("Ok", [Ident])
                            ExprKind::Call(n, args) if matches!(n.as_str(), "Ok" | "Err" | "Some") && args.len() == 1 => Some((n.as_str(), Some(&args[0]))),
                            _ => None,
                        };
                        if let Some((cname, carg)) = ctor {
                            let want_tag: i64 = if cname == "Err" || cname == "None" { 1 } else { 0 };
                            self.declare("declare i64 @gt_result_tag(ptr)");
                            // subj 可能是 ptr（Result）或 i64（句柄），统一转成 ptr
                            let sp = if subj.ty.llvm() == "ptr" {
                                subj.s.clone()
                            } else {
                                let r = self.new_reg();
                                self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", r, subj.s));
                                r
                            };
                            let tag = self.new_reg();
                            self.body.push_str(&format!("  {} = call i64 @gt_result_tag(ptr {})\n", tag, sp));
                            let eq = self.new_reg();
                            self.body.push_str(&format!("  {} = icmp eq i64 {}, {}\n", eq, tag, want_tag));
                            let mut cond = eq;
                            if let Some(carg) = carg {
                                // 递归绑定：支持嵌套解构（Some(Some(v)) 等）。
                                let bty = match &subj.ty {
                                    Ty::Result(t, e) => match cname { "Ok" => (**t).clone(), "Err" => (**e).clone(), _ => Ty::Unknown },
                                    Ty::Option(t) => if cname == "Some" { (**t).clone() } else { Ty::Unknown },
                                    _ => Ty::Unknown,
                                };
                                self.declare("declare i64 @gt_result_val(ptr)");
                                let val = self.new_reg();
                                self.body.push_str(&format!("  {} = call i64 @gt_result_val(ptr {})\n", val, sp));
                                if let Some(inner_cond) = self.bind_pat_deep(&bty, &val, carg, &mut pending_binds) {
                                    let r2 = self.new_reg();
                                    self.body.push_str(&format!("  {} = and i1 {}, {}\n", r2, cond, inner_cond));
                                    cond = r2;
                                }
                            }
                            // guard 可能引用载荷绑定（Some(v) if v > 0）：先临时注入作用域
                            self.push_scope();
                            for (n2, l2) in &pending_binds { self.scopes.last_mut().unwrap().insert(n2.clone(), l2.clone()); }
                            let gres = match &arm.guard { None => cond, Some(g) => { let gv = self.cond(g)?; let r = self.new_reg(); self.body.push_str(&format!("  {} = and i1 {}, {}\n", r, cond, gv)); r } };
                            self.pop_scope();
                            gres
                        } else if let ExprKind::EnumLit(en, var, binds) = &p.kind {
                            // 枚举解构：比较 tag 并暂存载荷
                            let vidx = self.enum_variants.get(en).and_then(|vs| vs.iter().position(|(n, _)| n == var)).unwrap_or(0);
                            let tp = self.new_reg();
                            self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 0\n", tp, subj.s));
                            let tag = self.new_reg();
                            self.body.push_str(&format!("  {} = load i64, ptr {}\n", tag, tp));
                            let mut eq = self.new_reg();
                            self.body.push_str(&format!("  {} = icmp eq i64 {}, {}\n", eq, tag, vidx));
                            // 绑定：把载荷存入新槽（在 arm 块内注入，见下方 bindings）
                            let ptys: Vec<Ty> = self.enum_variants.get(en).and_then(|vs| vs.iter().find(|(n, _)| n == var).map(|(_, ts)| ts.clone())).unwrap_or_default();
                            for (i, b) in binds.iter().enumerate() {
                                let bty = ptys.get(i).cloned().unwrap_or(Ty::I64);
                                let lp = self.new_reg();
                                self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 {}\n", lp, subj.s, i + 1));
                                let lv = self.new_reg();
                                self.body.push_str(&format!("  {} = load i64, ptr {}\n", lv, lp));
                                // inner tag check for destructuring payload (E::A(Some(v)) vs E::A(None))
                                // 内层 enum 变体的真实 tag 从 enum_variants 取（不能一律当 0）。
                                if !matches!(b.kind, ExprKind::Ident(_)) {
                                    let want_inner: i64 = match &b.kind {
                                        ExprKind::None | ExprKind::Err(_) => 1,
                                        ExprKind::Call(n, _) if n == "None" || n == "Err" => 1,
                                        ExprKind::EnumLit(ien, ivar, _) => self.enum_variants.get(ien).and_then(|vs| vs.iter().position(|(n, _)| n == ivar)).map(|p| p as i64).unwrap_or(0),
                                        _ => 0,
                                    };
                                    let lvp = self.new_reg();
                                    self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", lvp, lv));
                                    let itag = self.new_reg();
                                    self.body.push_str(&format!("  {} = load i64, ptr {}\n", itag, lvp));
                                    let ieq = self.new_reg();
                                    self.body.push_str(&format!("  {} = icmp eq i64 {}, {}\n", ieq, itag, want_inner));
                                    let neq = self.new_reg();
                                    self.body.push_str(&format!("  {} = and i1 {}, {}\n", neq, eq, ieq));
                                    eq = neq;
                                }
                                if let ExprKind::Ident(bn) = &b.kind {
                                    let slot = self.new_alloca(&bty);
                                    // f64 载荷按位模式存储，需 bitcast
                                    if bty == Ty::F64 {
                                        let d = self.new_reg();
                                        self.body.push_str(&format!("  {} = bitcast i64 {} to double\n", d, lv));
                                        self.body.push_str(&format!("  store double {}, ptr {}\n", d, slot));
                                    } else {
                                        self.body.push_str(&format!("  store i64 {}, ptr {}\n", lv, slot));
                                    }
                                    pending_binds.push((bn.clone(), Local { ptr: slot, ty: bty }));
                                } else {
                                    // 载荷本身是解构模式（E::A(Some(v))）：递归绑定
                                    self.bind_pat_deep(&bty, &lv, b, &mut pending_binds);
                                }
                            }
                            // guard 里可能引用载荷绑定（如 E::A(n) if n > 5）：
                            // 先临时注入绑定作用域，再求值 guard。
                            self.push_scope();
                            for (n2, l2) in &pending_binds { self.scopes.last_mut().unwrap().insert(n2.clone(), l2.clone()); }
                            let gres = match &arm.guard { None => eq, Some(g) => { let gv = self.cond(g)?; let r = self.new_reg(); self.body.push_str(&format!("  {} = and i1 {}, {}\n", r, eq, gv)); r } };
                            self.pop_scope();
                            gres
                        } else if let ExprKind::Ident(bn) = &p.kind {
                            // 裸标识符模式：绑定主体值（与 JIT 侧一致）。
                            // 若该名字已在作用域中，退回"比较"语义（外层变量当模式）。
                            if self.lookup(bn).is_some() {
                                let pv = self.expr(p)?;
                                let eq = self.eq(&subj, &pv, line)?;
                                match &arm.guard { None => eq, Some(g) => { let gv = self.cond(g)?; let r = self.new_reg(); self.body.push_str(&format!("  {} = and i1 {}, {}\n", r, eq, gv)); r } }
                            } else {
                                let bty = subj.ty.clone();
                                let slot = self.new_alloca(&bty);
                                if bty == Ty::F64 {
                                    self.body.push_str(&format!("  store double {}, ptr {}\n", subj.s, slot));
                                } else if bty.llvm() == "ptr" {
                                    self.body.push_str(&format!("  store ptr {}, ptr {}\n", subj.s, slot));
                                } else {
                                    self.body.push_str(&format!("  store i64 {}, ptr {}\n", subj.s, slot));
                                }
                                pending_binds.push((bn.clone(), Local { ptr: slot, ty: bty }));
                                // guard 可能引用本绑定：临时注入作用域后求值
                                self.push_scope();
                                for (n2, l2) in &pending_binds { self.scopes.last_mut().unwrap().insert(n2.clone(), l2.clone()); }
                                let gres = match &arm.guard { None => "true".to_string(), Some(g) => self.cond(g)? };
                                self.pop_scope();
                                gres
                            }
                        } else {
                            let pv = self.expr(p)?;
                            let eq = self.eq(&subj, &pv, line)?;
                            match &arm.guard { None => eq, Some(g) => { let gv = self.cond(g)?; let r = self.new_reg(); self.body.push_str(&format!("  {} = and i1 {}, {}\n", r, eq, gv)); r } }
                        }
                    }
                }
            };
            self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", cond, larm, lnext));
            self.emit_label(&larm);
            self.push_scope();
            for (n, l) in pending_binds.drain(..) {
                self.scopes.last_mut().unwrap().insert(n, l);
            }
            let v = self.block_ret(&arm.body, want)?;
            self.pop_scope();
            if !self.terminated {
                if let (Some(slot), Some(v)) = (&slot, v) { let v = self.coerce(&v, want)?; self.body.push_str(&format!("  store {} {}, ptr {}\n", want.llvm(), v.s, slot)); }
                self.body.push_str(&format!("  br label %{}\n", lend));
            }
            self.emit_label(&lnext);
            if i + 1 == arms.len() { self.body.push_str(&format!("  br label %{}\n", lend)); }
        }
        self.emit_label(&lend);
        match slot { Some(slot) => { let r = self.new_reg(); self.body.push_str(&format!("  {} = load {}, ptr {}\n", r, want.llvm(), slot)); Ok(Val::new(want, r)) } None => Ok(Val::new(&Ty::Void, "0")) }
    }

    pub(crate) fn eq(&mut self, a: &Val, b: &Val, _line: usize) -> Result<String, String> {
        let r = self.new_reg();
        // enum 相等性：比较 tag（堆块第一个 i64 槽）
        if let (Ty::Enum(_), Ty::Enum(_)) = (&a.ty, &b.ty) {
            let ta = self.new_reg();
            let tb = self.new_reg();
            self.body.push_str(&format!("  {} = load i64, ptr {}
", ta, a.s));
            self.body.push_str(&format!("  {} = load i64, ptr {}
", tb, b.s));
            self.body.push_str(&format!("  {} = icmp eq i64 {}, {}
", r, ta, tb));
            return Ok(r);
        }
        // 非标量（元组/结构体/容器等 ptr 类）不能按 i64 比较——否则生成非法 IR。
        // 明确拒绝，交由上层报"不支持的模式"。
        if matches!(a.ty, Ty::Tuple(_) | Ty::Struct(_) | Ty::List(_) | Ty::Set(_) | Ty::Map(_, _) | Ty::Array(_, _))
            || matches!(b.ty, Ty::Tuple(_) | Ty::Struct(_) | Ty::List(_) | Ty::Set(_) | Ty::Map(_, _) | Ty::Array(_, _))
        {
            return Err(crate::lb!(_line, "match pattern of composite type is not supported", "不支持复合类型的 match 模式"));
        }
        if a.ty == Ty::Str || b.ty == Ty::Str {
            self.declare("declare i32 @strcmp(ptr, ptr)");
            let c = self.new_reg();
            self.body.push_str(&format!("  {} = call i32 @strcmp(ptr {}, ptr {})\n", c, a.s, b.s));
            self.body.push_str(&format!("  {} = icmp eq i32 {}, 0\n", r, c));
        } else if a.ty == Ty::F64 || b.ty == Ty::F64 {
            self.body.push_str(&format!("  {} = fcmp oeq double {}, {}\n", r, a.s, b.s));
        } else if a.ty == Ty::Bool || b.ty == Ty::Bool {
            // 布尔字面量是 i1；按 i64 比较会生成非法 IR。
            self.body.push_str(&format!("  {} = icmp eq i1 {}, {}\n", r, a.s, b.s));
        } else {
            self.body.push_str(&format!("  {} = icmp eq i64 {}, {}\n", r, a.s, b.s));
        }
        Ok(r)
    }
}



