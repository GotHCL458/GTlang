//! FnState 的语句代码生成。

use super::*;

impl FnState {
    pub(crate) fn gen_stmt(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, s: &Stmt) -> Result<(), String> {
        if self.terminated { return Ok(()); }
        match s {
            Stmt::Labeled { inner, .. } => {
                let inner_blk: Block = vec![(**inner).clone()];
                self.gen_block(jit, b, &inner_blk)?;
            }
            Stmt::Let { name, ty, value, .. } => {
                let v = self.gen_expr(jit, b, value)?;
                let vt = if v.1 == Ty::Unknown { ty.clone().unwrap_or(Ty::I64) } else { v.1.clone() };
                let var = self.new_var(b, &vt);
                let cv = self.convert(b, &v, &vt);
                b.def_var(var, cv);
                self.bind(name, var, vt);
            }
            Stmt::Const { name, ty, value, .. } => {
                let v = self.gen_expr(jit, b, value)?;
                let vt = if v.1 == Ty::Unknown { ty.clone().unwrap_or(Ty::I64) } else { v.1.clone() };
                let var = self.new_var(b, &vt);
                let cv = self.convert(b, &v, &vt);
                b.def_var(var, cv);
                self.bind(name, var, vt);
            }
            Stmt::Go { func, args, line } => {
                let info = jit.fns.get(func).map(|x| x.fid).ok_or_else(|| crate::lb!(*line, "undefined function '{}'", "未定义的函数 '{}'", func))?;
                let fref = jit.module.declare_func_in_func(info, b.func);
                let fp = b.ins().func_addr(types::I64, fref);
                // 参数打包
                let f = self.rt_ref(jit, b, "mem_alloc")?;
                let n = b.ins().iconst(types::I64, (args.len().max(1) * 8) as i64);
                let call = b.ins().call(f, &[n]);
                let pack = b.inst_results(call)[0];
                for (i, a) in args.iter().enumerate() {
                    let v = self.gen_expr(jit, b, a)?;
                    let iv = self.convert(b, &v, &Ty::I64);
                    b.ins().store(MemFlags::new(), iv, pack, (i * 8) as i32);
                }
                let nargs = b.ins().iconst(types::I64, args.len() as i64);
                let sp = self.rt_ref(jit, b, "thread_spawn")?;
                b.ins().call(sp, &[fp, pack, nargs]);
            }
            Stmt::Asm { line, .. } => {
                return Err(crate::lb!(line, "inline asm is only supported by the compiler backend, not the interpreter", "内联汇编仅编译器后端支持，解释器不支持"));
            }
            Stmt::Throw(e, _) => {
                // `throw e`：构造 Err(e)。在 try 内跳到捕获块；否则从函数返回。
                let v = self.gen_expr(jit, b, e)?;
                let iv = self.convert(b, &v, &Ty::I64);
                let one = b.ins().iconst(types::I64, 1);
                let f = self.rt_ref(jit, b, "result_new")?;
                let call = b.ins().call(f, &[one, iv]);
                let r = b.inst_results(call)[0];
                if let Some((blk, var)) = self.fault_stack.last().cloned() {
                    // 取 Err payload 存变量，跳捕获块
                    let f_val = self.rt_ref(jit, b, "result_val")?;
                    let call = b.ins().call(f_val, &[r]);
                    let ev = b.inst_results(call)[0];
                    b.def_var(var, ev);
                    b.ins().jump(blk, &[]);
                } else {
                    b.ins().return_(&[r]);
                }
                self.terminated = true;
            }
            Stmt::Try { body, catches, fin, .. } => {
                // try { body } expt e { h } fily { f }：
                // 1) body 末表达式是 Result（Err → 捕获）
                // 2) try 内运行时故障（除零/越界/溢出）→ 跳到捕获块（阶段 4C）
                let want = Ty::Result(Box::new(Ty::I64), Box::new(Ty::I64));
                let l_err = b.create_block();
                let l_ok = b.create_block();
                let l_end = b.create_block();
                let fvar = self.new_var(b, &Ty::I64);
                let zero = b.ins().iconst(types::I64, 0);
                b.def_var(fvar, zero);
                self.fault_stack.push((l_err, fvar));
                let rv = self.gen_block_value(jit, b, body, &want)?;
                self.fault_stack.pop();
                // body 正常结束（有值）：按 Result 的 tag 分发到 l_ok / l_err
                if let Some(v) = rv {
                    let f_tag = self.rt_ref(jit, b, "result_tag")?;
                    let call = b.ins().call(f_tag, &[v]);
                    let tag = b.inst_results(call)[0];
                    let is_err = b.ins().icmp_imm(IntCC::NotEqual, tag, 0);
                    b.ins().brif(is_err, l_err, &[], l_ok, &[]);
                    // Ok 分支（l_ok 由后面统一处理，这里直接跳 l_end）
                    b.switch_to_block(l_ok); self.terminated = false;
                    b.ins().jump(l_end, &[]);
                    // l_ok 已在上面 switch 并 jump（跳到 l_end），无需重复。
                } else {
                    // body 无值（如以 throw 结束，已跳到 l_err）：跳 l_ok（空）避免块未终结
                    if !self.terminated { b.ins().jump(l_ok, &[]); }
                    // l_ok：正常路径（无额外动作）
                    b.switch_to_block(l_ok); self.terminated = false;
                    b.ins().jump(l_end, &[]);
                }
                // ---- l_err 统一入口 ----
                b.switch_to_block(l_err); self.terminated = false;
                let fv = b.use_var(fvar);
                let ev2 = fv;
                if let Some(ca) = catches.first() {
                    self.push_scope();
                    if let Some(binding) = &ca.binding {
                        let bv = self.new_var(b, &Ty::I64);
                        b.def_var(bv, ev2);
                        self.bind(binding, bv, Ty::I64);
                    }
                    let _ = self.gen_block_value(jit, b, &ca.body, &Ty::I64)?;
                    self.pop_scope();
                }
                if !self.terminated { b.ins().jump(l_end, &[]); }
                b.switch_to_block(l_end); self.terminated = false;
                // fily
                if let Some(f) = fin {
                    self.push_scope();
                    self.gen_block(jit, b, f)?;
                    self.pop_scope();
                }
            }
            Stmt::Assign { name, index, op, value, line } => {
                let (var, ty) = match self.lookup(name) {
                    Some(x) => x,
                    None if index.is_none() && op.is_none() => {
                        // 裸赋值 x = v：自动声明（类型由 RHS 推导）
                        let rhs = self.gen_expr(jit, b, value)?;
                        let nty = if rhs.1 == Ty::Unknown { Ty::I64 } else { rhs.1.clone() };
                        let nvar = self.new_var(b, &nty);
                        let res = self.convert(b, &rhs, &nty);
                        b.def_var(nvar, res);
                        self.bind(name, nvar, nty.clone());
                        return Ok(());
                    }
                    None => return Err(crate::lb!(line, "undefined variable '{}'", "未定义的变量 '{}'", name)),
                };
                match index {
                    Some(idx) => {
                        let base = b.use_var(var);
                        // List / Map：走运行时 set / insert（引用语义句柄）
                        match &ty {
                            Ty::List(el) => {
                                let ie = self.gen_expr(jit, b, idx)?;
                                let i = self.convert(b, &ie, &Ty::I64);
                                let rhs = self.gen_expr(jit, b, value)?;
                                let rv = self.to_slot(b, &rhs, el);
                                let f = self.rt_ref(jit, b, "list_set")?;
                                b.ins().call(f, &[base, i, rv]);
                            }
                            Ty::Map(_, v) => {
                                let ke = self.gen_expr(jit, b, idx)?;
                                let k = self.convert(b, &ke, &Ty::I64);
                                let rhs = self.gen_expr(jit, b, value)?;
                                // map 的值类型可能仍是 Unknown（未细化）；此时按 rhs 的实际类型决定
                                // 是否需按位保真（f64 -> i64 槽）。
                                let vt = if **v == Ty::Unknown { rhs.1.clone() } else { (**v).clone() };
                                let rv = self.to_slot(b, &rhs, &vt);
                                let f = self.rt_ref(jit, b, "map_insert")?;
                                b.ins().call(f, &[base, k, rv]);
                            }
                            _ => {
                                let (elem, addr) = self.gen_elem_addr(jit, b, &base, &ty, idx, *line)?;
                                let rhs = self.gen_expr(jit, b, value)?;
                                let safe = self.range.as_ref().map(|ra| ra.is_stmt_safe(s)).unwrap_or(false);
                                let res = match op {
                                    Some(o) => { let cur = b.ins().load(cl_ty(&elem), MemFlags::new(), addr, 0); self.gen_binop(jit, b, *o, &(cur, elem.clone()), &rhs, *line, safe)?.0 }
                                    None => self.convert(b, &rhs, &elem),
                                };
                                b.ins().store(MemFlags::new(), res, addr, 0);
                            }
                        }
                    }
                    None => {
                        let rhs = self.gen_expr(jit, b, value)?;
                        if op.is_none() && rhs.1 != Ty::Unknown && rhs.1 != ty {
                            // 类型改变：新建变量并重新绑定
                            let nty = rhs.1.clone();
                            let nvar = self.new_var(b, &nty);
                            let res = self.convert(b, &rhs, &nty);
                            b.def_var(nvar, res);
                            self.rebind(name, nvar, nty);
                        } else {
                            let cur = b.use_var(var);
                            let curv = (cur, ty.clone());
                            let safe = self.range.as_ref().map(|ra| ra.is_stmt_safe(s)).unwrap_or(false);
                            let res = match op { Some(o) => self.gen_binop(jit, b, *o, &curv, &rhs, *line, safe)?.0, None => self.convert(b, &rhs, &ty) };
                            b.def_var(var, res);
                        }
                    }
                }
            }
            Stmt::FieldAssign { obj, field, op, value, line } => {
                let (var, ty) = self.lookup(obj).ok_or_else(|| crate::lb!(line, "undefined variable '{}'", "未定义的变量 '{}'", obj))?;
                let base = b.use_var(var);
                // 借用自动解引用：`a := &mut p; a.x = v`
                let bt = match &ty { Ty::Ref(t) | Ty::RefMut(t) => (**t).clone(), other => other.clone() };
                let sname = match &bt { Ty::Struct(n) => n.clone(), other => return Err(crate::lb!(line, "{} is not a struct", "{} 不是结构体", other)) };
                let layout = jit.structs.get(&sname).cloned().ok_or_else(|| crate::lb!(line, "undefined struct '{}'", "未定义的结构体 '{}'", sname))?;
                let idx = layout.iter().position(|(n, _)| n == field).ok_or_else(|| crate::lb!(line, "no field '{}'", "无字段 '{}'", field))?;
                let fty = layout[idx].1.clone();
                let addr = b.ins().iadd_imm(base, (idx * 8) as i64);
                let rhs = self.gen_expr(jit, b, value)?;
                let safe = self.range.as_ref().map(|ra| ra.is_stmt_safe(s)).unwrap_or(false);
                let res = match op {
                    Some(o) => { let cur = b.ins().load(cl_ty(&fty), MemFlags::new(), addr, 0); self.gen_binop(jit, b, *o, &(cur, fty.clone()), &rhs, *line, safe)?.0 }
                    None => self.convert(b, &rhs, &fty),
                };
                b.ins().store(MemFlags::new(), res, addr, 0);
            }
            Stmt::Expr(e) => { self.gen_expr(jit, b, e)?; }
            Stmt::If { cond, then, els, .. } => { self.gen_if(jit, b, cond, then, els.as_ref())?; }
            Stmt::While { cond, body, .. } => {
                let header = self.new_block(b); let bodyb = self.new_block(b); let exit = self.new_block(b);
                b.ins().jump(header, &[]);
                b.switch_to_block(header); self.terminated = false;
                let c = self.gen_cond(jit, b, cond)?;
                b.ins().brif(c, bodyb, &[], exit, &[]);
                b.switch_to_block(bodyb); self.terminated = false;
                self.loops.push((exit, header));
                self.gen_block(jit, b, body)?;
                self.loops.pop();
                if !self.terminated { b.ins().jump(header, &[]); }
                b.switch_to_block(exit); self.terminated = false;
            }
            Stmt::DoWhile { body, cond, .. } => {
                let bodyb = self.new_block(b); let condb = self.new_block(b); let exit = self.new_block(b);
                b.ins().jump(bodyb, &[]);
                b.switch_to_block(bodyb); self.terminated = false;
                self.loops.push((exit, condb));
                self.gen_block(jit, b, body)?;
                self.loops.pop();
                if !self.terminated { b.ins().jump(condb, &[]); }
                b.switch_to_block(condb); self.terminated = false;
                let c = self.gen_cond(jit, b, cond)?;
                b.ins().brif(c, bodyb, &[], exit, &[]);
                b.switch_to_block(exit); self.terminated = false;
            }
            Stmt::ForRange { var, from, to, body, els, .. } => {
                let fv = self.gen_expr(jit, b, from)?; let fv = self.convert(b, &fv, &Ty::I64);
                let tv = self.gen_expr(jit, b, to)?; let tv = self.convert(b, &tv, &Ty::I64);
                let iv = self.new_var(b, &Ty::I64); b.def_var(iv, fv);
                let header = self.new_block(b); let bodyb = self.new_block(b); let inc = self.new_block(b); let exit = self.new_block(b);
                b.ins().jump(header, &[]);
                b.switch_to_block(header);
                let cur = b.use_var(iv);
                let c = b.ins().icmp(IntCC::SignedLessThan, cur, tv);
                b.ins().brif(c, bodyb, &[], exit, &[]);
                b.switch_to_block(bodyb); self.terminated = false;
                self.push_scope(); self.bind(var, iv, Ty::I64); self.loops.push((exit, inc));
                // 若为 `for v in 0..N`（字面量），记录上界 N 以便省略 a[v] 的边界检查
                let bounded = matches!(&from.kind, ExprKind::Int(0))
                    .then(|| if let ExprKind::Int(n) = &to.kind { Some(*n) } else { None })
                    .flatten();
                if let Some(n) = bounded { self.bounded.insert(var.clone(), n); }
                self.gen_block(jit, b, body)?;
                if bounded.is_some() { self.bounded.remove(var); }
                self.pop_scope(); self.loops.pop();
                if !self.terminated { b.ins().jump(inc, &[]); }
                b.switch_to_block(inc); self.terminated = false;
                let c2 = b.use_var(iv); let nx = b.ins().iadd_imm(c2, 1); b.def_var(iv, nx);
                b.ins().jump(header, &[]);
                b.switch_to_block(exit); self.terminated = false;
                // for-else：正常结束（i 达到终值）才执行 else
                if let Some(els) = els {
                    let done_blk = self.new_block(b);
                    let skip_blk = self.new_block(b);
                    let ci = b.use_var(iv);
                    let done = b.ins().icmp(IntCC::Equal, ci, tv);
                    b.ins().brif(done, done_blk, &[], skip_blk, &[]);
                    b.switch_to_block(done_blk); self.terminated = false;
                    self.push_scope(); self.gen_block(jit, b, els)?; self.pop_scope();
                    if !self.terminated { b.ins().jump(skip_blk, &[]); }
                    b.switch_to_block(skip_blk); self.terminated = false;
                }
            }
            Stmt::ForEach { var, iter, body, els: _, line } => {
                let arr = self.gen_expr(jit, b, iter)?;
                let is_list = matches!(&arr.1, Ty::List(_));
                let is_str = matches!(&arr.1, Ty::Str);
                let is_set = matches!(&arr.1, Ty::Set(_));
                let is_map = matches!(&arr.1, Ty::Map(..));
                let elem = match &arr.1 {
                    Ty::Array(e, _) => (**e).clone(),
                    Ty::List(e) => (**e).clone(),
                    Ty::Str => Ty::Str,
                    Ty::Set(e) => (**e).clone(),
                    Ty::Map(k, _) => (**k).clone(),
                    other => return Err(crate::lb!(line, "for can only iterate over arrays/lists/strings/sets/maps, found {}", "for 只能遍历数组/列表/字符串/集合/映射，实际是 {}", other)),
                };
                let base = arr.0;
                let n_const: i64 = match &arr.1 { Ty::Array(_, n) => *n as i64, _ => 0 };
                let idx = self.new_var(b, &Ty::I64);
                let zero = b.ins().iconst(types::I64, 0); b.def_var(idx, zero);
                let ev = self.new_var(b, &elem); b.def_var(ev, zero);
                let header = self.new_block(b); let bodyb = self.new_block(b); let inc = self.new_block(b); let exit = self.new_block(b);
                // 长度在循环外计算一次
                let len_val = if is_list {
                    let f = self.rt_ref(jit, b, "list_len")?;
                    let call = b.ins().call(f, &[base]);
                    Some(b.inst_results(call)[0])
                } else if is_str {
                    let f = self.rt_ref(jit, b, "str_char_len")?;
                    let call = b.ins().call(f, &[base]);
                    Some(b.inst_results(call)[0])
                } else if is_set {
                    let f = self.rt_ref(jit, b, "set_len")?;
                    let call = b.ins().call(f, &[base]);
                    Some(b.inst_results(call)[0])
                } else if is_map {
                    let f = self.rt_ref(jit, b, "map_len")?;
                    let call = b.ins().call(f, &[base]);
                    Some(b.inst_results(call)[0])
                } else { None };
                b.ins().jump(header, &[]);
                b.switch_to_block(header);
                let i1 = b.use_var(idx);
                let c = match len_val {
                    Some(lv) => b.ins().icmp(IntCC::SignedLessThan, i1, lv),
                    None => b.ins().icmp_imm(IntCC::SignedLessThan, i1, n_const),
                };
                b.ins().brif(c, bodyb, &[], exit, &[]);
                b.switch_to_block(bodyb); self.terminated = false;
                let i2 = b.use_var(idx);
                let ld = if is_list {
                    // 内联 `data[i]`（RtList 首字段 rc 占 8 字节，data 在偏移 8），省函数调用与边界检查
                    // （循环上界已保证 i < len）
                    let data = b.ins().load(types::I64, MemFlags::new(), base, 8);
                    let off = b.ins().imul_imm(i2, 8);
                    let addr = b.ins().iadd(data, off);
                    let raw = b.ins().load(types::I64, MemFlags::new(), addr, 0);
                    if elem == Ty::F64 { b.ins().bitcast(types::F64, MemFlags::new(), raw) } else { raw }
                } else if is_str {
                    let f = self.rt_ref(jit, b, "str_char_at")?;
                    let call = b.ins().call(f, &[base, i2]);
                    b.inst_results(call)[0]
                } else if is_set {
                    let f = self.rt_ref(jit, b, "set_at")?;
                    let call = b.ins().call(f, &[base, i2]);
                    let raw = b.inst_results(call)[0];
                    if elem == Ty::F64 { b.ins().bitcast(types::F64, MemFlags::new(), raw) } else { raw }
                } else if is_map {
                    let f = self.rt_ref(jit, b, "map_key_at")?;
                    let call = b.ins().call(f, &[base, i2]);
                    let raw = b.inst_results(call)[0];
                    if elem == Ty::F64 { b.ins().bitcast(types::F64, MemFlags::new(), raw) } else { raw }
                } else {
                    let off = b.ins().imul_imm(i2, 8); let addr = b.ins().iadd(base, off);
                    b.ins().load(cl_ty(&elem), MemFlags::new(), addr, 0)
                };
                b.def_var(ev, ld);
                self.push_scope(); self.bind(var, ev, elem); self.loops.push((exit, inc));
                self.gen_block(jit, b, body)?;
                self.pop_scope(); self.loops.pop();
                if !self.terminated { b.ins().jump(inc, &[]); }
                b.switch_to_block(inc); self.terminated = false;
                let c2 = b.use_var(idx); let nx = b.ins().iadd_imm(c2, 1); b.def_var(idx, nx);
                b.ins().jump(header, &[]);
                b.switch_to_block(exit); self.terminated = false;
            }
            Stmt::Return(Some(e), _) => {
                // impl Trait 返回位置：具体类型自动装箱为 dyn Trait
                if let Ty::Dyn(tr) = self.cur_ret.clone() {
                    if matches!(e.ty, Ty::Struct(_) | Ty::Enum(_)) {
                        let v = self.gen_expr(jit, b, e)?;
                        let boxed = self.gen_dyn_box(jit, b, &tr, e)?;
                        let _ = v;
                        b.ins().return_(&[boxed.0]);
                        self.terminated = true;
                        return Ok(());
                    }
                }
                let v = self.gen_expr(jit, b, e)?;
                let want = self.cur_ret.clone();
                let v = self.convert(b, &v, &want);
                b.ins().return_(&[v]); self.terminated = true;
            }
            Stmt::Return(None, _) => { b.ins().return_(&[]); self.terminated = true; }
            Stmt::Break(_, _) => { if let Some((brk, _)) = self.loops.last() { let t = *brk; b.ins().jump(t, &[]); self.terminated = true; } }
            Stmt::Continue(_, _) => { if let Some((_, cont)) = self.loops.last() { let t = *cont; b.ins().jump(t, &[]); self.terminated = true; } }
            Stmt::Block(inner) => { self.gen_block(jit, b, inner)?; }
            Stmt::LocalFn(_) => {}
        }
        Ok(())
    }

