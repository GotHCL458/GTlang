//! Codegen 的内建函数/方法调用分派。

use super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn call(&mut self, name: &str, args: &[Expr], line: usize, call_ty: &Ty) -> Result<Val, String> {
        // `s.方法(args)`：s 是 dyn Trait 对象 → 从 vtable 取址 + 间接调用
        if let Some(dot) = name.find('.') {
            let recv = &name[..dot];
            if let Some(loc) = self.lookup(recv) {
                if let Ty::Dyn(tr) = loc.ty.clone() {
                    let method = name[dot+1..].to_string();
                    // vtable 索引 = trait 方法序号 + 1
                    let idx = self.traits.get(&tr).and_then(|ms| ms.iter().position(|m| *m == method)).map(|i| i + 1).unwrap_or(0);
                    let rv = self.load(&loc)?;
                    let dp = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 0\n", dp, rv.s));
                    let data = self.new_reg();
                    self.body.push_str(&format!("  {} = load i64, ptr {}\n", data, dp));
                    let vp = self.new_reg();
                    self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 {}\n", vp, rv.s, idx));
                    let addr = self.new_reg();
                    self.body.push_str(&format!("  {} = load i64, ptr {}\n", addr, vp));
                    let fp = self.new_reg();
                    self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", fp, addr));
                    let mut ops: Vec<String> = vec!["i64".into(), data];
                    for a in args {
                        let v = self.expr(a)?;
                        let av = self.as_i64(&v);
                        ops.push("i64".into());
                        ops.push(av);
                    }
                    let argstr: Vec<String> = ops.chunks(2).map(|c| format!("{} {}", c[0], c[1])).collect();
                    // 返回类型按方法签名（f64 → double，ptr 类保持 ptr，其余 i64）
                    let ret_llvm = call_ty.llvm();
                    let ret_llvm: String = if ret_llvm == "double" { "double".into() } else if ret_llvm == "void" { "void".into() } else if ret_llvm == "ptr" { "ptr".into() } else if ret_llvm == "i1" { "i1".into() } else { "i64".into() };
                    let r = self.new_reg();
                    if ret_llvm == "void" {
                        self.body.push_str(&format!("  call void {}({})\n", fp, argstr.join(", ")));
                        return Ok(Val::new(&Ty::Void, "0"));
                    }
                    self.body.push_str(&format!("  {} = call {} {}({})\n", r, ret_llvm, fp, argstr.join(", ")));
                    return Ok(Val::new(&call_ty.clone(), r));
                }
            }
        }



        // 内置函数的"哪些调用合法"由 type.rs 统一判定，这里只负责生成代码
        if let Some(r) = builtin_ret(name, &args.iter().map(|a| a.ty.clone()).collect::<Vec<_>>()) {
            r.map_err(|why| crate::lb!(line, "{}", "{}", why))?;
        }
        match name {
            "Ok" | "Err" => {
                let is_err = name == "Err";
                let v = self.expr(&args[0])?;
                let slotv = self.to_slot(&v);
                self.declare("declare ptr @gt_result_new(i64, i64)");
                let r = self.new_reg();
                self.body.push_str(&format!(
                    "  {} = call ptr @gt_result_new(i64 {}, i64 {})
",
                    r, if is_err { 1 } else { 0 }, slotv
                ));
                return Ok(Val::new(call_ty, r));
            }
            "Some" => {
                eprintln!("[Some] call_ty={:?}", call_ty);
                let v = self.expr(&args[0])?;
                let slotv = self.to_slot(&v);
                self.declare("declare ptr @gt_result_new(i64, i64)");
                let r = self.new_reg();
                self.body.push_str(&format!(
                    "  {} = call ptr @gt_result_new(i64 0, i64 {})
",
                    r, slotv
                ));
                return Ok(Val::new(call_ty, r));
            }
            "None" => {
                self.declare("declare ptr @gt_result_new(i64, i64)");
                let r = self.new_reg();
                self.body.push_str(&format!(
                    "  {} = call ptr @gt_result_new(i64 1, i64 0)
",
                    r
                ));
                return Ok(Val::new(call_ty, r));
            }
            "put" | "print" => {
                if args.is_empty() { return Err(crate::lb!(line, "{}() requires 1 argument", "{}() 需要 1 个参数", name)); }
                self.emit_print(&args[0], name == "put")?;
                Ok(Val::new(&Ty::Void, "0"))
            }
            "sb_new" => {
                self.declare("declare i64 @gt_sb_new()");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = call i64 @gt_sb_new()\n", r));
                Ok(Val::new(&Ty::I64, r))
            }
            "sb_push" | "sb_push_str" | "sb_push_int" | "sb_push_f64" | "sb_push_bool" => {
                if args.len() != 2 { return Err(crate::lb!(line, "sb_push() requires 2 arguments", "sb_push() 需要 2 个参数")); }
                let h = self.expr(&args[0])?;
                let hs = self.as_i64(&h);
                let v = self.expr(&args[1])?;
                // 按值类型选运行时函数
                let fname = match v.ty {
                    Ty::F64 => "gt_sb_push_f64",
                    Ty::Str => "gt_sb_push_str",
                    Ty::Bool => "gt_sb_push_bool",
                    _ => "gt_sb_push_i64",
                };
                let argty = if v.ty == Ty::F64 { "double" } else if v.ty == Ty::Str { "ptr" } else { "i64" };
                let av = if v.ty == Ty::Str { v.s.clone() } else { self.as_i64(&v) };
                self.declare(&format!("declare void @{}(i64, {})", fname, argty));
                self.body.push_str(&format!("  call void @{}(i64 {}, {} {})\n", fname, hs, argty, av));
                Ok(Val::new(&Ty::Void, "0"))
            }
            "sb_finish" => {
                if args.len() != 1 { return Err(crate::lb!(line, "sb_finish() requires 1 argument", "sb_finish() 需要 1 个参数")); }
                let h = self.expr(&args[0])?;
                let hs = self.as_i64(&h);
                self.declare("declare ptr @gt_sb_finish(i64)");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_sb_finish(i64 {})\n", r, hs));
                Ok(Val::new(&Ty::Str, r))
            }
            "len" => match args[0].ty.clone() {
                Ty::Array(_, n) => Ok(Val::new(&Ty::I64, n.to_string())),
                Ty::Str => { self.declare("declare i64 @strlen(ptr)"); let v = self.expr(&args[0])?; let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @strlen(ptr {})\n", r, v.s)); Ok(Val::new(&Ty::I64, r)) }
                Ty::List(_) | Ty::Set(_) | Ty::Map(..) => { let v = self.expr(&args[0])?; let key = match args[0].ty.clone() { Ty::List(_) => "gt_list_len", Ty::Set(_) => "gt_set_len", _ => "gt_map_len" }; self.declare(&format!("declare i64 @{}(ptr)", key)); let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @{}(ptr {})\n", r, key, v.s)); Ok(Val::new(&Ty::I64, r)) }
                other => Err(crate::lb!(line, "len() does not support {}", "len() 不支持 {}", other)),
            },
            "str" | "string" => { let v = self.expr(&args[0])?; self.to_str(&v, line) }
            "int" | "i64" => { let v = self.expr(&args[0])?; self.to_i64(&v, line) }
            "f64" | "float" => { let v = self.expr(&args[0])?; self.to_f64(&v, line) }
            "bool" => { let v = self.expr(&args[0])?; self.to_bool(&v, line) }
            "list" | "List" => {
                // 元素引用计数：取 List 的元素类型判断是否堆指针（Unknown 保守取 0）
                let ep = match call_ty { Ty::List(e) => elem_is_ptr(e), _ => 0 };
                self.declare("declare ptr @gt_list_new(i64)");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_list_new(i64 {})\n", r, ep));
                Ok(Val::new(call_ty, r))
            }
            "chan" => {
                self.declare("declare ptr @gt_chan_new()");
                let p = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_chan_new()\n", p));
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = ptrtoint ptr {} to i64\n", r, p));
                Ok(Val::new(&Ty::I64, r))
            }
            "chan_send" => {
                let c = self.expr(&args[0])?;
                let v = self.expr(&args[1])?;
                let vs = self.to_slot(&v);
                let cp = self.new_reg();
                self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", cp, c.s));
                self.declare("declare void @gt_chan_send(ptr, i64)");
                self.body.push_str(&format!("  call void @gt_chan_send(ptr {}, i64 {})\n", cp, vs));
                Ok(Val::new(&Ty::Void, "0".to_string()))
            }
            "chan_recv" => {
                let c = self.expr(&args[0])?;
                let cp = self.new_reg();
                self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", cp, c.s));
                self.declare("declare i64 @gt_chan_recv(ptr)");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = call i64 @gt_chan_recv(ptr {})\n", r, cp));
                Ok(Val::new(&Ty::I64, r))
            }
            "sleep" => {
                let a = self.expr(&args[0])?;
                let av = self.as_i64(&a);
                self.declare("declare void @gt_sleep(i64)");
                self.body.push_str(&format!("  call void @gt_sleep(i64 {})\n", av));
                Ok(Val::new(&Ty::Void, "0".to_string()))
            }
            "read_line" | "readline" | "input" => {
                self.declare("declare ptr @gt_read_line()");
                let p = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_read_line()\n", p));
                Ok(Val::new(&Ty::Str, p))
            }
            "read_int" | "readint" => {
                self.declare("declare i64 @gt_read_int()");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = call i64 @gt_read_int()\n", r));
                Ok(Val::new(&Ty::I64, r))
            }
            "assert" => {
                let c = self.expr(&args[0])?;
                let cv = self.as_i64(&c);
                let msg = if args.len() > 1 {
                    let m = self.expr(&args[1])?;
                    m.s
                } else { "null".to_string() };
                self.declare("declare void @gt_assert(i64, ptr, i64)");
                let ms = if msg == "null" { "null".to_string() } else { msg };
                self.body.push_str(&format!("  call void @gt_assert(i64 {}, ptr {}, i64 {})\n", cv, ms, args[0].line));
                Ok(Val::new(&Ty::Void, "0".to_string()))
            }
            "range" => {
                let a = self.expr(&args[0])?;
                let av = self.as_i64(&a);
                let bv = if args.len() > 1 { let x = self.expr(&args[1])?; self.as_i64(&x) } else { "0".to_string() };
                let (lo, hi) = if args.len() > 1 { (av, bv) } else { (bv, av) };
                self.declare("declare ptr @gt_range(i64, i64)");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_range(i64 {}, i64 {})\n", r, lo, hi));
                Ok(Val::new(call_ty, r))
            }
            "set" | "Set" => {
                let ep = match call_ty { Ty::Set(e) => elem_is_ptr(e), _ => 0 };
                self.declare("declare ptr @gt_set_new(i64)");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_set_new(i64 {})\n", r, ep));
                Ok(Val::new(call_ty, r))
            }
            "map" | "Map" | "dict" => {
                let ep = match call_ty { Ty::Map(k, v) => elem_is_ptr(k) | elem_is_ptr(v), _ => 0 };
                self.declare("declare ptr @gt_map_new(i64)");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_map_new(i64 {})\n", r, ep));
                Ok(Val::new(call_ty, r))
            }
            "push" | "append" => { self.declare("declare void @gt_list_push(ptr, i64)"); let l = self.expr(&args[0])?; let lp = self.as_ptr(&l); let v = self.expr(&args[1])?; let vs = self.to_slot(&v); self.body.push_str(&format!("  call void @gt_list_push(ptr {}, i64 {})\n", lp, vs)); Ok(Val::new(&Ty::Void, "0")) }
            "pop" => { self.declare("declare i64 @gt_list_pop(ptr)"); let l = self.expr(&args[0])?; let lp = self.as_ptr(&l); let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @gt_list_pop(ptr {})\n", r, lp)); let v = self.from_slot(&r, call_ty); Ok(Val::new(call_ty, v)) }
            "at" => { let c = self.expr(&args[0])?; let i = self.expr(&args[1])?; let i = self.as_i64(&i); match args[0].ty.clone() {
                Ty::List(e) => { let cp = self.as_ptr(&c); self.declare("declare i64 @gt_list_at(ptr, i64)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @gt_list_at(ptr {}, i64 {})\n", r, cp, i)); let v = self.from_slot(&r, &e); Ok(Val::new(&e, v)) }
                Ty::Map(_, v) => { let cp = self.as_ptr(&c); self.declare("declare i64 @gt_map_get(ptr, i64)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @gt_map_get(ptr {}, i64 {})\n", r, cp, i)); let vv = self.from_slot(&r, &v); Ok(Val::new(&v, vv)) }
                _ => Err(crate::lb!(line, "at() does not support this container", "at() 不支持该容器")) } }
            "insert" => { let c = self.expr(&args[0])?; match args[0].ty.clone() {
                Ty::Map(..) => { let cp = self.as_ptr(&c); self.declare("declare void @gt_map_insert(ptr, i64, i64)"); let k = self.expr(&args[1])?; let k = self.to_slot(&k); let v = self.expr(&args[2])?; let v = self.to_slot(&v); self.body.push_str(&format!("  call void @gt_map_insert(ptr {}, i64 {}, i64 {})\n", cp, k, v)); }
                _ => { let cp = self.as_ptr(&c); self.declare("declare void @gt_set_insert(ptr, i64)"); let v = self.expr(&args[1])?; let v = self.to_slot(&v); self.body.push_str(&format!("  call void @gt_set_insert(ptr {}, i64 {})\n", cp, v)); } }
                Ok(Val::new(&Ty::Void, "0")) }
            "has" | "contains" if !matches!(args[0].ty, Ty::Str) => { let c = self.expr(&args[0])?; let k = self.expr(&args[1])?; let k = self.to_slot(&k); let (fdecl, fname) = match args[0].ty.clone() { Ty::Map(..) => ("declare i64 @gt_map_has(ptr, i64)", "gt_map_has"), Ty::Set(..) => ("declare i64 @gt_set_has(ptr, i64)", "gt_set_has"), _ => ("declare i64 @gt_list_has(ptr, i64)", "gt_list_has") }; self.declare(fdecl); let cp = self.as_ptr(&c); let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @{}(ptr {}, i64 {})\n", r, fname, cp, k)); let bb = self.new_reg(); self.body.push_str(&format!("  {} = icmp ne i64 {}, 0\n", bb, r)); Ok(Val::new(&Ty::Bool, bb)) }
            "remove" => { let c = self.expr(&args[0])?; let cp = self.as_ptr(&c); let k = self.expr(&args[1])?; let k = self.to_slot(&k); match args[0].ty.clone() {
                Ty::Map(..) => { self.declare("declare void @gt_map_remove(ptr, i64)"); self.body.push_str(&format!("  call void @gt_map_remove(ptr {}, i64 {})\n", cp, k)); }
                Ty::Set(..) => { self.declare("declare void @gt_set_remove(ptr, i64)"); self.body.push_str(&format!("  call void @gt_set_remove(ptr {}, i64 {})\n", cp, k)); }
                _ => { self.declare("declare void @gt_list_remove(ptr, i64)"); self.body.push_str(&format!("  call void @gt_list_remove(ptr {}, i64 {})\n", cp, k)); } }
                Ok(Val::new(&Ty::Void, "0")) }
            "keys" => { self.declare("declare ptr @gt_map_keys(ptr)"); let c = self.expr(&args[0])?; let cp = self.as_ptr(&c); let r = self.new_reg(); self.body.push_str(&format!("  {} = call ptr @gt_map_keys(ptr {})\n", r, cp)); let kt = match args[0].ty.clone() { Ty::Map(k, _) => (*k).clone(), _ => Ty::I64 }; Ok(Val::new(&Ty::List(Box::new(kt)), r)) }
            "values" => { self.declare("declare ptr @gt_map_values(ptr)"); let c = self.expr(&args[0])?; let cp = self.as_ptr(&c); let r = self.new_reg(); self.body.push_str(&format!("  {} = call ptr @gt_map_values(ptr {})\n", r, cp)); let vt = match args[0].ty.clone() { Ty::Map(_, v) => (*v).clone(), _ => Ty::I64 }; Ok(Val::new(&Ty::List(Box::new(vt)), r)) }
            "abs" => { let v = self.expr(&args[0])?; let is_f = v.ty == Ty::F64; if is_f { self.declare("declare double @gt_abs_f(double)") } else { self.declare("declare i64 @gt_abs_i(i64)") } let r = self.new_reg(); if is_f { self.body.push_str(&format!("  {} = call double @gt_abs_f(double {})\n", r, v.s)); Ok(Val::new(&Ty::F64, r)) } else { self.body.push_str(&format!("  {} = call i64 @gt_abs_i(i64 {})\n", r, v.s)); Ok(Val::new(&Ty::I64, r)) } }
            "min" | "max" => { let a = self.expr(&args[0])?; let b2 = self.expr(&args[1])?; let is_f = a.ty == Ty::F64 || b2.ty == Ty::F64; let (suf, argt, ret) = if is_f { ("f", "double", Ty::F64) } else { ("i", "i64", Ty::I64) }; self.declare(&format!("declare {} @gt_{}_{}({}, {})", ret.llvm(), name, suf, argt, argt)); let r = self.new_reg(); self.body.push_str(&format!("  {} = call {} @gt_{}_{}({} {}, {} {})\n", r, ret.llvm(), name, suf, argt, a.s, argt, b2.s)); Ok(Val::new(&ret, r)) }
            "sum" => { let l = self.expr(&args[0])?; let is_f = matches!(&args[0].ty, Ty::List(e) if **e == Ty::F64); let (fname, ret) = if is_f { ("gt_sum_f", Ty::F64) } else { ("gt_sum_i", Ty::I64) }; self.declare(&format!("declare {} @{}(ptr)", ret.llvm(), fname)); let lp = self.as_ptr(&l); let r = self.new_reg(); self.body.push_str(&format!("  {} = call {} @{}(ptr {})\n", r, ret.llvm(), fname, lp)); Ok(Val::new(&ret, r)) }
            "substr" => { let s = self.expr(&args[0])?; let a = self.expr(&args[1])?; let n = self.expr(&args[2])?; let a = self.as_i64(&a); let n = self.as_i64(&n); self.declare("declare ptr @gt_str_substr(ptr, i64, i64)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call ptr @gt_str_substr(ptr {}, i64 {}, i64 {})\n", r, s.s, a, n)); Ok(Val::new(&Ty::Str, r)) }
            "find" => { let s = self.expr(&args[0])?; let sub = self.expr(&args[1])?; self.declare("declare i64 @gt_str_find(ptr, ptr)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @gt_str_find(ptr {}, ptr {})\n", r, s.s, sub.s)); Ok(Val::new(&Ty::I64, r)) }
            "upper" | "lower" | "trim" => { let s = self.expr(&args[0])?; self.declare(&format!("declare ptr @gt_str_{}(ptr)", name)); let r = self.new_reg(); self.body.push_str(&format!("  {} = call ptr @gt_str_{}(ptr {})\n", r, name, s.s)); Ok(Val::new(&Ty::Str, r)) }
            "repeat" => { let s = self.expr(&args[0])?; let n = self.expr(&args[1])?; let n = self.as_i64(&n); self.declare("declare ptr @gt_str_repeat(ptr, i64)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call ptr @gt_str_repeat(ptr {}, i64 {})\n", r, s.s, n)); Ok(Val::new(&Ty::Str, r)) }
            "replace" => { let s = self.expr(&args[0])?; let a = self.expr(&args[1])?; let c = self.expr(&args[2])?; self.declare("declare ptr @gt_str_replace(ptr, ptr, ptr)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call ptr @gt_str_replace(ptr {}, ptr {}, ptr {})\n", r, s.s, a.s, c.s)); Ok(Val::new(&Ty::Str, r)) }
            "pad_left" | "pad_right" | "lpad" | "rpad" => {
                let s = self.expr(&args[0])?;
                let w = self.expr(&args[1])?;
                let wi = self.as_i64(&w);
                let fv = if args.len() > 2 { let f = self.expr(&args[2])?; f.s } else { "null".to_string() };
                let key = if matches!(name, "pad_right" | "rpad") { "gt_pad_right" } else { "gt_pad_left" };
                self.declare(&format!("declare ptr @{}(ptr, i64, ptr)", key));
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @{}(ptr {}, i64 {}, ptr {})\n", r, key, s.s, wi, fv));
                Ok(Val::new(&Ty::Str, r))
            }
            // fmt_int(x, width) -> 右对齐字符串
            "fmt_int" => {
                let x = self.expr(&args[0])?;
                let xi = self.as_i64(&x);
                let w = self.expr(&args[1])?;
                let wi = self.as_i64(&w);
                self.declare("declare ptr @gt_fmt_int(i64, i64)");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = call ptr @gt_fmt_int(i64 {}, i64 {})\n", r, xi, wi));
                Ok(Val::new(&Ty::Str, r))
            }
            "split" => { let s = self.expr(&args[0])?; let sep = self.expr(&args[1])?; self.declare("declare ptr @gt_str_split(ptr, ptr)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call ptr @gt_str_split(ptr {}, ptr {})\n", r, s.s, sep.s)); Ok(Val::new(call_ty, r)) }
            "join" => { let l = self.expr(&args[0])?; let lp = self.as_ptr(&l); let sep = self.expr(&args[1])?; let sp = self.as_ptr(&sep); self.declare("declare ptr @gt_str_join(ptr, ptr)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call ptr @gt_str_join(ptr {}, ptr {})\n", r, lp, sp)); Ok(Val::new(&Ty::Str, r)) }
            "mem_alloc" => { let n = self.expr(&args[0])?; let n = self.as_i64(&n); self.declare("declare ptr @gt_mem_alloc(i64)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call ptr @gt_mem_alloc(i64 {})\n", r, n)); let ri = self.new_reg(); self.body.push_str(&format!("  {} = ptrtoint ptr {} to i64\n", ri, r)); Ok(Val::new(&Ty::I64, ri)) }
            "mem_free" => { let p = self.expr(&args[0])?; let p = self.as_i64(&p); self.declare("declare void @gt_mem_free(i64)"); self.body.push_str(&format!("  call void @gt_mem_free(i64 {})\n", p)); Ok(Val::new(&Ty::Void, "0")) }
            "mem_store_i64" | "mem_store_u8" | "mem_set" => { let key = match name { "mem_store_i64" => "gt_mem_store_i64", "mem_store_u8" => "gt_mem_store_u8", _ => "gt_mem_set" }; let p = self.expr(&args[0])?; let p = self.as_i64(&p); let o = self.expr(&args[1])?; let o = self.as_i64(&o); let v = self.expr(&args[2])?; let v = self.as_i64(&v); self.declare(&format!("declare void @{}(i64, i64, i64)", key)); self.body.push_str(&format!("  call void @{}(i64 {}, i64 {}, i64 {})\n", key, p, o, v)); Ok(Val::new(&Ty::Void, "0")) }
            "mem_load_i64" | "mem_load_u8" => { let key = if name == "mem_load_i64" { "gt_mem_load_i64" } else { "gt_mem_load_u8" }; let p = self.expr(&args[0])?; let p = self.as_i64(&p); let o = self.expr(&args[1])?; let o = self.as_i64(&o); self.declare(&format!("declare i64 @{}(i64, i64)", key)); let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @{}(i64 {}, i64 {})\n", r, key, p, o)); Ok(Val::new(&Ty::I64, r)) }
            "mem_copy" => { let d = self.expr(&args[0])?; let d = self.as_i64(&d); let s = self.expr(&args[1])?; let s = self.as_i64(&s); let n = self.expr(&args[2])?; let n = self.as_i64(&n); self.declare("declare void @gt_mem_copy(i64, i64, i64)"); self.body.push_str(&format!("  call void @gt_mem_copy(i64 {}, i64 {}, i64 {})\n", d, s, n)); Ok(Val::new(&Ty::Void, "0")) }
            _ => {
                if let Some(info) = self.fns.get(name).cloned() {
                    let mut ops = Vec::new();
                    for (i, a) in args.iter().enumerate() { let want = info.params.get(i).cloned().unwrap_or(Ty::I64); let v = self.expr(a)?; let v = self.coerce(&v, &want)?; ops.push(format!("{} {}", want.llvm(), v.s)); }
                    let argstr = ops.join(", ");
                    if info.ret == Ty::Void { self.body.push_str(&format!("  call void @{}({})\n", info.cname, argstr)); Ok(Val::new(&Ty::Void, "0")) }
                    else { let r = self.new_reg(); self.body.push_str(&format!("  {} = call {} @{}({})\n", r, info.ret.llvm(), info.cname, argstr)); Ok(Val::new(&info.ret, r)) }
                } else if let Some(sf) = crate::types::gtlib_fn(name) {
                    let mut ops = Vec::new();
                    for (i, a) in args.iter().enumerate() { let want = sf.params.get(i).cloned().unwrap_or(Ty::I64); let v = self.expr(a)?; let v = self.coerce(&v, &want)?; ops.push(format!("{} {}", want.llvm(), v.s)); }
                    let ret = if sf.ret == Ty::Void { "void".to_string() } else { sf.ret.llvm() };
                    let argstr = ops.join(", ");
                    self.declare(&format!("declare {} @{}({})", ret, sf.symbol, sf.params.iter().map(|t| t.llvm()).collect::<Vec<_>>().join(", ")));
                    if sf.ret == Ty::Void { self.body.push_str(&format!("  call void @{}({})\n", sf.symbol, argstr)); Ok(Val::new(&Ty::Void, "0")) }
                    else { let r = self.new_reg(); self.body.push_str(&format!("  {} = call {} @{}({})\n", r, ret, sf.symbol, argstr)); Ok(Val::new(&sf.ret, r)) }
                } else {
                    Err(crate::lb!(line, "undefined function '{}'", "未定义的函数 '{}'", name))
                }
            }
        }
    }

    // ---------- 类型转换内置 ----------
}
