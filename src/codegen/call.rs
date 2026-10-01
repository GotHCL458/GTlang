//! Codegen 的调用/转换/辅助方法。

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
                    let r = self.new_reg();
                    self.body.push_str(&format!("  {} = call i64 {}({})\n", r, fp, argstr.join(", ")));
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
            "push" | "append" => { self.declare("declare void @gt_list_push(ptr, i64)"); let l = self.expr(&args[0])?; let lp = if l.ty.llvm() == "ptr" { l.s.clone() } else { let ls = self.as_i64(&l); self.from_slot(&ls, &Ty::List(Box::new(Ty::Unknown))) }; let v = self.expr(&args[1])?; let vs = self.to_slot(&v); self.body.push_str(&format!("  call void @gt_list_push(ptr {}, i64 {})\n", lp, vs)); Ok(Val::new(&Ty::Void, "0")) }
            "pop" => { self.declare("declare i64 @gt_list_pop(ptr)"); let l = self.expr(&args[0])?; let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @gt_list_pop(ptr {})\n", r, l.s)); let v = self.from_slot(&r, call_ty); Ok(Val::new(call_ty, v)) }
            "at" => { let c = self.expr(&args[0])?; let i = self.expr(&args[1])?; let i = self.as_i64(&i); match args[0].ty.clone() {
                Ty::List(e) => { self.declare("declare i64 @gt_list_at(ptr, i64)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @gt_list_at(ptr {}, i64 {})\n", r, c.s, i)); let v = self.from_slot(&r, &e); Ok(Val::new(&e, v)) }
                Ty::Map(_, v) => { self.declare("declare i64 @gt_map_get(ptr, i64)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @gt_map_get(ptr {}, i64 {})\n", r, c.s, i)); let vv = self.from_slot(&r, &v); Ok(Val::new(&v, vv)) }
                _ => Err(crate::lb!(line, "at() does not support this container", "at() 不支持该容器")) } }
            "insert" => { let c = self.expr(&args[0])?; match args[0].ty.clone() {
                Ty::Map(..) => { self.declare("declare void @gt_map_insert(ptr, i64, i64)"); let k = self.expr(&args[1])?; let k = self.to_slot(&k); let v = self.expr(&args[2])?; let v = self.to_slot(&v); self.body.push_str(&format!("  call void @gt_map_insert(ptr {}, i64 {}, i64 {})\n", c.s, k, v)); }
                _ => { self.declare("declare void @gt_set_insert(ptr, i64)"); let v = self.expr(&args[1])?; let v = self.to_slot(&v); self.body.push_str(&format!("  call void @gt_set_insert(ptr {}, i64 {})\n", c.s, v)); } }
                Ok(Val::new(&Ty::Void, "0")) }
            "has" | "contains" if !matches!(args[0].ty, Ty::Str) => { let c = self.expr(&args[0])?; let k = self.expr(&args[1])?; let k = self.to_slot(&k); let (fdecl, fname) = match args[0].ty.clone() { Ty::Map(..) => ("declare i64 @gt_map_has(ptr, i64)", "gt_map_has"), Ty::Set(..) => ("declare i64 @gt_set_has(ptr, i64)", "gt_set_has"), _ => ("declare i64 @gt_list_has(ptr, i64)", "gt_list_has") }; self.declare(fdecl); let r = self.new_reg(); self.body.push_str(&format!("  {} = call i64 @{}(ptr {}, i64 {})\n", r, fname, c.s, k)); let bb = self.new_reg(); self.body.push_str(&format!("  {} = icmp ne i64 {}, 0\n", bb, r)); Ok(Val::new(&Ty::Bool, bb)) }
            "remove" => { let c = self.expr(&args[0])?; let k = self.expr(&args[1])?; let k = self.to_slot(&k); match args[0].ty.clone() {
                Ty::Map(..) => { self.declare("declare void @gt_map_remove(ptr, i64)"); self.body.push_str(&format!("  call void @gt_map_remove(ptr {}, i64 {})\n", c.s, k)); }
                Ty::Set(..) => { self.declare("declare void @gt_set_remove(ptr, i64)"); self.body.push_str(&format!("  call void @gt_set_remove(ptr {}, i64 {})\n", c.s, k)); }
                _ => { self.declare("declare void @gt_list_remove(ptr, i64)"); self.body.push_str(&format!("  call void @gt_list_remove(ptr {}, i64 {})\n", c.s, k)); } }
                Ok(Val::new(&Ty::Void, "0")) }
            "keys" => { self.declare("declare ptr @gt_map_keys(ptr)"); let c = self.expr(&args[0])?; let r = self.new_reg(); self.body.push_str(&format!("  {} = call ptr @gt_map_keys(ptr {})\n", r, c.s)); let kt = match args[0].ty.clone() { Ty::Map(k, _) => (*k).clone(), _ => Ty::I64 }; Ok(Val::new(&Ty::List(Box::new(kt)), r)) }
            "values" => { self.declare("declare ptr @gt_map_values(ptr)"); let c = self.expr(&args[0])?; let r = self.new_reg(); self.body.push_str(&format!("  {} = call ptr @gt_map_values(ptr {})\n", r, c.s)); let vt = match args[0].ty.clone() { Ty::Map(_, v) => (*v).clone(), _ => Ty::I64 }; Ok(Val::new(&Ty::List(Box::new(vt)), r)) }
            "abs" => { let v = self.expr(&args[0])?; let is_f = v.ty == Ty::F64; if is_f { self.declare("declare double @gt_abs_f(double)") } else { self.declare("declare i64 @gt_abs_i(i64)") } let r = self.new_reg(); if is_f { self.body.push_str(&format!("  {} = call double @gt_abs_f(double {})\n", r, v.s)); Ok(Val::new(&Ty::F64, r)) } else { self.body.push_str(&format!("  {} = call i64 @gt_abs_i(i64 {})\n", r, v.s)); Ok(Val::new(&Ty::I64, r)) } }
            "min" | "max" => { let a = self.expr(&args[0])?; let b2 = self.expr(&args[1])?; let is_f = a.ty == Ty::F64 || b2.ty == Ty::F64; let (suf, argt, ret) = if is_f { ("f", "double", Ty::F64) } else { ("i", "i64", Ty::I64) }; self.declare(&format!("declare {} @gt_{}_{}({}, {})", ret.llvm(), name, suf, argt, argt)); let r = self.new_reg(); self.body.push_str(&format!("  {} = call {} @gt_{}_{}({} {}, {} {})\n", r, ret.llvm(), name, suf, argt, a.s, argt, b2.s)); Ok(Val::new(&ret, r)) }
            "sum" => { let l = self.expr(&args[0])?; let is_f = matches!(&args[0].ty, Ty::List(e) if **e == Ty::F64); let (fname, ret) = if is_f { ("gt_sum_f", Ty::F64) } else { ("gt_sum_i", Ty::I64) }; self.declare(&format!("declare {} @{}(ptr)", ret.llvm(), fname)); let r = self.new_reg(); self.body.push_str(&format!("  {} = call {} @{}(ptr {})\n", r, ret.llvm(), fname, l.s)); Ok(Val::new(&ret, r)) }
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
            "join" => { let l = self.expr(&args[0])?; let sep = self.expr(&args[1])?; self.declare("declare ptr @gt_str_join(ptr, ptr)"); let r = self.new_reg(); self.body.push_str(&format!("  {} = call ptr @gt_str_join(ptr {}, ptr {})\n", r, l.s, sep.s)); Ok(Val::new(&Ty::Str, r)) }
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
                        let ctor: Option<(&str, &Expr)> = match &p.kind {
                            ExprKind::Ok(a) => Some(("Ok", a)),
                            ExprKind::Err(a) => Some(("Err", a)),
                            ExprKind::Some(a) => Some(("Some", a)),
                            // `Ok(v)` 在 parser 中是 Call("Ok", [Ident])
                            ExprKind::Call(n, args) if matches!(n.as_str(), "Ok" | "Err" | "Some") && args.len() == 1 => Some((n.as_str(), &args[0])),
                            _ => None,
                        };
                        if let Some((cname, carg)) = ctor {
                            let want_tag: i64 = if cname == "Err" { 1 } else { 0 };
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
                            if let ExprKind::Ident(bn) = &carg.kind {
                                let bty = match &subj.ty {
                                    Ty::Result(t, e) => if cname == "Ok" { (**t).clone() } else { (**e).clone() },
                                    Ty::Option(t) => (**t).clone(),
                                    _ => Ty::I64,
                                };
                                self.declare("declare i64 @gt_result_val(ptr)");
                                let val = self.new_reg();
                                self.body.push_str(&format!("  {} = call i64 @gt_result_val(ptr {})\n", val, sp));
                                let slot = self.new_alloca(&bty);
                                if bty == Ty::F64 {
                                    let d = self.new_reg();
                                    self.body.push_str(&format!("  {} = bitcast i64 {} to double\n", d, val));
                                    self.body.push_str(&format!("  store double {}, ptr {}\n", d, slot));
                                } else {
                                    self.body.push_str(&format!("  store i64 {}, ptr {}\n", val, slot));
                                }
                                pending_binds.push((bn.clone(), Local { ptr: slot, ty: bty }));
                            }
                            match &arm.guard { None => eq, Some(g) => { let gv = self.cond(g)?; let r = self.new_reg(); self.body.push_str(&format!("  {} = and i1 {}, {}\n", r, eq, gv)); r } }
                        } else if let ExprKind::EnumLit(en, var, binds) = &p.kind {
                            // 枚举解构：比较 tag 并暂存载荷
                            let vidx = self.enum_variants.get(en).and_then(|vs| vs.iter().position(|(n, _)| n == var)).unwrap_or(0);
                            let tp = self.new_reg();
                            self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 0\n", tp, subj.s));
                            let tag = self.new_reg();
                            self.body.push_str(&format!("  {} = load i64, ptr {}\n", tag, tp));
                            let eq = self.new_reg();
                            self.body.push_str(&format!("  {} = icmp eq i64 {}, {}\n", eq, tag, vidx));
                            // 绑定：把载荷存入新槽（在 arm 块内注入，见下方 bindings）
                            let ptys: Vec<Ty> = self.enum_variants.get(en).and_then(|vs| vs.iter().find(|(n, _)| n == var).map(|(_, ts)| ts.clone())).unwrap_or_default();
                            for (i, b) in binds.iter().enumerate() {
                                if let ExprKind::Ident(bn) = &b.kind {
                                    let bty = ptys.get(i).cloned().unwrap_or(Ty::I64);
                                    let lp = self.new_reg();
                                    self.body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 {}\n", lp, subj.s, i + 1));
                                    let lv = self.new_reg();
                                    self.body.push_str(&format!("  {} = load i64, ptr {}\n", lv, lp));
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
                                }
                            }
                            match &arm.guard { None => eq, Some(g) => { let gv = self.cond(g)?; let r = self.new_reg(); self.body.push_str(&format!("  {} = and i1 {}, {}\n", r, eq, gv)); r } }
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
        if a.ty == Ty::Str || b.ty == Ty::Str {
            self.declare("declare i32 @strcmp(ptr, ptr)");
            let c = self.new_reg();
            self.body.push_str(&format!("  {} = call i32 @strcmp(ptr {}, ptr {})\n", c, a.s, b.s));
            self.body.push_str(&format!("  {} = icmp eq i32 {}, 0\n", r, c));
        } else if a.ty == Ty::F64 || b.ty == Ty::F64 {
            self.body.push_str(&format!("  {} = fcmp oeq double {}, {}\n", r, a.s, b.s));
        } else {
            self.body.push_str(&format!("  {} = icmp eq i64 {}, {}\n", r, a.s, b.s));
        }
        Ok(r)
    }

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
        if v.ty == *to || to == &Ty::Unknown || v.ty == Ty::Unknown { return Ok(v.clone()); }
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
            _ => v.s.clone(),
        }
    }
    pub(crate) fn to_slot(&mut self, v: &Val) -> String {
        if v.ty.llvm() == "ptr" { let r = self.new_reg(); self.body.push_str(&format!("  {} = ptrtoint ptr {} to i64\n", r, v.s)); r }
        else if v.ty == Ty::F64 { let r = self.new_reg(); self.body.push_str(&format!("  {} = bitcast double {} to i64\n", r, v.s)); r }
        else if v.ty == Ty::Bool { let r = self.new_reg(); self.body.push_str(&format!("  {} = zext i1 {} to i64\n", r, v.s)); r }
        else { v.s.clone() }
    }
    pub(crate) fn from_slot(&mut self, s: &str, ty: &Ty) -> String {
        match ty {
            Ty::Str | Ty::List(..) | Ty::Set(..) | Ty::Map(..) | Ty::Array(..) | Ty::Struct(_) => { let r = self.new_reg(); self.body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", r, s)); r }
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
        let v = self.expr(e)?;
        self.declare("declare i32 @gt_printf(ptr, ...)");
        match v.ty {
            Ty::F64 => { let f = self.intern(b"%g"); self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {}, double {})\n", f, v.s)); }
            Ty::Str => { let f = self.intern(b"%s"); self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {}, ptr {})\n", f, v.s)); }
            Ty::Bool => {
                let t = self.intern(b"true"); let fa = self.intern(b"false");
                let r = self.new_reg();
                self.body.push_str(&format!("  {} = select i1 {}, ptr {}, ptr {}\n", r, v.s, t, fa));
                let f = self.intern(b"%s");
                self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {}, ptr {})\n", f, r));
            }
            _ => { let f = self.intern(b"%lld"); self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {}, i64 {})\n", f, v.s)); }
        }
        if newline { let f = self.intern(b"\n"); self.declare("declare i32 @gt_printf(ptr, ...)"); self.body.push_str(&format!("  call i32 (ptr, ...) @gt_printf(ptr {})\n", f)); }
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