    pub(crate) fn gen_if(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, cond: &Expr, then: &Block, els: Option<&Block>) -> Result<(), String> {
        let c = self.gen_cond(jit, b, cond)?;
        let tb = self.new_block(b); let eb = self.new_block(b); let exit = self.new_block(b);
        b.ins().brif(c, tb, &[], eb, &[]);
        b.switch_to_block(tb); self.terminated = false;
        self.gen_block(jit, b, then)?;
        if !self.terminated { b.ins().jump(exit, &[]); }
        b.switch_to_block(eb); self.terminated = false;
        if let Some(e) = els { self.gen_block(jit, b, e)?; }
        if !self.terminated { b.ins().jump(exit, &[]); }
        b.switch_to_block(exit); self.terminated = false;
        Ok(())
    }

    pub(crate) fn gen_if_value(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, cond: &Expr, then: &Block, els: Option<&Block>, want: &Ty) -> Result<Option<Value>, String> {
        let c = self.gen_cond(jit, b, cond)?;
        let tb = self.new_block(b); let eb = self.new_block(b); let exit = self.new_block(b);
        let slot = b.func.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        b.ins().brif(c, tb, &[], eb, &[]);
        b.switch_to_block(tb); self.terminated = false;
        let tv = self.gen_block_value(jit, b, then, want)?;
        if !self.terminated {
            if let Some(v) = tv { let a = b.ins().stack_addr(types::I64, slot, 0); b.ins().store(MemFlags::new(), v, a, 0); }
            b.ins().jump(exit, &[]);
        }
        b.switch_to_block(eb); self.terminated = false;
        let ev = match els { Some(blk) => self.gen_block_value(jit, b, blk, want)?, None => None };
        if !self.terminated {
            if let Some(v) = ev { let a = b.ins().stack_addr(types::I64, slot, 0); b.ins().store(MemFlags::new(), v, a, 0); }
            b.ins().jump(exit, &[]);
        }
        b.switch_to_block(exit); self.terminated = false;
        let a = b.ins().stack_addr(types::I64, slot, 0);
        Ok(Some(b.ins().load(cl_ty(want), MemFlags::new(), a, 0)))
    }

