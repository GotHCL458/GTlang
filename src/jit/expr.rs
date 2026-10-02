//! FnState 的表达式代码生成。

use super::*;

impl FnState {
    pub(crate) fn gen_expr(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, e: &Expr) -> Result<(Value, Ty), String> {
        match &e.kind {
            ExprKind::Int(v) => Ok((b.ins().iconst(types::I64, *v), Ty::I64)),
            ExprKind::Float(v) => Ok((b.ins().f64const(*v), Ty::F64)),
            ExprKind::Bool(v) => Ok((b.ins().iconst(types::I64, *v as i64), Ty::Bool)),
            ExprKind::CallNamed(_, _) => Err("internal: CallNamed not resolved".to_string()),
            ExprKind::MethodOn { recv, method, args } => {
                let rv = self.gen_expr(jit, b, recv)?;
                if let Ty::Dyn(tr) = rv.1.clone() {
                    let idx = jit.traits.get(&tr).and_then(|ms| ms.iter().position(|m| m == method)).map(|i| i + 1).unwrap_or(0);
                    let dp = b.ins().load(types::I64, MemFlags::new(), rv.0, 0);
                    let addr = b.ins().load(types::I64, MemFlags::new(), rv.0, (idx * 8) as i32);
                    let mut vals: Vec<cranelift_codegen::ir::Value> = vec![dp];
                    for a in args { let v = self.gen_expr(jit, b, a)?; vals.push(self.convert(b, &v, &Ty::I64)); }
                    let mut sig = jit.module.make_signature();
                    for _ in 0..vals.len() { sig.params.push(AbiParam::new(types::I64)); }
                    if e.ty != Ty::Void { sig.returns.push(AbiParam::new(cl_ty(&e.ty))); }
                    let sigref = b.import_signature(sig);
                    let call = b.ins().call_indirect(sigref, addr, &vals);
                    if e.ty == Ty::Void { Ok((b.ins().iconst(types::I64, 0), Ty::Void)) }
                    else { Ok((b.inst_results(call)[0], e.ty.clone())) }
                } else {
                    Err("internal: MethodOn (non-dyn) not lowered".to_string())
                }
            }
            ExprKind::ListComp { .. } => Err("internal: ListComp not expanded".to_string()),
            ExprKind::DynBox { trait_name, value } => self.gen_dyn_box(jit, b, trait_name, value),
            ExprKind::EnumLit(name, variant, args) => {
                let f = self.rt_ref(jit, b, "mem_alloc")?;
                let n = b.ins().iconst(types::I64, ((args.len() + 1) * 8) as i64);
                let call = b.ins().call(f, &[n]);
                let base = b.inst_results(call)[0];
                let tag = jit.enum_variants.get(name).and_then(|vs| vs.iter().position(|(vn, _)| vn == variant)).unwrap_or(0) as i64;
                let tagv = b.ins().iconst(types::I64, tag);
                b.ins().store(MemFlags::new(), tagv, base, 0);
                for (i, a) in args.iter().enumerate() {
                    let v = self.gen_expr(jit, b, a)?;
                    // f64 载荷按位模式存储
                    let iv = if v.1 == Ty::F64 { b.ins().bitcast(types::I64, MemFlags::new(), v.0) } else { self.convert(b, &v, &Ty::I64) };
                    b.ins().store(MemFlags::new(), iv, base, ((i + 1) * 8) as i32);
                }
                Ok((base, e.ty.clone()))
            }
            ExprKind::TupleLit(items) => {
                let f = self.rt_ref(jit, b, "mem_alloc")?;
                let n = b.ins().iconst(types::I64, (items.len().max(1) * 8) as i64);
                let call = b.ins().call(f, &[n]);
                let base = b.inst_results(call)[0];
                for (i, it) in items.iter().enumerate() {
                    let v = self.gen_expr(jit, b, it)?;
                    let iv = self.convert(b, &v, &Ty::I64);
                    b.ins().store(MemFlags::new(), iv, base, (i * 8) as i32);
                }
                Ok((base, e.ty.clone()))
            }
            ExprKind::Str(s) => {
                let id = jit.data_id_of(s.as_bytes()).ok_or_else(|| crate::lb!(e.line, "string constant not interned (internal error)", "字符串常量未预置（内部错误）"))?;
                Ok((data_ptr(jit, b, id), Ty::Str))
            }
            ExprKind::Interp(parts) => {
                let f_new = self.rt_ref(jit, b, "sb_new")?;
                let call = b.ins().call(f_new, &[]);
                let h = b.inst_results(call)[0];
                for p in parts {
                    match p {
                        StrPart::Lit(s) => {
                            let id = jit.data_id_of(s.as_bytes()).ok_or_else(|| "内部错误：字符串段未预置".to_string())?;
                            let ptr = data_ptr(jit, b, id);
                            let f = self.rt_ref(jit, b, "sb_push_str")?;
                            b.ins().call(f, &[h, ptr]);
                        }
                        StrPart::Expr(inner) => {
                            let v = self.gen_expr(jit, b, inner)?;
                            let key = match v.1 { Ty::F64 => "sb_push_f64", Ty::Str => "sb_push_str", Ty::Bool => "sb_push_bool", _ => "sb_push_i64" };
                            let f = self.rt_ref(jit, b, key)?;
                            b.ins().call(f, &[h, v.0]);
                        }
                    }
                }
                let f_fin = self.rt_ref(jit, b, "sb_finish")?;
                let call = b.ins().call(f_fin, &[h]);
                Ok((b.inst_results(call)[0], Ty::Str))
            }
            ExprKind::Ident(n) => {
                if let Some((var, ty)) = self.lookup(n) {
                    // sema 可能回填了更精确的类型（list/map 元素细化）；优先用它。
                    let ty = if e.ty != Ty::Unknown && e.ty != ty { e.ty.clone() } else { ty };
                    return Ok((b.use_var(var), ty));
                }
                if let Some(c) = jit.consts.get(n) { let c = c.clone(); return Ok(self.gen_const(jit, b, &c)); }
                Err(crate::lb!(e.line, "undefined variable '{}'", "未定义的变量 '{}'", n))
            }
            ExprKind::Unary(op, a) => {
                let v = self.gen_expr(jit, b, a)?;
                match op {
                    UnOp::Neg => {
                        if v.1 == Ty::F64 { Ok((b.ins().fneg(v.0), Ty::F64)) }
                        else { let z = b.ins().iconst(types::I64, 0); Ok((b.ins().isub(z, v.0), Ty::I64)) }
                    }
                    UnOp::Not => {
                        let iv = self.convert(b, &v, &Ty::I64);
                        let c = b.ins().icmp_imm(IntCC::Equal, iv, 0);
                        Ok((b.ins().uextend(types::I64, c), Ty::Bool))
                    }
                    UnOp::BitNot => {
                        let iv = self.convert(b, &v, &Ty::I64);
                        let all = b.ins().iconst(types::I64, -1);
                        Ok((b.ins().bxor(iv, all), Ty::I64))
                    }
                }
            }
            ExprKind::Binary(op, a, c) => {
                if op.is_logic() { return self.gen_logic(jit, b, *op, a, c); }
                let av = self.gen_expr(jit, b, a)?;
                let cv = self.gen_expr(jit, b, c)?;
                let safe = self.range.as_ref().map(|ra| ra.is_safe(e)).unwrap_or(false);
                self.gen_binop(jit, b, *op, &av, &cv, e.line, safe)
            }
            ExprKind::Call(name, args) => self.gen_call(jit, b, name, args, e.line, &e.ty),
            ExprKind::CallValue { callee, args } => self.gen_call_value(jit, b, callee, args, e),
            ExprKind::Index(base, idx) => {
                let bs = self.gen_expr(jit, b, base)?;
                match &bs.1 {
                    Ty::Array(el, _) => {
                        let (elem, addr) = self.gen_elem_addr(jit, b, &bs.0, &bs.1, idx, e.line)?;
                        let ld = b.ins().load(cl_ty(&elem), MemFlags::new(), addr, 0);
                        Ok((ld, (**el).clone()))
                    }
                    Ty::Str => {
                        let is = self.gen_expr(jit, b, idx)?;
                        let i = self.convert(b, &is, &Ty::I64);
                        let f = self.rt_ref(jit, b, "str_len")?;
                        let call = b.ins().call(f, &[bs.0]);
                        let len = b.inst_results(call)[0];
                        let i = self.norm_idx(b, i, len);
                        self.gen_bounds(jit, b, i, len, e.line)?;
                        let addr = b.ins().iadd(bs.0, i);
                        let ch = b.ins().load(types::I8, MemFlags::new(), addr, 0);
                        Ok((b.ins().uextend(types::I64, ch), Ty::I64))
                    }
                    Ty::List(el) => {
                        let is = self.gen_expr(jit, b, idx)?;
                        let i0 = self.convert(b, &is, &Ty::I64);
                        let f = self.rt_ref(jit, b, "list_len")?;
                        let lc = b.ins().call(f, &[bs.0]);
                        let len = b.inst_results(lc)[0];
                        let i = self.norm_idx(b, i0, len);
                        self.gen_bounds(jit, b, i, len, e.line)?;
                        let f = self.rt_ref(jit, b, "list_at")?;
                        let call = b.ins().call(f, &[bs.0, i]);
                        Ok((b.inst_results(call)[0], (**el).clone()))
                    }
                    Ty::Map(_, v) => {
                        let is = self.gen_expr(jit, b, idx)?;
                        let k = self.convert(b, &is, &Ty::I64);
                        let f = self.rt_ref(jit, b, "map_get")?;
                        let call = b.ins().call(f, &[bs.0, k]);
                        Ok((b.inst_results(call)[0], (**v).clone()))
                    }
                    other => Err(crate::lb!(e.line, "{} does not support indexing", "{} 不支持下标访问", other)),
                }
            }
            ExprKind::Slice(base, lo, hi) => {
                let bs = self.gen_expr(jit, b, base)?;
                let ls = self.gen_expr(jit, b, lo)?;
                let hs = self.gen_expr(jit, b, hi)?;
                let l = self.convert(b, &ls, &Ty::I64);
                let h = self.convert(b, &hs, &Ty::I64);
                // list 切片：返回新 list
                if matches!(bs.1, Ty::List(_)) {
                    let f = self.rt_ref(jit, b, "list_slice")?;
                    let call = b.ins().call(f, &[bs.0, l, h]);
                    return Ok((b.inst_results(call)[0], Ty::List(Box::new(Ty::I64))));
                }
                let len = b.ins().isub(h, l);
                let f = self.rt_ref(jit, b, "str_substr")?;
                let call = b.ins().call(f, &[bs.0, l, len]);
                Ok((b.inst_results(call)[0], Ty::Str))
            }
            ExprKind::ArrayLit(items) => {
                let key = e as *const Expr as usize;
                let (elem, n) = match &e.ty { Ty::Array(el, n) => ((**el).clone(), *n), _ => (Ty::I64, items.len()) };
                let slot = match self.array_slots.get(&key) {
                    Some(s) => *s,
                    None => {
                        let s = b.func.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, (n.max(1) * 8) as u32, 3));
                        self.array_slots.insert(key, s); s
                    }
                };
                let base = b.ins().stack_addr(types::I64, slot, 0);
                for (i, it) in items.iter().enumerate() {
                    let v = self.gen_expr(jit, b, it)?;
                    let v = self.convert(b, &v, &elem);
                    b.ins().store(MemFlags::new(), v, base, (i * 8) as i32);
                }
                Ok((base, Ty::Array(Box::new(elem), n)))
            }
            ExprKind::StructLit(name, fields) => {
                let layout = jit.structs.get(name).cloned().ok_or_else(|| crate::lb!(e.line, "undefined struct '{}'", "未定义的结构体 '{}'", name))?;
                // 堆分配：结构体可能作为返回值跨栈帧存活（栈槽会悬垂）
                let nbytes = (layout.len().max(1) * 8) as i64;
                let f_alloc = self.rt_ref(jit, b, "mem_alloc")?;
                let nb = b.ins().iconst(types::I64, nbytes);
                let call = b.ins().call(f_alloc, &[nb]);
                let base = b.inst_results(call)[0];
                let zero = b.ins().iconst(types::I64, 0);
                for i in 0..layout.len() { b.ins().store(MemFlags::new(), zero, base, (i * 8) as i32); }
                for (fname, v) in fields {
                    let idx = layout.iter().position(|(n, _)| n == fname).ok_or_else(|| crate::lb!(e.line, "struct '{}' has no field '{}'", "结构体 '{}' 没有字段 '{}'", name, fname))?;
                    let fty = layout[idx].1.clone();
                    let val = self.gen_expr(jit, b, v)?;
                    let val = self.convert(b, &val, &fty);
                    b.ins().store(MemFlags::new(), val, base, (idx * 8) as i32);
                }
                Ok((base, Ty::Struct(name.clone())))
            }
            ExprKind::Field(base, field) => {
                let bv = self.gen_expr(jit, b, base)?;
                if let Ty::Tuple(ts) = &bv.1 {
                    let idx: usize = field.parse().unwrap_or(usize::MAX);
                    let fty = ts.get(idx).cloned().ok_or_else(|| crate::lb!(e.line, "tuple index out of range", "元组下标越界"))?;
                    let ld = b.ins().load(cl_ty(&fty), MemFlags::new(), bv.0, (idx * 8) as i32);
                    return Ok((ld, fty));
                }
                let sname = match &bv.1 { Ty::Struct(n) => n.clone(), other => return Err(crate::lb!(e.line, "{} is not a struct; cannot access field '{}'", "{} 不是结构体，不能访问字段 '{}'", other, field)) };
                let layout = jit.structs.get(&sname).cloned().ok_or_else(|| crate::lb!(e.line, "undefined struct '{}'", "未定义的结构体 '{}'", sname))?;
                let idx = layout.iter().position(|(n, _)| n == field).ok_or_else(|| crate::lb!(e.line, "struct '{}' has no field '{}'", "结构体 '{}' 没有字段 '{}'", sname, field))?;
                let fty = layout[idx].1.clone();
                let ld = b.ins().load(cl_ty(&fty), MemFlags::new(), bv.0, (idx * 8) as i32);
                Ok((ld, fty))
            }
            ExprKind::Match { subject, arms } => self.gen_match(jit, b, subject, arms, &e.ty.clone(), e.line),
            ExprKind::If { cond, then, els } => {
                let want = e.ty.clone();
                self.gen_if_value(jit, b, cond, then, els.as_ref(), &want)
                    .map(|v| (v.unwrap_or_else(|| b.ins().iconst(types::I64, 0)), want))
            }
            ExprKind::None => {
                let zero = b.ins().iconst(types::I64, 0);
                let one = b.ins().iconst(types::I64, 1);
                let f = self.rt_ref(jit, b, "result_new")?;
                let call = b.ins().call(f, &[one, zero]);
                return Ok((b.inst_results(call)[0], e.ty.clone()));
            }
            ExprKind::TryBlock { body, catches, fin } => {
                let want = Ty::Result(Box::new(Ty::I64), Box::new(Ty::I64));
                let slot = b.func.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                let saddr = b.ins().stack_addr(types::I64, slot, 0);
                let l_err = b.create_block();
                let l_ok = b.create_block();
                let l_end = b.create_block();
                let fvar = self.new_var(b, &Ty::I64);
                let zero = b.ins().iconst(types::I64, 0);
                b.def_var(fvar, zero);
                self.fault_stack.push((l_err, fvar));
                let rv = self.gen_block_value(jit, b, body, &want)?;
                self.fault_stack.pop();
                if let Some(v) = rv {
                    let f_tag = self.rt_ref(jit, b, "result_tag")?;
                    let call = b.ins().call(f_tag, &[v]);
                    let tag = b.inst_results(call)[0];
                    let f_val = self.rt_ref(jit, b, "result_val")?;
                    let call2 = b.ins().call(f_val, &[v]);
                    let val = b.inst_results(call2)[0];
                    b.ins().store(MemFlags::new(), val, saddr, 0);
                    let is_err = b.ins().icmp_imm(IntCC::NotEqual, tag, 0);
                    b.ins().brif(is_err, l_err, &[], l_ok, &[]);
                    b.switch_to_block(l_ok); self.terminated = false;
                    b.ins().jump(l_end, &[]);
                    b.switch_to_block(l_err); self.terminated = false;
                    let fv = b.use_var(fvar);
                    let is_fault = b.ins().icmp_imm(IntCC::NotEqual, fv, 0);
                    let ev2 = b.ins().select(is_fault, fv, val);
                    if let Some(ca) = catches.first() {
                        self.push_scope();
                        if let Some(binding) = &ca.binding {
                            let bv = self.new_var(b, &Ty::I64);
                            b.def_var(bv, ev2);
                            self.bind(binding, bv, Ty::I64);
                        }
                        let hv = self.gen_block_value(jit, b, &ca.body, &Ty::I64)?;
                        if let Some(hv) = hv {
                            b.ins().store(MemFlags::new(), hv, saddr, 0);
                        }
                        self.pop_scope();
                    }
                    if !self.terminated { b.ins().jump(l_end, &[]); }
                    b.switch_to_block(l_end); self.terminated = false;
                }
                if let Some(f) = fin {
                    self.push_scope();
                    self.gen_block(jit, b, f)?;
                    self.pop_scope();
                }
                let r = b.ins().load(types::I64, MemFlags::new(), saddr, 0);
                return Ok((r, Ty::I64));
            }
            ExprKind::Some(inner) => {
                let v = self.gen_expr(jit, b, inner)?;
                let iv = self.convert(b, &v, &Ty::I64);
                let zero = b.ins().iconst(types::I64, 0);
                let f = self.rt_ref(jit, b, "result_new")?;
                let call = b.ins().call(f, &[zero, iv]);
                return Ok((b.inst_results(call)[0], e.ty.clone()));
            }
            ExprKind::Ok(inner) => {
                let v = self.gen_expr(jit, b, inner)?;
                let iv = self.convert(b, &v, &Ty::I64);
                let zero = b.ins().iconst(types::I64, 0);
                let f = self.rt_ref(jit, b, "result_new")?;
                let call = b.ins().call(f, &[zero, iv]);
                Ok((b.inst_results(call)[0], e.ty.clone()))
            }
            ExprKind::Err(inner) => {
                let v = self.gen_expr(jit, b, inner)?;
                let iv = self.convert(b, &v, &Ty::I64);
                let one = b.ins().iconst(types::I64, 1);
                let f = self.rt_ref(jit, b, "result_new")?;
                let call = b.ins().call(f, &[one, iv]);
                Ok((b.inst_results(call)[0], e.ty.clone()))
            }
            ExprKind::Try(inner) => {
                let v = self.gen_expr(jit, b, inner)?;
                let f_tag = self.rt_ref(jit, b, "result_tag")?;
                let call = b.ins().call(f_tag, &[v.0]);
                let tag = b.inst_results(call)[0];
                // tag == 1 → 提前返回该 Result
                let err_blk = self.new_block(b);
                let ok_blk = self.new_block(b);
                let is_err = b.ins().icmp_imm(IntCC::NotEqual, tag, 0);
                b.ins().brif(is_err, err_blk, &[], ok_blk, &[]);
                b.switch_to_block(err_blk); self.terminated = false;
                if let Some((blk, var)) = self.fault_stack.last().cloned() {
                    let f_val = self.rt_ref(jit, b, "result_val")?;
                    let call = b.ins().call(f_val, &[v.0]);
                    let ev = b.inst_results(call)[0];
                    b.def_var(var, ev);
                    b.ins().jump(blk, &[]);
                } else {
                    b.ins().return_(&[v.0]);
                }
                b.switch_to_block(ok_blk); self.terminated = false;
                let f_val = self.rt_ref(jit, b, "result_val")?;
                let call = b.ins().call(f_val, &[v.0]);
                let raw = b.inst_results(call)[0];
                // 解包为内层类型
                let inner_ty = match &e.ty { Ty::Unknown => Ty::I64, t => t.clone() };
                let val = if inner_ty == Ty::F64 { b.ins().bitcast(types::F64, MemFlags::new(), raw) } else { raw };
                Ok((val, inner_ty))
            }
            ExprKind::Borrow { inner, .. } => self.gen_expr(jit, b, inner),
            ExprKind::Closure { .. } => Err(crate::lb!(e.line, "closure was not hoisted (internal error)", "闭包未被提升（内部错误）")),
            ExprKind::ClosureNew { fn_name, captures } => {
                let f = self.rt_ref(jit, b, "mem_alloc")?;
                let n = b.ins().iconst(types::I64, ((captures.len() + 1) * 8) as i64);
                let call = b.ins().call(f, &[n]);
                let blk = b.inst_results(call)[0];
                let fid = jit.fns.get(fn_name).map(|x| x.fid).ok_or_else(|| crate::lb!(e.line, "closure function '{}' not found", "闭包函数 '{}' 未找到", fn_name))?;
                let fref = jit.module.declare_func_in_func(fid, b.func);
                let fp = b.ins().func_addr(types::I64, fref);
                b.ins().store(MemFlags::new(), fp, blk, 0);
                for (i, c) in captures.iter().enumerate() {
                    let v = self.gen_expr(jit, b, c)?;
                    let iv = self.convert(b, &v, &Ty::I64);
                    b.ins().store(MemFlags::new(), iv, blk, ((i + 1) * 8) as i32);
                }
                Ok((blk, e.ty.clone()))
            }
        }
    }

    pub(crate) fn gen_logic(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, op: BinOp, a: &Expr, c: &Expr) -> Result<(Value, Ty), String> {
        let is_and = op == BinOp::And;
        let slot = self.new_var(b, &Ty::Bool);
        let rhs_blk = self.new_block(b);
        let end_blk = self.new_block(b);
        let lv = self.gen_cond(jit, b, a)?;
        let lvi = b.ins().uextend(types::I64, lv);
        b.def_var(slot, lvi);
        if is_and { b.ins().brif(lv, rhs_blk, &[], end_blk, &[]); }
        else { b.ins().brif(lv, end_blk, &[], rhs_blk, &[]); }
        b.switch_to_block(rhs_blk); self.terminated = false;
        let rv = self.gen_cond(jit, b, c)?;
        let rvi = b.ins().uextend(types::I64, rv);
        b.def_var(slot, rvi);
        b.ins().jump(end_blk, &[]);
        b.switch_to_block(end_blk); self.terminated = false;
        Ok((b.use_var(slot), Ty::Bool))
    }

    pub(crate) fn gen_const(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, c: &ConstVal) -> (Value, Ty) {
        match &c.val {
            CVal::Int(i) => (b.ins().iconst(types::I64, *i), Ty::I64),
            CVal::Float(f) => (b.ins().f64const(*f), Ty::F64),
            CVal::Bool(v) => (b.ins().iconst(types::I64, *v as i64), Ty::Bool),
            CVal::Str(s) => match jit.data_id_of(s.as_bytes()) { Some(id) => (data_ptr(jit, b, id), Ty::Str), None => (b.ins().iconst(types::I64, 0), Ty::Str) },
        }
    }

    /// 构造 trait 对象：堆块 [data, addr0, ...]（JIT 版）。
    pub(crate) fn gen_dyn_box(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, trait_name: &str, value: &Expr) -> Result<(Value, Ty), String> {
        let v = self.gen_expr(jit, b, value)?;
        let conc = match &v.1 {
            Ty::Struct(n) => n.clone(),
            Ty::Enum(n) => n.clone(),
            _ => String::new(),
        };
        let methods = jit.trait_impls.get(&(conc, trait_name.to_string())).cloned().unwrap_or_default();
        let n = methods.len() + 1;
        let f = self.rt_ref(jit, b, "mem_alloc")?;
        let sz = b.ins().iconst(types::I64, (n * 8) as i64);
        let call = b.ins().call(f, &[sz]);
        let base = b.inst_results(call)[0];
        let data = if v.1.llvm() == "ptr" {
            b.ins().bitcast(types::I64, MemFlags::new(), v.0)
        } else { v.0 };
        b.ins().store(MemFlags::new(), data, base, 0);
        for (i, mn) in methods.iter().enumerate() {
            let addr = if mn.is_empty() { b.ins().iconst(types::I64, 0) } else {
                match jit.fns.get(mn) {
                    Some(info) => {
                        let fref = jit.module.declare_func_in_func(info.fid, b.func);
                        b.ins().func_addr(types::I64, fref)
                    }
                    None => b.ins().iconst(types::I64, 0),
                }
            };
            b.ins().store(MemFlags::new(), addr, base, ((i + 1) * 8) as i32);
        }
        Ok((base, Ty::Dyn(trait_name.to_string())))
    }

}
