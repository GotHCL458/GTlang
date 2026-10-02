//! Codegen 的表达式代码生成。
/// 判断 `cand` 是否比 `old` 更精确（元素类型细化回填用）。
/// 仅当两者同构、old 含 Unknown、cand 不含时返回 true。
fn is_more_precise(cand: &Ty, old: &Ty) -> bool {
    match (cand, old) {
        (Ty::List(a), Ty::List(b)) => !matches!(**a, Ty::Unknown) && matches!(**b, Ty::Unknown),
        (Ty::Set(a), Ty::Set(b)) => !matches!(**a, Ty::Unknown) && matches!(**b, Ty::Unknown),
        (Ty::Map(ak, av), Ty::Map(bk, bv)) => {
            (!matches!(**ak, Ty::Unknown) && matches!(**bk, Ty::Unknown))
                || (!matches!(**av, Ty::Unknown) && matches!(**bv, Ty::Unknown))
        }
        _ => false,
    }
}

use super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn expr(&mut self, e: &Expr) -> Result<Val, String> {
        match &e.kind {
            ExprKind::Int(v) => Ok(Val::new(&Ty::I64, v.to_string())),
            ExprKind::Float(v) => Ok(Val::new(&Ty::F64, fmt_double(*v))),
            ExprKind::Bool(v) => Ok(Val::new(&Ty::Bool, if *v { "true" } else { "false" })),
            ExprKind::CallNamed(_, _) => Err("internal: CallNamed not resolved".to_string()),
            ExprKind::MethodOn { .. } => Err("internal: MethodOn not lowered".to_string()),
            ExprKind::ListComp { .. } => Err("internal: ListComp not expanded".to_string()),
            ExprKind::DynBox { trait_name, value } => self.dyn_box(trait_name, value, e),
            ExprKind::EnumLit(name, variant, args) => {
                // 堆块 [tag, payload...]（与 Result/Option 一致）
                self.declare("declare ptr @gt_mem_alloc(i64)");
                let n = args.len() + 1;
                let base = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_mem_alloc(i64 {})\n", base, n * 8));
                // tag
                let tag = match self.enum_variants.get(name).and_then(|vs| vs.iter().position(|(vn, _)| vn == variant)) {
                    Some(t) => t,
                    None => 0,
                };
                let tp = self.new_reg();
                self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 0\n", tp, base));
                self.body.push_str(&format!("  store i64 {}, ptr {}\n", tag, tp));
                for (i, a) in args.iter().enumerate() {
                    let v = self.expr(a)?;
                    // f64 载荷按位模式存（与解构 load f64 对应）
                    let s = if v.ty == Ty::F64 {
                        let b = self.new_reg();
                        self.body.push_str(&format!("  {} = bitcast double {} to i64\n", b, v.s));
                        b
                    } else { self.to_slot(&v) };
                    let p = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 {}\n", p, base, i + 1));
                    self.body.push_str(&format!("  store i64 {}, ptr {}\n", s, p));
                }
                Ok(Val::new(&e.ty.clone(), base))
            }
            ExprKind::TupleLit(items) => {
                // 元组：堆分配 n*8 字节，字段依次存储（与 struct 一致）
                let n = items.len().max(1);
                self.declare("declare ptr @gt_mem_alloc(i64)");
                let base = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_mem_alloc(i64 {})\n", base, n * 8));
                for (i, it) in items.iter().enumerate() {
                    let v = self.expr(it)?;
                    let s = self.to_slot(&v);
                    let p = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 {}\n", p, base, i));
                    self.body.push_str(&format!("  store i64 {}, ptr {}\n", s, p));
                }
                Ok(Val::new(&e.ty.clone(), base))
            }
            ExprKind::Str(s) => {
                let g = self.intern(s.as_bytes());
                Ok(Val::new(&Ty::Str, g))
            }
            ExprKind::Interp(_) => self.interp_value(e),
            ExprKind::Ident(n) => {
                if let Some(loc) = self.lookup(n) {
                    let v = self.load(&loc)?;
                    // 若 sema 回填了更精确的类型（如 list/map 元素被 push/m[k]=v 细化），
                    // 用它覆盖变量声明时的类型 —— 让 for-in 等拿到精确元素类型。
                    if e.ty != Ty::Unknown && e.ty != v.ty && is_more_precise(&e.ty, &v.ty) {
                        return Ok(Val::new(&e.ty, v.s));
                    }
                    return Ok(v);
                }
                if let Some(c) = self.consts.get(n).cloned() {
                    return Ok(self.const_val(&c));
                }
                Err(crate::lb!(e.line, "undefined variable '{}'", "未定义的变量 '{}'", n))
            }
            ExprKind::Unary(op, a) => {
                let v = self.expr(a)?;
                match op {
                    UnOp::Neg => {
                        if v.ty.is_float() {
                            let r = self.new_reg();
                            self.body
                                .push_str(&format!("  {} = fneg double {}\n", r, v.s));
                            Ok(Val::new(&Ty::F64, r))
                        } else {
                            let x = self.as_i64(&v);
                            let r = self.new_reg();
                            self.body
                                .push_str(&format!("  {} = sub i64 0, {}\n", r, x));
                            Ok(Val::new(&Ty::I64, r))
                        }
                    }
                    UnOp::Not => {
                        let x = self.coerce(&v, &Ty::Bool)?;
                        let r = self.new_reg();
                        self.body
                            .push_str(&format!("  {} = xor i1 {}, true\n", r, x.s));
                        Ok(Val::new(&Ty::Bool, r))
                    }
                    UnOp::BitNot => {
                        let x = self.as_i64(&v);
                        let r = self.new_reg();
                        self.body
                            .push_str(&format!("  {} = xor i64 {}, -1\n", r, x));
                        Ok(Val::new(&Ty::I64, r))
                    }
                }
            }
            ExprKind::Binary(op, a, b) => {
                // `&&` / `||` 短路：只有必要时才求值右侧
                if op.is_logic() {
                    return self.logic_value(*op, a, b);
                }
                let va = self.expr(a)?;
                let vb = self.expr(b)?;
                let safe = self.range_analysis.as_ref().map(|an| an.is_safe(e)).unwrap_or(false);
                self.binary(*op, &va, &vb, e.line, safe)
            }
            ExprKind::Call(name, args) => self.call(name, args, e.line, &e.ty),
            ExprKind::Index(base, idx) => {
                let b = self.expr(base)?;
                let iv = self.expr(idx)?;
                let i = self.as_i64(&iv);
                // map 键可为字符串：统一按 slot（ptrtoint）处理
                let key_slot = self.to_slot(&iv);
                match b.ty.clone() {
                    Ty::Array(el, n) => {
                        // 负索引归一化（下标检查与地址计算都用归一化后的 i）
                        let i = if self.index_in_bounds(idx, n) {
                            i.clone()
                        } else {
                            let ni = self.norm_idx(&i, &n.to_string());
                            self.emit_bounds_check(&ni, &n.to_string(), e.line);
                            ni
                        };
                        let arrty = format!("[{} x {}]", n, el.llvm());
                        let p = self.new_reg();
                        self.body.push_str(&format!(
                            "  {} = getelementptr inbounds {}, ptr {}, i64 0, i64 {}\n",
                            p, arrty, b.s, i
                        ));
                        let v = self.new_reg();
                        self.body
                            .push_str(&format!("  {} = load {}, ptr {}\n", v, el.llvm(), p));
                        Ok(Val::new(&el, v))
                    }
                    Ty::Str => {
                        // 字符串下标：长度是运行期 strlen，也要做越界检查
                        self.declare("declare i64 @strlen(ptr)");
                        let len = self.new_reg();
                        self.body
                            .push_str(&format!("  {} = call i64 @strlen(ptr {})\n", len, b.s));
                        // 负索引归一化：i < 0 ? i + len : i
                        let i = self.norm_idx(&i, &len);
                        self.emit_bounds_check(&i, &len, e.line);
                        let p = self.new_reg();
                        self.body.push_str(&format!(
                            "  {} = getelementptr inbounds i8, ptr {}, i64 {}\n",
                            p, b.s, i
                        ));
                        let c = self.new_reg();
                        self.body.push_str(&format!("  {} = load i8, ptr {}\n", c, p));
                        let v = self.new_reg();
                        self.body
                            .push_str(&format!("  {} = zext i8 {} to i64\n", v, c));
                        Ok(Val::new(&Ty::I64, v))
                    }
                    Ty::List(el) => {
                        let bp = self.as_ptr(&b);
                        self.declare("declare i64 @gt_list_len(ptr)");
                        let ln = self.new_reg();
                        self.body.push_str(&format!("  {} = call i64 @gt_list_len(ptr {})\n", ln, bp));
                        let i = self.norm_idx(&i, &ln);
                        // 越界检查：try 内 → 捕获；否则打印诊断并终止。
                        self.emit_bounds_check(&i, &ln, e.line);
                        self.declare("declare i64 @gt_list_at(ptr, i64)");
                        let v = self.new_reg();
                        self.body.push_str(&format!("  {} = call i64 @gt_list_at(ptr {}, i64 {})\n", v, bp, i));
                        // 元素在运行时是 i64 slot；若是 ptr 类元素，取出后立即转回 ptr 形态。
                        if el.llvm() == "ptr" {
                            let p = self.new_reg();
                            self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", p, v));
                            Ok(Val::new(&el, p))
                        } else {
                            Ok(Val::new_slot(&el, v))
                        }
                    }
                    Ty::Map(_, v) => {
                        let bp = self.as_ptr(&b);
                        self.declare("declare i64 @gt_map_get(ptr, i64)");
                        let r = self.new_reg();
                        self.body.push_str(&format!("  {} = call i64 @gt_map_get(ptr {}, i64 {})\n", r, bp, key_slot));
                        if v.llvm() == "ptr" {
                            let p = self.new_reg();
                            self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", p, r));
                            Ok(Val::new(&v, p))
                        } else {
                            Ok(Val::new_slot(&v, r))
                        }
                    }
                    other => Err(crate::lb!(e.line, "{} does not support indexing", "{} 不支持下标访问", other)),
                }
            }
            ExprKind::Slice(base, lo, hi) => {
                let b = self.expr(base)?;
                let lv = self.expr(lo)?;
                let hv = self.expr(hi)?;
                let l = self.as_i64(&lv);
                let h = self.as_i64(&hv);
                // list 切片：返回新 list（[lo, hi)）
                if matches!(b.ty, Ty::List(_)) {
                    let bp = self.as_ptr(&b);
                    self.declare("declare ptr @gt_list_slice(ptr, i64, i64)");
                    let r = self.new_reg();
                    self.body.push_str(&format!("  {} = call ptr @gt_list_slice(ptr {}, i64 {}, i64 {})\n", r, bp, l, h));
                    return Ok(Val::new(&Ty::List(Box::new(Ty::I64)), r));
                }
                let len = self.new_reg();
                self.body.push_str(&format!("  {} = sub i64 {}, {}\n", len, h, l));
                self.declare("declare ptr @gt_str_substr(ptr, i64, i64)");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_str_substr(ptr {}, i64 {}, i64 {})\n", r, b.s, l, len));
                Ok(Val::new(&Ty::Str, r))
            }
            ExprKind::ArrayLit(items) => {
                let (elem, n) = match &e.ty {
                    Ty::Array(el, n) => ((**el).clone(), *n),
                    _ => (Ty::I64, items.len()),
                };
                let arrty = format!("[{} x {}]", n, elem.llvm());
                let slot = self.new_alloca_raw(&arrty);
                for (i, it) in items.iter().enumerate() {
                    let v = self.expr(it)?;
                    let v = self.coerce(&v, &elem)?;
                    let p = self.new_reg();
                    self.body.push_str(&format!(
                        "  {} = getelementptr inbounds {}, ptr {}, i64 0, i64 {}\n",
                        p, arrty, slot, i
                    ));
                    self.body
                        .push_str(&format!("  store {} {}, ptr {}\n", elem.llvm(), v.s, p));
                }
                Ok(Val::new(&Ty::Array(Box::new(elem), n), slot))
            }
            ExprKind::StructLit(name, fields) => {
                let layout = self
                    .structs
                    .get(name)
                    .cloned()
                    .ok_or_else(|| crate::lb!(e.line, "undefined struct '{}'", "未定义的结构体 '{}'", name))?;
                // 堆分配：结构体可能作为返回值跨栈帧存活（栈槽会悬垂）
                self.declare("declare ptr @gt_mem_alloc(i64)");
                let nbytes = (layout.len().max(1) * 8) as i64;
                let slot = self.new_reg();
                self.body.push_str(&format!(
                    "  {} = call ptr @gt_mem_alloc(i64 {})\n",
                    slot, nbytes
                ));
                // 全部字段初始化为 0（i64）
                for i in 0..layout.len() {
                    let p = self.new_reg();
                    self.body.push_str(&format!(
                        "  {} = getelementptr inbounds i64, ptr {}, i64 {}\n",
                        p, slot, i
                    ));
                    self.body.push_str(&format!("  store i64 0, ptr {}\n", p));
                }
                for (fname, v) in fields {
                    let idx = layout
                        .iter()
                        .position(|(n, _)| n == fname)
                        .ok_or_else(|| crate::lb!(e.line, "struct '{}' has no field '{}'", "结构体 '{}' 没有字段 '{}'", name, fname))?;
                    let fty = layout[idx].1.clone();
                    let val = self.expr(v)?;
                    let val = self.coerce(&val, &fty)?;
                    let slotv = self.to_slot(&val);
                    let p = self.new_reg();
                    self.body.push_str(&format!(
                        "  {} = getelementptr inbounds i64, ptr {}, i64 {}\n",
                        p, slot, idx
                    ));
                    self.body.push_str(&format!("  store i64 {}, ptr {}\n", slotv, p));
                }
                Ok(Val::new(&Ty::Struct(name.clone()), slot))
            }
            ExprKind::Field(base, field) => {
                let bv = self.expr(base)?;
                // 元组下标 `.0`：堆块按 i64 槽访问
                if let Ty::Tuple(ts) = &bv.ty {
                    let idx: usize = field.parse().unwrap_or(usize::MAX);
                    let fty = ts.get(idx).cloned().ok_or_else(|| crate::lb!(e.line, "tuple index {} out of range", "元组下标 {} 越界", field))?;
                    let p = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 {}\n", p, bv.s, idx));
                    let raw = self.new_reg();
                    self.body.push_str(&format!("  {} = load i64, ptr {}\n", raw, p));
                    let v = self.from_slot(&raw, &fty);
                    return Ok(Val::new(&fty, v));
                }
                let sname = match &bv.ty {
                    Ty::Struct(n) => n.clone(),
                    other => {
                        return Err(crate::lb!(e.line, "{} is not a struct; cannot access field '{}'", "{} 不是结构体，不能访问字段 '{}'", other, field))
                    }
                };
                let layout = self
                    .structs
                    .get(&sname)
                    .cloned()
                    .ok_or_else(|| crate::lb!(e.line, "undefined struct '{}'", "未定义的结构体 '{}'", sname))?;
                let idx = layout
                    .iter()
                    .position(|(n, _)| n == field)
                    .ok_or_else(|| crate::lb!(e.line, "struct '{}' has no field '{}'", "结构体 '{}' 没有字段 '{}'", sname, field))?;
                let fty = layout[idx].1.clone();
                let arrty = format!("[{} x i64]", layout.len().max(1));
                let p = self.new_reg();
                self.body.push_str(&format!(
                    "  {} = getelementptr inbounds {}, ptr {}, i64 0, i64 {}\n",
                    p, arrty, bv.s, idx
                ));
                let raw = self.new_reg();
                self.body.push_str(&format!("  {} = load i64, ptr {}\n", raw, p));
                let v = self.from_slot(&raw, &fty);
                Ok(Val::new(&fty, v))
            }
            ExprKind::Match { subject, arms } => {
                let t = e.ty.clone();
                self.match_value(subject, arms, &t, e.line)
            }
            ExprKind::TryBlock { body, catches, fin } => {
                // try 块作为表达式：body 末表达式是 Result，返回 Ok 值 / handler 值
                let want = Ty::Result(Box::new(Ty::I64), Box::new(Ty::I64));
                let rv = self.block_ret(body, &want)?;
                let slot = self.new_alloca(&Ty::I64);
                if let Some(v) = rv {
                    self.declare("declare i64 @gt_result_tag(ptr)");
                    self.declare("declare i64 @gt_result_val(ptr)");
                    let tag = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_result_tag(ptr {})\n", tag, v.s));
                    let is_err = self.new_reg();
                    self.body.push_str(&format!("  {} = icmp ne i64 {}, 0\n", is_err, tag));
                    let l_err = self.new_label();
                    let l_ok = self.new_label();
                    let l_end = self.new_label();
                    self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", is_err, l_err, l_ok));
                    self.emit_label(&l_ok);
                    self.terminated = false;
                    let val = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_result_val(ptr {})\n", val, v.s));
                    self.body.push_str(&format!("  store i64 {}, ptr {}\n", val, slot));
                    self.body.push_str(&format!("  br label %{}\n", l_end));
                    self.emit_label(&l_err);
                    self.terminated = false;
                    let ev = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_result_val(ptr {})\n", ev, v.s));
                    if let Some(ca) = catches.first() {
                        self.push_scope();
                        if let Some(binding) = &ca.binding {
                            let bslot = self.new_alloca(&Ty::I64);
                            self.body.push_str(&format!("  store i64 {}, ptr {}\n", ev, bslot));
                            self.scopes.last_mut().unwrap().insert(binding.clone(), Local { ptr: bslot, ty: Ty::I64 });
                        }
                        let hv = self.block_ret(&ca.body, &Ty::I64)?;
                        if let Some(hv) = hv {
                            let hs = self.as_i64(&hv);
                            self.body.push_str(&format!("  store i64 {}, ptr {}\n", hs, slot));
                        }
                        self.pop_scope();
                    }
                    if !self.terminated {
                        self.body.push_str(&format!("  br label %{}\n", l_end));
                    }
                    self.emit_label(&l_end);
                    self.terminated = false;
                }
                if let Some(f) = fin {
                    self.push_scope();
                    self.block(f)?;
                    self.pop_scope();
                }
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = load i64, ptr {}\n", r, slot));
                Ok(Val::new(&Ty::I64, r))
            }
            ExprKind::Borrow { inner, .. } => self.expr(inner),
            ExprKind::Closure { .. } => {
                Err(crate::lb!(e.line, "closure was not hoisted (internal error)", "闭包未被提升（内部错误）"))
            }
            ExprKind::ClosureNew { fn_name, captures } => {
                // 分配 [fn_ptr, cap0, ...]（malloc）
                self.declare("declare ptr @gt_mem_alloc(i64)");
                let n = (captures.len() + 1) * 8;
                let r = self.new_reg();
                self.body
                    .push_str(&format!("  {} = call ptr @gt_mem_alloc(i64 {})\n", r, n));
                // 取闭包函数地址
                let finfo = self
                    .fns
                    .get(fn_name)
                    .cloned()
                    .ok_or_else(|| crate::lb!(e.line, "closure function '{}' not found", "闭包函数 '{}' 未找到", fn_name))?;
                let fp = self.new_reg();
                self.body
                    .push_str(&format!("  {} = ptrtoint ptr @{} to i64\n", fp, finfo.cname));
                self.body
                    .push_str(&format!("  store i64 {}, ptr {}\n", fp, r));
                // 存捕获值
                for (i, c) in captures.iter().enumerate() {
                    let v = self.expr(c)?;
                    let sv = self.to_slot(&v);
                    let p = self.new_reg();
                    self.body.push_str(&format!(
                        "  {} = getelementptr inbounds i8, ptr {}, i64 {}\n",
                        p, r, (i + 1) * 8
                    ));
                    self.body
                        .push_str(&format!("  store i64 {}, ptr {}\n", sv, p));
                }
                Ok(Val::new(&e.ty, r))
            }
            ExprKind::CallValue { callee, args } => {
                let cb = self.expr(callee)?;
                let (ptypes, ret) = match &callee.ty {
                    Ty::Closure(p, r) => (p.clone(), (**r).clone()),
                    _ => (vec![Ty::I64; args.len()], Ty::I64),
                };
                // 载入函数指针
                let fp = self.new_reg();
                self.body
                    .push_str(&format!("  {} = load i64, ptr {}\n", fp, cb.s));
                // 组装实参：先捕获值（块偏移 8 起），再显式实参
                let ncap = ptypes.len().saturating_sub(args.len());
                let mut ops: Vec<String> = Vec::new();
                for i in 0..ncap {
                    let p = self.new_reg();
                    self.body.push_str(&format!(
                        "  {} = getelementptr inbounds i8, ptr {}, i64 {}\n",
                        p, cb.s, (i + 1) * 8
                    ));
                    let v = self.new_reg();
                    self.body
                        .push_str(&format!("  {} = load i64, ptr {}\n", v, p));
                    ops.push(format!("i64 {}", v));
                }
                for (i, a) in args.iter().enumerate() {
                    let got = self.expr(a)?;
                    let want = ptypes.get(ncap + i).cloned().unwrap_or(Ty::I64);
                    let cv = self.coerce(&got, &want)?;
                    ops.push(format!("{} {}", want.llvm(), cv.s));
                }
                // 间接调用：从函数指针 inttoptr 后 call
                let fpp = self.new_reg();
                self.body
                    .push_str(&format!("  {} = inttoptr i64 {} to ptr\n", fpp, fp));
                let ret_llvm = if ret == Ty::Void { "void".to_string() } else { ret.llvm() };
                let argstr = ops.join(", ");
                if ret == Ty::Void {
                    self.body.push_str(&format!("  call void {}({})\n", fpp, argstr));
                    Ok(Val::new(&Ty::Void, "0"))
                } else {
                    let r = self.new_reg();
                    self.body.push_str(&format!(
                        "  {} = call {} {}({})\n",
                        r, ret_llvm, fpp, argstr
                    ));
                    Ok(Val::new(&ret, r))
                }
            }
            ExprKind::If { cond, then, els } => {
                let t = e.ty.clone();
                self.if_value(cond, then, els.as_ref(), &t, e.line)
            }
            ExprKind::Ok(inner) => { eprintln!("[ok] e.ty={:?}", e.ty); self.result_new(inner, false, e) },
            ExprKind::Err(inner) => self.result_new(inner, true, e),
            ExprKind::Some(inner) => self.result_new(inner, false, e),
            ExprKind::None => self.result_new_none(e),
            ExprKind::Try(inner) => self.result_try(inner, e),
        }
    }

    /// 构造 Result：tag=0（Ok）/ tag=1（Err），payload 为值的 8 字节槽。
    pub(crate) fn result_new(&mut self, inner: &Expr, is_err: bool, e: &Expr) -> Result<Val, String> {
        let v = self.expr(inner)?;
        let slotv = self.to_slot(&v);
        self.declare("declare ptr @gt_result_new(i64, i64)");
        let r = self.new_reg();
        self.body.push_str(&format!(
            "  {} = call ptr @gt_result_new(i64 {}, i64 {})
",
            r, if is_err { 1 } else { 0 }, slotv
        ));
        Ok(Val::new(&e.ty.clone(), r))
    }

    /// `None`：构造空 Option（tag=1, payload=0）
    pub(crate) fn result_new_none(&mut self, e: &Expr) -> Result<Val, String> {
        self.declare("declare ptr @gt_result_new(i64, i64)");
        let r = self.new_reg();
        self.body.push_str(&format!(
            "  {} = call ptr @gt_result_new(i64 1, i64 0)\n",
            r
        ));
        Ok(Val::new(&e.ty.clone(), r))
    }


    /// `expr?`：Err 时提前返回该 Result，Ok 时解包其值。
    pub(crate) fn result_try(&mut self, inner: &Expr, e: &Expr) -> Result<Val, String> {
        let v = self.expr(inner)?;
        self.declare("declare i64 @gt_result_tag(ptr)");
        self.declare("declare i64 @gt_result_val(ptr)");
        let tag = self.new_reg();
        self.body.push_str(&format!("  {} = call i64 @gt_result_tag(ptr {})\n", tag, v.s));
        let is_err = self.new_reg();
        self.body.push_str(&format!("  {} = icmp ne i64 {}, 0\n", is_err, tag));
        let l_err = self.new_label();
        let l_ok = self.new_label();
        self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", is_err, l_err, l_ok));
        self.emit_label(&l_err);
        if let Some((lbl, slot)) = self.err_stack.last().cloned() {
            let pv = self.new_reg();
            self.body.push_str(&format!("  {} = call i64 @gt_result_val(ptr {})\n", pv, v.s));
            self.body.push_str(&format!("  store i64 {}, ptr {}\n", pv, slot));
            self.body.push_str(&format!("  br label %{}\n", lbl));
        } else {
            self.body.push_str(&format!("  ret ptr {}\n", v.s));
        }
        self.terminated = true;
        self.emit_label(&l_ok);
        self.terminated = false;
        let raw = self.new_reg();
        self.body.push_str(&format!("  {} = call i64 @gt_result_val(ptr {})\n", raw, v.s));
        // 解包为内层类型
        match &e.ty {
            Ty::F64 => {
                let f = self.new_reg();
                self.body.push_str(&format!("  {} = bitcast i64 {} to double\n", f, raw));
                Ok(Val::new(&Ty::F64, f))
            }
            t => Ok(Val::new(&t.clone(), raw)),
        }
    }

    // ---------- 数组下标：越界检查与元素地址 ----------

    /// 数组下标越界检查（同时覆盖负下标）。
    ///
    /// 用**无符号**比较 `i < n`：负数会被当成极大值，一条 `icmp ult` 就同时判掉
    /// 下界与上界，不需要两次比较。失败时调用运行时 `gt_bounds` 打印诊断并终止。
    /// 负索引归一化：返回 `i < 0 ? i + len : i` 的新寄存器名。
    pub(crate) fn norm_idx(&mut self, i: &str, len: &str) -> String {
        let neg = self.new_reg();
        self.body.push_str(&format!("  {} = icmp slt i64 {}, 0\n", neg, i));
        let add = self.new_reg();
        self.body.push_str(&format!("  {} = add i64 {}, {}\n", add, i, len));
        let r = self.new_reg();
        self.body.push_str(&format!("  {} = select i1 {}, i64 {}, i64 {}\n", r, neg, add, i));
        r
    }

    pub(crate) fn emit_bounds_check(&mut self, i: &str, n_operand: &str, line: usize) {
        let ok = self.new_reg();
        self.body
            .push_str(&format!("  {} = icmp ult i64 {}, {}\n", ok, i, n_operand));
        let l_ok = self.new_label();
        let l_bad = self.new_label();
        self.body
            .push_str(&format!("  br i1 {}, label %{}, label %{}\n", ok, l_ok, l_bad));
        self.emit_label(&l_bad);
        if let Some((lbl, slot)) = self.err_stack.last().cloned() {
            self.body.push_str(&format!("  store i64 1, ptr {}\n", slot));
            self.body.push_str(&format!("  br label %{}\n", lbl));
        } else {
            self.declare("declare void @gt_bounds(i64, i64, i64)");
            self.body.push_str(&format!(
                "  call void @gt_bounds(i64 {}, i64 {}, i64 {})\n",
                i, n_operand, line
            ));
            self.body.push_str("  unreachable\n");
        }
        self.emit_label(&l_ok);
    }

    /// 下标是否**可证明**在 `[0, n)` 内（常量下标 / 已知上界的循环变量）。
    /// 成立时省略边界检查——语义不变，只是省一条比较+分支。
    pub(crate) fn index_in_bounds(&self, idx: &Expr, n: usize) -> bool {
        match &idx.kind {
            ExprKind::Int(v) => *v >= 0 && (*v as usize) < n,
            ExprKind::Ident(v) => self.bounded.get(v).map(|m| (*m as usize) <= n).unwrap_or(false),
            _ => false,
        }
    }

    /// 数组元素的地址（含越界检查）。`loc` 必须是数组类型的局部变量。
    /// 返回 `(元素类型, 元素指针)`。
    pub(crate) fn elem_ptr(&mut self, loc: &Local, idx: &Expr, line: usize) -> Result<(Ty, String), String> {
        let (elem, n) = match &loc.ty {
            Ty::Array(el, n) => ((**el).clone(), *n),
            other => return Err(crate::lb!(line, "{} does not support indexed assignment", "{} 不支持下标赋值", other)),
        };
        let arrp = self.load(loc)?;
        let iv = self.expr(idx)?;
        let i = self.as_i64(&iv);
        if !self.index_in_bounds(idx, n) {
            self.emit_bounds_check(&i, &n.to_string(), line);
        }
        let arrty = format!("[{} x {}]", n, elem.llvm());
        let p = self.new_reg();
        self.body.push_str(&format!(
            "  {} = getelementptr inbounds {}, ptr {}, i64 0, i64 {}\n",
            p, arrty, arrp.s, i
        ));
        Ok((elem, p))
    }

    /// 插值字符串作为值：格式化到栈缓冲，返回 ptr
    pub(crate) fn interp_value(&mut self, e: &Expr) -> Result<Val, String> {
        let (fmt, args) = self.fmt_of(e)?;
        let g = self.intern(&fmt);
        self.declare("declare i32 @gt_sprintf(ptr, i64, ptr, ...)");
        let buf = self.new_alloca_raw(&format!("[{} x i8]", INTERP_BUF));
        let mut ops = vec![
            format!("ptr {}", buf),
            format!("i64 {}", INTERP_BUF),
            format!("ptr {}", g),
        ];
        for (t, o) in &args {
            ops.push(format!("{} {}", t.llvm(), o));
        }
        let r = self.new_reg();
        self.body.push_str(&format!(
            "  {} = call i32 (ptr, i64, ptr, ...) @gt_sprintf({})\n",
            r,
            ops.join(", ")
        ));
        Ok(Val::new(&Ty::Str, buf))
    }

    /// `&&` / `||` 短路求值。
    ///
    /// 结果先写入一个槽（默认值即短路结果），只有左侧无法决定结果时才求值右侧：
    /// - `a && b`：a 为假 → 结果 false，跳过 b
    /// - `a || b`：a 为真 → 结果 true，跳过 b
    ///
    /// 这样 `false && f()` 不会调用 f()，与解释器后端语义一致。
    pub(crate) fn logic_value(&mut self, op: BinOp, a: &Expr, b: &Expr) -> Result<Val, String> {
        let is_and = op == BinOp::And;
        let slot = self.new_alloca(&Ty::Bool);
        let l_rhs = self.new_label();
        let l_end = self.new_label();

        let lhs = self.expr(a)?;
        let x = self.coerce(&lhs, &Ty::Bool)?;
        // 短路分支的默认值
        self.body.push_str(&format!(
            "  store i1 {}, ptr {}\n",
            if is_and { "false" } else { "true" },
            slot
        ));
        // && ：x 为真才去求值右侧；|| ：x 为假才去求值右侧
        let (t, f) = if is_and { (&l_rhs, &l_end) } else { (&l_end, &l_rhs) };
        self.body
            .push_str(&format!("  br i1 {}, label %{}, label %{}\n", x.s, t, f));

        self.emit_label(&l_rhs);
        let rhs = self.expr(b)?;
        let y = self.coerce(&rhs, &Ty::Bool)?;
        self.body
            .push_str(&format!("  store i1 {}, ptr {}\n", y.s, slot));
        self.body.push_str(&format!("  br label %{}\n", l_end));

        self.emit_label(&l_end);
        let r = self.new_reg();
        self.body
            .push_str(&format!("  {} = load i1, ptr {}\n", r, slot));
        Ok(Val::new(&Ty::Bool, r))
    }

    /// 位运算/算术等普通二元（非短路路径）
    pub(crate) fn binary(&mut self, op: BinOp, a: &Val, b: &Val, line: usize, safe: bool) -> Result<Val, String> {
        if op.is_logic() {
            // 调用方已走 logic_value 短路；这里只是兜底（正常情况下不会到）
            let x = self.coerce(a, &Ty::Bool)?;
            let y = self.coerce(b, &Ty::Bool)?;
            let r = self.new_reg();
            let ins = if op == BinOp::And { "and" } else { "or" };
            self.body
                .push_str(&format!("  {} = {} i1 {}, {}\n", r, ins, x.s, y.s));
            return Ok(Val::new(&Ty::Bool, r));
        }

        if a.ty == Ty::Str && b.ty == Ty::Str {
            if op == BinOp::Add {
                // 字符串拼接：调用运行时 gt_str_concat(a, b)
                let ap = self.as_ptr(a);
                let bp = self.as_ptr(b);
                self.declare("declare ptr @gt_str_concat(ptr, ptr)");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_str_concat(ptr {}, ptr {})\n", r, ap, bp));
                return Ok(Val::new(&Ty::Str, r));
            }
            if matches!(op, BinOp::Eq | BinOp::Ne) {
                let ap = self.as_ptr(a);
                let bp = self.as_ptr(b);
                self.declare("declare i32 @strcmp(ptr, ptr)");
                let r = self.new_reg();
                self.body.push_str(&format!(
                    "  {} = call i32 @strcmp(ptr {}, ptr {})\n",
                    r, ap, bp
                ));
                let c = self.new_reg();
                let pred = if op == BinOp::Eq { "eq" } else { "ne" };
                self.body
                    .push_str(&format!("  {} = icmp {} i32 {}, 0\n", c, pred, r));
                return Ok(Val::new(&Ty::Bool, c));
            }
            return Err(crate::lb!(line, "strings only support == / != comparison", "字符串只支持 == / != 比较"));
        }

        let use_float = a.ty.is_float() || b.ty.is_float();
        if use_float {
            let x = self.coerce(a, &Ty::F64)?;
            let y = self.coerce(b, &Ty::F64)?;
            if op.is_cmp() {
                let pred = match op {
                    BinOp::Eq => "oeq",
                    BinOp::Ne => "one",
                    BinOp::Lt => "olt",
                    BinOp::Le => "ole",
                    BinOp::Gt => "ogt",
                    _ => "oge",
                };
                let r = self.new_reg();
                self.body
                    .push_str(&format!("  {} = fcmp {} double {}, {}\n", r, pred, x.s, y.s));
                return Ok(Val::new(&Ty::Bool, r));
            }
            let ins = match op {
                BinOp::Add => "fadd",
                BinOp::Sub => "fsub",
                BinOp::Mul => "fmul",
                BinOp::Div | BinOp::FloorDiv => "fdiv",
                _ => "frem",
            };
            let r = self.new_reg();
            self.body
                .push_str(&format!("  {} = {} double {}, {}\n", r, ins, x.s, y.s));
            if op == BinOp::FloorDiv {
                self.declare("declare double @floor(double)");
                let f = self.new_reg();
                self.body
                    .push_str(&format!("  {} = call double @floor(double {})\n", f, r));
                return Ok(Val::new(&Ty::F64, f));
            }
            return Ok(Val::new(&Ty::F64, r));
        }

        let x = self.as_i64(a);
        let y = self.as_i64(b);
        if op.is_cmp() {
            let pred = match op {
                BinOp::Eq => "eq",
                BinOp::Ne => "ne",
                BinOp::Lt => "slt",
                BinOp::Le => "sle",
                BinOp::Gt => "sgt",
                _ => "sge",
            };
            let r = self.new_reg();
            self.body
                .push_str(&format!("  {} = icmp {} i64 {}, {}\n", r, pred, x, y));
            return Ok(Val::new(&Ty::Bool, r));
        }
        // 位运算（含移位）：结果仍是 i64
        if op.is_bit() {
            let r = self.new_reg();
            let line = match op {
                BinOp::BitAnd => format!("  {} = and i64 {}, {}\n", r, x, y),
                BinOp::BitOr => format!("  {} = or i64 {}, {}\n", r, x, y),
                BinOp::BitXor => format!("  {} = xor i64 {}, {}\n", r, x, y),
                BinOp::Shl => format!("  {} = shl i64 {}, {}\n", r, x, y),
                // 算术右移（有符号数保持符号位）
                _ => format!("  {} = ashr i64 {}, {}\n", r, x, y),
            };
            self.body.push_str(&line);
            return Ok(Val::new(&Ty::I64, r));
        }
        // 整数除法/取余：显式检查除零。
        // LLVM 的 sdiv/srem 在除数为 0 时是 UB（会产出任意值），必须自己拦，
        // 否则与解释器（CPU 直接崩溃）表现不一致。
        if matches!(op, BinOp::Div | BinOp::FloorDiv | BinOp::Rem) {
            self.emit_div_zero_check(&y, line);
        }
        // 加/减/乘：用 with.overflow 内建检测有符号溢出，溢出则运行时终止
        if matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) && overflow_check_enabled() && !safe {
            let intr = match op {
                BinOp::Add => "llvm.sadd.with.overflow.i64",
                BinOp::Sub => "llvm.ssub.with.overflow.i64",
                _ => "llvm.smul.with.overflow.i64",
            };
            self.declare(&format!("declare {{ i64, i1 }} @{}(i64, i64)", intr));
            let agg = self.new_reg();
            self.body.push_str(&format!(
                "  {} = call {{ i64, i1 }} @{}(i64 {}, i64 {})\n",
                agg, intr, x, y
            ));
            let r = self.new_reg();
            let ovf = self.new_reg();
            self.body
                .push_str(&format!("  {} = extractvalue {{ i64, i1 }} {}, 0\n", r, agg));
            self.body
                .push_str(&format!("  {} = extractvalue {{ i64, i1 }} {}, 1\n", ovf, agg));
            self.emit_overflow_check(&ovf, line);
            return Ok(Val::new(&Ty::I64, r));
        }
        let ins = match op {
            BinOp::Div | BinOp::FloorDiv => "sdiv",
            BinOp::Rem => "srem",
            // 关闭溢出检查时，加/减/乘用普通回绕指令
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            _ => "srem",
        };
        let r = self.new_reg();
        self.body
            .push_str(&format!("  {} = {} i64 {}, {}\n", r, ins, x, y));
        Ok(Val::new(&Ty::I64, r))
    }

    /// 整数溢出检查：`ovf` 是 i1 溢出标志；为真时调运行时终止。
    /// 错误分支标记 `cold`（`noreturn`），让 LLVM 把它移出热路径并优化分支预测。
    pub(crate) fn emit_overflow_check(&mut self, ovf: &str, line: usize) {
        let l_ok = self.new_label();
        let l_bad = self.new_label();
        // `ovf` 为真表示**溢出**（坏路径），故真→l_bad、假→l_ok
        self.body
            .push_str(&format!("  br i1 {}, label %{}, label %{}\n", ovf, l_bad, l_ok));
        self.emit_label(&l_bad);
        if let Some((lbl, slot)) = self.err_stack.last().cloned() {
            self.body.push_str(&format!("  store i64 3, ptr {}\n", slot));
            self.body.push_str(&format!("  br label %{}\n", lbl));
        } else {
            self.declare("declare void @gt_overflow(i64) noreturn");
            self.body
                .push_str(&format!("  call void @gt_overflow(i64 {})\n", line));
            self.body.push_str("  unreachable\n");
        }
        self.emit_label(&l_ok);
    }

    /// 整数除零检查：除数为 0 时调运行时打印诊断并终止
    pub(crate) fn emit_div_zero_check(&mut self, divisor: &str, line: usize) {
        let nz = self.new_reg();
        self.body
            .push_str(&format!("  {} = icmp ne i64 {}, 0\n", nz, divisor));
        let l_ok = self.new_label();
        let l_bad = self.new_label();
        self.body.push_str(&format!(
            "  br i1 {}, label %{}, label %{}\n",
            nz, l_ok, l_bad
        ));
        self.emit_label(&l_bad);
        if let Some((lbl, slot)) = self.err_stack.last().cloned() {
            self.body.push_str(&format!("  store i64 2, ptr {}\n", slot));
            self.body.push_str(&format!("  br label %{}\n", lbl));
        } else {
            self.declare("declare void @gt_div_zero(i64)");
            self.body
                .push_str(&format!("  call void @gt_div_zero(i64 {})\n", line));
            self.body.push_str("  unreachable\n");
        }
        self.emit_label(&l_ok);
    }

}