    /// 递归判定解构模式的内层 tag（Result/Option）：`Some`/`Ok`→0，`None`/`Err`→1。
    /// 用于 `E::A(Some(v))` 与 `E::A(None)` 共存时区分内层。
    fn pat_tag_cond(&mut self, b: &mut FunctionBuilder, ty: &Ty, val: Value, pat: &Expr) -> Value {
        let want_tag: i64 = match &pat.kind {
            ExprKind::None | ExprKind::Err(_) => 1,
            ExprKind::Call(n, _) if n == "None" || n == "Err" => 1,
            _ => 0,
        };
        let _ = ty;
        let tag = b.ins().load(types::I64, MemFlags::new(), val, 0);
        b.ins().icmp_imm(IntCC::Equal, tag, want_tag)
    }

    /// 递归绑定解构模式：`v` 直接绑；`Some(inner)`/`Ok(inner)`/`Err(inner)` 继续解构。
    /// `val` 是当前层的载荷（Result/Option 的值槽，i64 或指针）。
    fn bind_pat_deep(&mut self, b: &mut FunctionBuilder, ty: &Ty, val: Value, pat: &Expr) {
        match &pat.kind {
            ExprKind::Ident(bn) => {
                let var = self.new_var(b, ty);
                b.def_var(var, val);
                self.scopes.last_mut().unwrap().push((bn.clone(), VarBind { var, ty: ty.clone() }));
            }
            ExprKind::Some(a) | ExprKind::Ok(a) | ExprKind::Err(a) => {
                let inner_ty = match ty {
                    Ty::Result(t, e) => if matches!(pat.kind, ExprKind::Ok(_)) { (**t).clone() } else { (**e).clone() },
                    Ty::Option(t) => (**t).clone(),
                    _ => Ty::I64,
                };
                let lv = b.ins().load(cl_ty(&inner_ty), MemFlags::new(), val, 8);
                self.bind_pat_deep(b, &inner_ty, lv, a);
            }
            ExprKind::Call(n, args) if matches!(n.as_str(), "Some" | "Ok" | "Err") && args.len() == 1 => {
                let inner_ty = match ty {
                    Ty::Result(t, e) => if n == "Ok" { (**t).clone() } else { (**e).clone() },
                    Ty::Option(t) => (**t).clone(),
                    _ => Ty::I64,
                };
                let lv = b.ins().load(cl_ty(&inner_ty), MemFlags::new(), val, 8);
                self.bind_pat_deep(b, &inner_ty, lv, &args[0]);
            }
            _ => {}
        }
    }

