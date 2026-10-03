//! Codegen 的语句生成。

use super::*;

impl<'a> Codegen<'a> {
    // ============================================================
    // 语句
    // ============================================================

    pub(crate) fn block(&mut self, b: &Block) -> Result<(), String> {
        for s in b {
            self.stmt(s)?;
        }
        Ok(())
    }

    /// 生成块；块尾表达式作为块的值返回
    pub(crate) fn block_ret(&mut self, b: &Block, want: &Ty) -> Result<Option<Val>, String> {
        if b.is_empty() {
            return Ok(None);
        }
        let n = b.len();
        for s in &b[..n - 1] {
            self.stmt(s)?;
        }
        self.stmt_value(&b[n - 1], want)
    }

    pub(crate) fn stmt_value(&mut self, s: &Stmt, want: &Ty) -> Result<Option<Val>, String> {
        if self.terminated {
            return Ok(None);
        }
        match s {
            // 块尾表达式即块的值：即使是纯字面量也必须求值（`fn f() { 42 }`）
            Stmt::Expr(e) if want != &Ty::Void => Ok(Some(self.expr(e)?)),
            Stmt::If { cond, then, els, line } if want != &Ty::Void => Ok(Some(self.if_value(
                cond,
                then,
                els.as_ref(),
                want,
                *line,
            )?)),
            Stmt::Block(inner) => self.block_ret(inner, want),
            other => {
                self.stmt(other)?;
                Ok(None)
            }
        }
    }