    pub(crate) fn gen_match(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, subject: &Expr, arms: &[MatchArm], want: &Ty, line: usize) -> Result<(Value, Ty), String> {
        let subj = self.gen_expr(jit, b, subject)?;
        let slot = b.func.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        let exit = self.new_block(b);
        for (i, arm) in arms.iter().enumerate() {
            let arm_blk = self.new_block(b); let next_blk = self.new_block(b);
            let cond = if let Some((lo, hi)) = &arm.range {
                let lov = self.gen_expr(jit, b, lo)?;
                let hiv = self.gen_expr(jit, b, hi)?;
                let ge = b.ins().icmp(IntCC::SignedGreaterThanOrEqual, subj.0, lov.0);
                let lt = b.ins().icmp(IntCC::SignedLessThan, subj.0, hiv.0);
                b.ins().band(ge, lt)
            } else {
                match &arm.pat {
                    None => match &arm.guard { None => b.ins().iconst(types::I8, 1), Some(g) => self.gen_cond(jit, b, g)? },
                    Some(p) => {
                        // Result/Option 变体绑定：Ok(v) / Err(e) / Some(v)
                        let ctor: Option<(&str, Option<&Expr>)> = match &p.kind {
                            ExprKind::Ok(a) => Some(("Ok", Some(a))),
                            ExprKind::Err(a) => Some(("Err", Some(a))),
                            ExprKind::Some(a) => Some(("Some", Some(a))),
                            ExprKind::None => Some(("None", None)),
                            // `Ok(v)` 在 parser 中是 Call("Ok", [Ident])
                            ExprKind::Call(n, args) if matches!(n.as_str(), "Ok" | "Err" | "Some") && args.len() == 1 => Some((n.as_str(), Some(&args[0]))),
                            _ => None,
                        };
                        if let Some((cname, carg)) = ctor {
                            let want_tag: i64 = if cname == "Err" || cname == "None" { 1 } else { 0 };
                            let tag = b.ins().load(types::I64, MemFlags::new(), subj.0, 0);
                            let eq = b.ins().icmp_imm(IntCC::Equal, tag, want_tag);
                            if let Some(carg) = carg {
                                // 递归绑定：支持嵌套解构（Some(Some(v)) / Ok(Some(v)) 等）。
                                let bty = match &subj.1 {
                                    Ty::Result(t, e) => match cname { "Ok" => (**t).clone(), "Err" => (**e).clone(), _ => Ty::Unknown },
                                    Ty::Option(t) => (**t).clone(),
                                    _ => Ty::I64,
                                };
                                // 载荷值（i64 槽）
                                let lv = b.ins().load(cl_ty(&bty), MemFlags::new(), subj.0, 8);
                                self.bind_pat_deep(b, &bty, lv, carg);
                            }
                            match &arm.guard { None => eq, Some(g) => { let gv = self.gen_cond(jit, b, g)?; b.ins().band(eq, gv) } }
                        } else if let ExprKind::EnumLit(en, var, _binds) = &p.kind {
                            // 枚举解构：tag 比较并绑定载荷
                            let vidx = jit.enum_variants.get(en).and_then(|vs| vs.iter().position(|(n, _)| n == var)).unwrap_or(0);
                            let tag = b.ins().load(types::I64, MemFlags::new(), subj.0, 0);
                            let eq = b.ins().icmp_imm(IntCC::Equal, tag, vidx as i64);
                            // 载荷绑定与 guard 求值都推迟到 arm 块内（外层 tag 已匹配），
                            // 否则会对无载荷/其它变体的槽越界读（曾崩溃）。
                            eq
                        } else if let ExprKind::Ident(bn) = &p.kind {
                            // 裸标识符模式：绑定主体值（如 `match v { n if n > 0 => ... }`）。
                            // 若该名字已在作用域中，退回"比较"语义（外层变量当模式）。
                            if self.lookup(bn).is_some() {
                                let pv = self.gen_expr(jit, b, p)?;
                                let eq = self.gen_eq(jit, b, &subj, &pv)?;
                                match &arm.guard { None => eq, Some(g) => { let gv = self.gen_cond(jit, b, g)?; b.ins().band(eq, gv) } }
                            } else {
                                let var = self.new_var(b, &subj.1);
                                b.def_var(var, subj.0);
                                self.scopes.last_mut().unwrap().push((bn.clone(), VarBind { var, ty: subj.1.clone() }));
                                match &arm.guard { None => b.ins().iconst(types::I8, 1), Some(g) => self.gen_cond(jit, b, g)? }
                            }
                        } else {
                            // 非标量（元组/结构体/容器）模式：与 LLVM 侧保持一致，明确拒绝。
                            if matches!(p.ty, Ty::Tuple(_) | Ty::Struct(_) | Ty::Enum(_) | Ty::List(_) | Ty::Set(_) | Ty::Map(_, _) | Ty::Array(_, _)) {
                                return Err(crate::lb!(p.line, "match pattern of composite type is not supported", "不支持复合类型的 match 模式"));
                            }
                            let pv = self.gen_expr(jit, b, p)?;
                            let eq = self.gen_eq(jit, b, &subj, &pv)?;
                            match &arm.guard { None => eq, Some(g) => { let gv = self.gen_cond(jit, b, g)?; b.ins().band(eq, gv) } }
                        }
                    }
                }
            };
            b.ins().brif(cond, arm_blk, &[], next_blk, &[]);
            b.switch_to_block(arm_blk); self.terminated = false;
            // enum 载荷解构：进入 arm 块（外层 tag 已匹配，载荷一定存在）后再判定内层 tag 并绑定。
            let mut load_ok: Option<Value> = None;
            let mut guard_ok: Option<Value> = None;
            if let Some(p) = &arm.pat {
                if let ExprKind::EnumLit(en, var, binds) = &p.kind {
                    let ptys: Vec<Ty> = jit.enum_variants.get(en).and_then(|vs| vs.iter().find(|(n, _)| n == var).map(|(_, ts)| ts.clone())).unwrap_or_default();
                    for (i, bd) in binds.iter().enumerate() {
                        let bty = ptys.get(i).cloned().unwrap_or(Ty::I64);
                        let lv = b.ins().load(cl_ty(&bty), MemFlags::new(), subj.0, ((i + 1) * 8) as i32);
                        if let ExprKind::Ident(bn) = &bd.kind {
                            let var = self.new_var(b, &bty);
                            b.def_var(var, lv);
                            self.scopes.last_mut().unwrap().push((bn.clone(), VarBind { var, ty: bty }));
                        } else {
                            let teq = self.pat_tag_cond(b, &bty, lv, bd);
                            load_ok = Some(match load_ok { None => teq, Some(prev) => b.ins().band(prev, teq) });
                            self.bind_pat_deep(b, &bty, lv, bd);
                        }
                    }
                }
            }
            // enum arm 的 guard（可能引用载荷绑定）在这里求值：此时绑定已生效。
            if matches!(&arm.pat.as_ref().map(|p| &p.kind), Some(ExprKind::EnumLit(..))) {
                if let Some(g) = &arm.guard {
                    let gv = self.gen_cond(jit, b, g)?;
                    guard_ok = Some(gv);
                }
            }
            // 内层 tag 判定与 guard 都在 arm 块内做：任一不满足 → 走 next_blk。
            let mut gate: Option<Value> = load_ok;
            if let Some(gv) = guard_ok {
                gate = Some(match gate { None => gv, Some(p) => b.ins().band(p, gv) });
            }
            let _body_blk = match gate {
                None => arm_blk,
                Some(ok) => {
                    let bodyb = self.new_block(b);
                    b.ins().brif(ok, bodyb, &[], next_blk, &[]);
                    b.switch_to_block(bodyb); self.terminated = false;
                    bodyb
                }
            };
            let v = self.gen_block_value(jit, b, &arm.body, want)?;
            if !self.terminated {
                if let Some(v) = v { let a = b.ins().stack_addr(types::I64, slot, 0); b.ins().store(MemFlags::new(), v, a, 0); }
                b.ins().jump(exit, &[]);
            }
            b.switch_to_block(next_blk); self.terminated = false;
            if i + 1 == arms.len() { b.ins().jump(exit, &[]); }
        }
        b.switch_to_block(exit); self.terminated = false;
        let a = b.ins().stack_addr(types::I64, slot, 0);
        let r = b.ins().load(cl_ty(want), MemFlags::new(), a, 0);
        let _ = line;
        Ok((r, want.clone()))
    }

    pub(crate) fn gen_eq(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, a: &(Value, Ty), c: &(Value, Ty)) -> Result<Value, String> {
        if a.1 == Ty::Str || c.1 == Ty::Str {
            let f = self.rt_ref(jit, b, "str_eq")?;
            let call = b.ins().call(f, &[a.0, c.0]);
            let r = b.inst_results(call)[0];
            return Ok(b.ins().icmp_imm(IntCC::NotEqual, r, 0));
        }
        let v = if a.1 == Ty::F64 || c.1 == Ty::F64 { b.ins().fcmp(FloatCC::Equal, a.0, c.0) } else { b.ins().icmp(IntCC::Equal, a.0, c.0) };
        Ok(v)
    }

    pub(crate) fn gen_cond(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, cond: &Expr) -> Result<Value, String> {
        let v = self.gen_expr(jit, b, cond)?;
        let iv = self.convert(b, &v, &Ty::I64);
        Ok(b.ins().icmp_imm(IntCC::NotEqual, iv, 0))
    }

}