    pub(crate) fn stmt(&mut self, s: &Stmt) -> Result<(), String> {
        if self.terminated {
            return Ok(());
        }
        match s {
            Stmt::Let { name, value, .. } => {
                let v = self.expr(value)?;
                let ty = if v.ty == Ty::Unknown { Ty::I64 } else { v.ty.clone() };
                let slot = self.new_alloca(&ty);
                let v = self.coerce(&v, &ty)?;
                let loc = Local { ptr: slot, ty };
                if self.immutable_lets.contains(name) {
                    self.perm_ptrs.insert(loc.ptr.clone());
                }
                self.store(&loc, &v)?;
                self.scopes.last_mut().unwrap().insert(name.clone(), loc);
            }
            Stmt::Const { name, value, .. } => {
                let v = self.expr(value)?;
                let ty = if v.ty == Ty::Unknown { Ty::I64 } else { v.ty.clone() };
                let slot = self.new_alloca(&ty);
                let v = self.coerce(&v, &ty)?;
                let loc = Local { ptr: slot, ty };
                self.perm_ptrs.insert(loc.ptr.clone());
                self.store(&loc, &v)?;
                self.scopes.last_mut().unwrap().insert(name.clone(), loc);
            }
            Stmt::Go { func, args, line } => {
                // go f(args)：取函数地址 + 参数指针，调用运行时 thread_spawn
                let info = self.fns.get(func).cloned().ok_or_else(|| crate::lb!(line, "undefined function '{}'", "未定义的函数 '{}'", func))?;
                let fref = format!("@{}", info.cname);
                let _ = fref;
                // 参数打包到堆（简化：逐个存 i64）
                self.declare("declare ptr @gt_mem_alloc(i64)");
                let np = args.len().max(1);
                let pack = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_mem_alloc(i64 {})\n", pack, np * 8));
                for (i, a) in args.iter().enumerate() {
                    let v = self.expr(a)?;
                    let s = self.to_slot(&v);
                    let p = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 {}\n", p, pack, i));
                    self.body.push_str(&format!("  store i64 {}, ptr {}\n", s, p));
                }
                let fp = self.new_reg();
                self.body.push_str(&format!("  {} = ptrtoint ptr @{} to i64\n", fp, info.cname));
                self.declare("declare void @gt_thread_spawn(i64, ptr, i64)");
                self.body.push_str(&format!("  call void @gt_thread_spawn(i64 {}, ptr {}, i64 {})\n", fp, pack, args.len()));
            }
            Stmt::Asm { lines, .. } => {
                // 内联汇编：LLVM `call void asm sideeffect "指令", ""()`
                for ins in lines {
                    let escaped = ins.replace('\\', "\\\\").replace('"', "\\22");
                    self.body.push_str(&format!("  call void asm sideeffect \"{}\", \"\"()\n", escaped));
                }
            }
            Stmt::Throw(e, _line) => {
                // `throw e`：构造 Err(e)。在 try 内跳到捕获块；否则从函数返回（向上传播）。
                let v = self.expr(e)?;
                let slotv = self.to_slot(&v);
                self.declare("declare ptr @gt_result_new(i64, i64)");
                let errv = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_result_new(i64 1, i64 {})\n", errv, slotv));
                if let Some((lbl, slot)) = self.err_stack.last().cloned() {
                    self.declare("declare i64 @gt_result_val(ptr)");
                    let pv = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_result_val(ptr {})\n", pv, errv));
                    self.body.push_str(&format!("  store i64 {}, ptr {}\n", pv, slot));
                    self.body.push_str(&format!("  br label %{}\n", lbl));
                } else {
                    self.body.push_str(&format!("  ret ptr {}\n", errv));
                }
                self.terminated = true;
            }
            Stmt::Try { body, catches, fin, .. } => {
                // `try { body } expt e { h } fily { f }`：
                // body 内 throw/? 命中的 Err 跳到 expt；fily 总执行。
                let slot = self.new_alloca(&Ty::I64);   // try 的值槽
                let err_slot = self.new_alloca(&Ty::I64); // 错误值槽（throw/? 写入）
                let l_handler = self.new_label();       // 统一的 expt 入口
                let l_end = self.new_label();
                self.err_stack.push((l_handler.clone(), err_slot.clone()));
                // body 作为普通语句块执行；?/throw 命中 Err 时经 err_stack 跳到 l_handler。
                self.block(body)?;
                self.err_stack.pop();
                if !self.terminated {
                    // 正常结束：把 0 存入值槽（try 语句的值无意义）
                    self.body.push_str(&format!("  store i64 0, ptr {}\n", slot));
                    self.body.push_str(&format!("  br label %{}\n", l_end));
                }
                // ---- expt 处理 ----
                self.emit_label(&l_handler);
                self.terminated = false;
                let errv = self.new_reg();
                self.body.push_str(&format!("  {} = load i64, ptr {}\n", errv, err_slot));
                if let Some(ca) = catches.first() {
                    self.push_scope();
                    if let Some(binding) = &ca.binding {
                        let bslot = self.new_alloca(&Ty::I64);
                        self.body.push_str(&format!("  store i64 {}, ptr {}\n", errv, bslot));
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
                // fily
                if let Some(f) = fin {
                    self.push_scope();
                    self.block(f)?;
                    self.pop_scope();
                }
                let _ = slot;
            }
            Stmt::Assign { name, index, op, value, line } => {
                let loc = match self.lookup(name) {
                    Some(l) => l,
                    None if index.is_none() && op.is_none() => {
                        // 裸赋值 x = v：自动声明（类型由 RHS 推导）
                        let rhs = self.expr(value)?;
                        let ty = if rhs.ty == Ty::Unknown { Ty::I64 } else { rhs.ty.clone() };
                        let slot = self.new_alloca(&ty);
                        let v = self.coerce(&rhs, &ty)?;
                        self.store(&Local { ptr: slot.clone(), ty: ty.clone() }, &v)?;
                        let loc = Local { ptr: slot, ty };
                        self.scopes.last_mut().unwrap().insert(name.clone(), loc.clone());
                        return Ok(());
                    }
                    None => return Err(crate::lb!(line, "undefined variable '{}'", "未定义的变量 '{}'", name)),
                };
                match index {
                    // 数组元素赋值：`a[i] = v` / `a[i] op= v`
                    Some(idx) => {
                        // List / Map：走运行时 set / insert（引用语义句柄）
                        if matches!(loc.ty, Ty::List(_) | Ty::Map(..)) {
                            let base = self.load(&loc)?;
                            let iv = self.expr(idx)?;
                            let rhs = self.expr(value)?;
                            match &loc.ty {
                                Ty::List(el) => {
                                    let i = self.as_i64(&iv);
                                    let rv = self.coerce(&rhs, el)?;
                                    let rv = self.to_slot(&rv);
                                    self.declare("declare void @gt_list_set(ptr, i64, i64)");
                                    self.body.push_str(&format!("  call void @gt_list_set(ptr {}, i64 {}, i64 {})\n", base.s, i, rv));
                                }
                                Ty::Map(_, v) => {
                                    let k = self.to_slot(&iv);
                                    let rv = self.coerce(&rhs, v)?;
                                    let rv = self.to_slot(&rv);
                                    self.declare("declare void @gt_map_insert(ptr, i64, i64)");
                                    self.body.push_str(&format!("  call void @gt_map_insert(ptr {}, i64 {}, i64 {})\n", base.s, k, rv));
                                }
                                _ => unreachable!(),
                            }
                            return Ok(());
                        }
                        let (elem, p) = self.elem_ptr(&loc, idx, *line)?;
                        let rhs = self.expr(value)?;
                        let rs = match op {
                            Some(bop) => {
                                let cur = self.new_reg();
                                self.body.push_str(&format!(
                                    "  {} = load {}, ptr {}\n",
                                    cur,
                                    elem.llvm(),
                                    p
                                ));
                                let cur = Val::new(&elem, cur);
                                self.binary(*bop, &cur, &rhs, *line, false)?
                            }
                            None => rhs,
                        };
                        let rs = self.coerce(&rs, &elem)?;
                        self.body.push_str(&format!(
                            "  store {} {}, ptr {}\n",
                            elem.llvm(),
                            rs.s,
                            p
                        ));
                    }
                    // 整体赋值 `x = v`
                    None => {
                        let rhs = self.expr(value)?;
                        let rs = match op {
                            Some(bop) => {
                                let cur = self.load(&loc)?;
                                let safe = self.range_analysis.as_ref().map(|ra| ra.is_stmt_safe(s)).unwrap_or(false);
                                self.binary(*bop, &cur, &rhs, *line, safe)?
                            }
                            None => rhs,
                        };
                        // 类型改变：重新分配新类型的槽（自动推导 / let mut 可改类型）
                        if op.is_none() && rs.ty != Ty::Unknown && rs.ty != loc.ty {
                            let nty = rs.ty.clone();
                            let nslot = self.new_alloca(&nty);
                            let nloc = Local { ptr: nslot, ty: nty };
                            let rs = self.coerce(&rs, &nloc.ty)?;
                            self.store(&nloc, &rs)?;
                            self.scopes.last_mut().unwrap().insert(name.clone(), nloc);
                        } else {
                            let rs = self.coerce(&rs, &loc.ty)?;
                            self.store(&loc, &rs)?;
                        }
                    }
                }
            }
            Stmt::Expr(e) => {
                if !is_pure(e) {
                    self.expr(e)?;
                }
            }
            Stmt::If { cond, then, els, .. } => {
                self.if_value(cond, then, els.as_ref(), &Ty::Void, 0)?;
            }
            Stmt::While { cond, body, .. } => {
                let lcond = self.new_label();
                let lbody = self.new_label();
                let lend = self.new_label();

                self.body.push_str(&format!("  br label %{}\n", lcond));
                self.emit_label(&lcond);
                let c = self.cond(cond)?;
                self.body.push_str(&format!(
                    "  br i1 {}, label %{}, label %{}\n",
                    c, lbody, lend
                ));
                self.emit_label(&lbody);

                let (ob, oc) = (self.break_label.clone(), self.continue_label.clone());
                self.break_label = Some(lend.clone());
                self.continue_label = Some(lcond.clone());
                if let Some(lbl) = self.labeled.last().cloned() { self.label_targets.insert(lbl, (lend.clone(), lcond.clone())); }
                self.push_scope();
                self.block(body)?;
                self.pop_scope();
                self.break_label = ob;
                self.continue_label = oc;

                if !self.terminated {
                    self.body.push_str(&format!("  br label %{}\n", lcond));
                }
                self.emit_label(&lend);
            }
            Stmt::Labeled { label, inner, .. } => {
                // 标签循环：登记标签；内层循环创建时把该标签映射到自己的 break/continue 目标
                self.labeled.push(label.clone());
                let inner_blk: Block = vec![(**inner).clone()];
                self.block(&inner_blk)?;
                self.labeled.pop();
            }
            Stmt::DoWhile { body, cond, .. } => {
                // 先执行 body，末尾判 cond：真→回到 body，假→退出
                let lbody = self.new_label();
                let lcond = self.new_label();
                let lend = self.new_label();
                self.body.push_str(&format!("  br label %{}\n", lbody));
                self.emit_label(&lbody);
                let (ob, oc) = (self.break_label.clone(), self.continue_label.clone());
                self.break_label = Some(lend.clone());
                self.continue_label = Some(lcond.clone());
                self.push_scope();
                self.block(body)?;
                self.pop_scope();
                self.break_label = ob;
                self.continue_label = oc;
                if !self.terminated {
                    self.body.push_str(&format!("  br label %{}\n", lcond));
                }
                self.emit_label(&lcond);
                let c = self.cond(cond)?;
                self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", c, lbody, lend));
                self.emit_label(&lend);
            }
            Stmt::ForRange { var, from, to, body, els, .. } => {
                let fv = self.expr(from)?;
                let fv = self.coerce(&fv, &Ty::I64)?;
                let tv = self.expr(to)?;
                let tv = self.coerce(&tv, &Ty::I64)?;
                let iv = self.new_alloca(&Ty::I64);
                self.body
                    .push_str(&format!("  store i64 {}, ptr {}\n", fv.s, iv));

                let lcond = self.new_label();
                let lbody = self.new_label();
                let linc = self.new_label();
                let lend = self.new_label();

                self.body.push_str(&format!("  br label %{}\n", lcond));
                self.emit_label(&lcond);
                let cur = self.new_reg();
                self.body
                    .push_str(&format!("  {} = load i64, ptr {}\n", cur, iv));
                let cmp = self.new_reg();
                self.body
                    .push_str(&format!("  {} = icmp slt i64 {}, {}\n", cmp, cur, tv.s));
                self.body.push_str(&format!(
                    "  br i1 {}, label %{}, label %{}\n",
                    cmp, lbody, lend
                ));

                self.emit_label(&lbody);
                let (ob, oc) = (self.break_label.clone(), self.continue_label.clone());
                self.break_label = Some(lend.clone());
                self.continue_label = Some(linc.clone());
                if let Some(lbl) = self.labeled.last().cloned() { self.label_targets.insert(lbl, (lend.clone(), linc.clone())); }
                self.push_scope();
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(var.clone(), Local { ptr: iv.clone(), ty: Ty::I64 });
                // 记录 `for v in 0..N` 的上界，用于省略 a[v] 的边界检查
                let bounded = matches!(&from.kind, ExprKind::Int(0))
                    .then(|| if let ExprKind::Int(n) = &to.kind { Some(*n) } else { None })
                    .flatten();
                if let Some(n) = bounded { self.bounded.insert(var.clone(), n); }
                self.block(body)?;
                if bounded.is_some() { self.bounded.remove(var); }
                self.pop_scope();
                self.break_label = ob;
                self.continue_label = oc;

                if !self.terminated {
                    self.body.push_str(&format!("  br label %{}\n", linc));
                }
                self.emit_label(&linc);
                let c2 = self.new_reg();
                self.body
                    .push_str(&format!("  {} = load i64, ptr {}\n", c2, iv));
                let nx = self.new_reg();
                self.body
                    .push_str(&format!("  {} = add i64 {}, 1\n", nx, c2));
                self.body
                    .push_str(&format!("  store i64 {}, ptr {}\n", nx, iv));
                self.body.push_str(&format!("  br label %{}\n", lcond));
                self.emit_label(&lend);
                // for-else：正常结束（i 达到终值）才执行 else
                if let Some(els) = els {
                    let lsel = self.new_label();
                    let lskip = self.new_label();
                    let ci = self.new_reg();
                    self.body.push_str(&format!("  {} = load i64, ptr {}\n", ci, iv));
                    let done = self.new_reg();
                    self.body.push_str(&format!("  {} = icmp eq i64 {}, {}\n", done, ci, tv.s));
                    self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", done, lsel, lskip));
                    self.emit_label(&lsel);
                    self.terminated = false;
                    self.push_scope();
                    self.block(els)?;
                    self.pop_scope();
                    if !self.terminated { self.body.push_str(&format!("  br label %{}\n", lskip)); }
                    self.emit_label(&lskip);
                    self.terminated = false;
                }
            }
            Stmt::ForEach { var, iter, body, els: _, line } => {
                let arr = self.expr(iter)?;
                let is_list = matches!(&arr.ty, Ty::List(_));
                let is_str = matches!(&arr.ty, Ty::Str);
                let is_set = matches!(&arr.ty, Ty::Set(_));
                let is_map = matches!(&arr.ty, Ty::Map(..));
                let elem = match &arr.ty {
                    Ty::Array(e, _) => (**e).clone(),
                    Ty::List(e) => (**e).clone(),
                    Ty::Str => Ty::Str,
                    Ty::Set(e) => (**e).clone(),
                    Ty::Map(k, _) => (**k).clone(),
                    other => return Err(crate::lb!(line, "for can only iterate over arrays/lists/strings/sets/maps, found {}", "for 只能遍历数组/列表/字符串/集合/映射，实际是 {}", other)),
                };
                let n: i64 = match &arr.ty { Ty::Array(_, n) => *n as i64, _ => 0 };
                let idx = self.new_alloca(&Ty::I64);
                let ev = self.new_alloca(&elem);
                self.body.push_str(&format!("  store i64 0, ptr {}\n", idx));
                let lcond = self.new_label();
                let lbody = self.new_label();
                let linc = self.new_label();
                let lend = self.new_label();
                // 长度在循环外计算一次（避免每次迭代都调用运行时）
                let len_reg = if is_list {
                    self.declare("declare i64 @gt_list_len(ptr)");
                    let lr = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_list_len(ptr {})\n", lr, arr.s));
                    Some(lr)
                } else if is_str {
                    self.declare("declare i64 @gt_str_char_len(ptr)");
                    let lr = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_str_char_len(ptr {})\n", lr, arr.s));
                    Some(lr)
                } else if is_set {
                    self.declare("declare i64 @gt_set_len(ptr)");
                    let lr = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_set_len(ptr {})\n", lr, arr.s));
                    Some(lr)
                } else if is_map {
                    self.declare("declare i64 @gt_map_len(ptr)");
                    let lr = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_map_len(ptr {})\n", lr, arr.s));
                    Some(lr)
                } else { None };
                self.body.push_str(&format!("  br label %{}\n", lcond));
                self.emit_label(&lcond);
                let i1 = self.new_reg();
                self.body.push_str(&format!("  {} = load i64, ptr {}\n", i1, idx));
                let c = self.new_reg();
                match &len_reg {
                    Some(lr) => { self.body.push_str(&format!("  {} = icmp slt i64 {}, {}\n", c, i1, lr)); }
                    None => { self.body.push_str(&format!("  {} = icmp slt i64 {}, {}\n", c, i1, n)); }
                }
                self.body.push_str(&format!("  br i1 {}, label %{}, label %{}\n", c, lbody, lend));
                self.emit_label(&lbody);
                let (ob, oc) = (self.break_label.clone(), self.continue_label.clone());
                self.break_label = Some(lend.clone());
                self.continue_label = Some(linc.clone());
                // 取元素值写入 ev
                if is_list {
                    // 内联 `data[i]`（GtList.data 在偏移 8），省函数调用与边界检查
                    let daddr = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr i8, ptr {}, i64 8\n", daddr, arr.s));
                    let data = self.new_reg();
                    self.body.push_str(&format!("  {} = load ptr, ptr {}\n", data, daddr));
                    let off = self.new_reg();
                    self.body.push_str(&format!("  {} = mul i64 {}, 8\n", off, i1));
                    let addr = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr i8, ptr {}, i64 {}\n", addr, data, off));
                    let raw = self.new_reg();
                    self.body.push_str(&format!("  {} = load i64, ptr {}\n", raw, addr));
                    let sv = self.from_slot(&raw, &elem);
                    self.body.push_str(&format!("  store {} {}, ptr {}\n", elem.llvm(), sv, ev));
                } else if is_str {
                    self.declare("declare ptr @gt_str_char_at(ptr, i64)");
                    let sp = self.new_reg();
                    self.body.push_str(&format!("  {} = call ptr @gt_str_char_at(ptr {}, i64 {})\n", sp, arr.s, i1));
                    self.body.push_str(&format!("  store ptr {}, ptr {}\n", sp, ev));
                } else if is_set {
                    self.declare("declare i64 @gt_set_at(ptr, i64)");
                    let raw = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_set_at(ptr {}, i64 {})\n", raw, arr.s, i1));
                    let sv = self.from_slot(&raw, &elem);
                    self.body.push_str(&format!("  store {} {}, ptr {}\n", elem.llvm(), sv, ev));
                } else if is_map {
                    self.declare("declare i64 @gt_map_key_at(ptr, i64)");
                    let raw = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 @gt_map_key_at(ptr {}, i64 {})\n", raw, arr.s, i1));
                    let sv = self.from_slot(&raw, &elem);
                    self.body.push_str(&format!("  store {} {}, ptr {}\n", elem.llvm(), sv, ev));
                } else {
                    let off = self.new_reg();
                    self.body.push_str(&format!("  {} = mul i64 {}, {}\n", off, i1, elem.llvm().replace("ptr","8").replace("i64","8").replace("double","8").replace("i1","8")));
                    let p = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr i8, ptr {}, i64 {}\n", p, arr.s, off));
                    let ld = self.new_reg();
                    self.body.push_str(&format!("  {} = load {}, ptr {}\n", ld, elem.llvm(), p));
                    self.body.push_str(&format!("  store {} {}, ptr {}\n", elem.llvm(), ld, ev));
                }
                self.push_scope();
                self.scopes.last_mut().unwrap().insert(var.clone(), Local { ptr: ev.clone(), ty: elem.clone() });
                self.block(body)?;
                self.pop_scope();
                self.break_label = ob;
                self.continue_label = oc;
                self.body.push_str(&format!("  br label %{}\n", linc));
                self.emit_label(&linc);
                let i2 = self.new_reg();
                self.body.push_str(&format!("  {} = load i64, ptr {}\n", i2, idx));
                let nx = self.new_reg();
                self.body.push_str(&format!("  {} = add i64 {}, 1\n", nx, i2));
                self.body.push_str(&format!("  store i64 {}, ptr {}\n", nx, idx));
                self.body.push_str(&format!("  br label %{}\n", lcond));
                self.emit_label(&lend);
            }
            Stmt::Return(e, _) => {
                if self.is_main_fn {
                    // main 的 IR 签名是 i32：无论 return 有无值都返回 0
                    self.body.push_str("  ret i32 0\n");
                    self.terminated = true;
                } else if self.cur_ret == Ty::Void {
                    self.body.push_str("  ret void\n");
                } else {
                    // impl Trait 返回位置：具体类型自动装箱为 dyn Trait
                    if let (Ty::Dyn(tr), Some(inner), Ty::Struct(_) | Ty::Enum(_)) = (self.cur_ret.clone(), e.as_ref(), &e.as_ref().map(|x| x.ty.clone()).unwrap_or(Ty::Unknown)) {
                        let boxed = self.dyn_box(&tr, inner, inner)?;
                        self.body.push_str(&format!("  ret {} {}\n", self.cur_ret.llvm(), boxed.s));
                        self.terminated = true;
                        return Ok(());
                    }
                    let v = match e {
                        Some(e) => self.expr(e)?,
                        None => Val::new(&self.cur_ret.clone(), self.cur_ret.zero()),
                    };
                    let rt = self.cur_ret.clone();
                    let v = self.coerce(&v, &rt)?;
                    self.body
                        .push_str(&format!("  ret {} {}\n", rt.llvm(), v.s));
                }
                self.terminated = true;
            }
            Stmt::Break(lbl, _) => {
                let l = match lbl {
                    Some(n) => self.label_targets.get(n).map(|(b, _)| b.clone()),
                    None => self.break_label.clone(),
                }
                .ok_or_else(|| "break 只能出现在循环内".to_string())?;
                self.body.push_str(&format!("  br label %{}\n", l));
                self.terminated = true;
            }
            Stmt::Continue(lbl, _) => {
                let l = match lbl {
                    Some(n) => self.label_targets.get(n).map(|(_, c)| c.clone()),
                    None => self.continue_label.clone(),
                }
                .ok_or_else(|| "continue 只能出现在循环内".to_string())?;
                self.body.push_str(&format!("  br label %{}\n", l));
                self.terminated = true;
            }
            Stmt::Block(b) => {
                // 不引入新作用域：与 sema/jit 对齐（解构块依赖此）
                self.block(b)?;
            }
            // 嵌套函数已在 hoist 阶段提升为顶层
            Stmt::LocalFn(_) => {}
            Stmt::FieldAssign { obj, field, op, value, line } => {
                let loc = self
                    .lookup(obj)
                    .ok_or_else(|| crate::lb!(line, "undefined variable '{}'", "未定义的变量 '{}'", obj))?;
                let sname = match &loc.ty {
                    Ty::Struct(n) => n.clone(),
                    other => {
                        return Err(crate::lb!(line, "{} is not a struct; cannot access field", "{} 不是结构体，不能访问字段", other))
                    }
                };
                let layout = self
                    .structs
                    .get(&sname)
                    .cloned()
                    .ok_or_else(|| crate::lb!(line, "undefined struct '{}'", "未定义的结构体 '{}'", sname))?;
                let idx = layout
                    .iter()
                    .position(|(n, _)| n == field)
                    .ok_or_else(|| crate::lb!(line, "struct '{}' has no field '{}'", "结构体 '{}' 没有字段 '{}'", sname, field))?;
                let fty = layout[idx].1.clone();
                let arrty = format!("[{} x i64]", layout.len().max(1));
                // 取结构体指针的**值**（alloca 里存的是指向结构体内存的指针）
                let basev = self.load(&loc)?;
                let base = basev.s;
                let p = self.new_reg();
                self.body.push_str(&format!(
                    "  {} = getelementptr inbounds {}, ptr {}, i64 0, i64 {}\n",
                    p, arrty, base, idx
                ));
                let rhs = self.expr(value)?;
                let res = match op {
                    Some(o) => {
                        let raw = self.new_reg();
                        self.body.push_str(&format!("  {} = load i64, ptr {}\n", raw, p));
                        let cur = self.from_slot(&raw, &fty);
                        let curv = Val::new(&fty, cur);
                        self.binary(*o, &curv, &rhs, *line, false)?
                    }
                    None => self.coerce(&rhs, &fty)?,
                };
                let slotv = self.to_slot(&res);
                self.body.push_str(&format!("  store i64 {}, ptr {}\n", slotv, p));
            }
        }
        Ok(())
    }
}
