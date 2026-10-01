//! FnState 的调用与辅助方法。

use super::*;

impl FnState {
    pub(crate) fn gen_call(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, name: &str, args: &[Expr], line: usize, call_ty: &Ty) -> Result<(Value, Ty), String> {
        // `s.方法(args)`：s 是 dyn Trait 对象 → vtable 取址 + call_indirect
        if let Some(dot) = name.find('.') {
            let recv = &name[..dot];
            if let Some((var, Ty::Dyn(tr))) = self.lookup(recv) {
                let method = name[dot+1..].to_string();
                let idx = jit.traits.get(&tr).and_then(|ms| ms.iter().position(|m| *m == method)).map(|i| i + 1).unwrap_or(0);
                let rv = b.use_var(var);
                let data = b.ins().load(types::I64, MemFlags::new(), rv, 0);
                let addr = b.ins().load(types::I64, MemFlags::new(), rv, (idx * 8) as i32);
                let fp = addr;
                let mut vals: Vec<Value> = vec![data];
                for a in args { let v = self.gen_expr(jit, b, a)?; vals.push(self.convert(b, &v, &Ty::I64)); }
                let mut sig = jit.module.make_signature();
                for _ in 0..vals.len() { sig.params.push(AbiParam::new(types::I64)); }
                if *call_ty != Ty::Void { sig.returns.push(AbiParam::new(cl_ty(call_ty))); }
                let sigref = b.import_signature(sig);
                let call = b.ins().call_indirect(sigref, fp, &vals);
                if *call_ty == Ty::Void { return Ok((b.ins().iconst(types::I64, 0), Ty::Void)); }
                return Ok((b.inst_results(call)[0], call_ty.clone()));
            }
        }
        // web.serve_fn(port, handler)：第二参数是 GTLang 函数名 → 取函数地址传给运行时
        if name == "serve_fn" && args.len() == 2 {
            let port_v = self.gen_expr(jit, b, &args[0])?;
            let port = self.convert(b, &port_v, &Ty::I64);
            let fname = match &args[1].kind {
                ExprKind::Ident(n) => n.clone(),
                _ => return Err(crate::lb!(line, "serve_fn second arg must be a function name", "serve_fn 第二参数须是函数名")),
            };
            let info = jit.fns.get(&fname).ok_or_else(|| crate::lb!(line, "serve_fn: function '{}' not found", "serve_fn: 未找到函数 '{}'", fname))?;
            let fid = info.fid;
            let fref = jit.module.declare_func_in_func(fid, b.func);
            let faddr = b.ins().func_addr(types::I64, fref);
            let fid = *jit.rt.get("py_serve_fn").ok_or_else(|| "标准库符号 py_serve_fn 未注册".to_string())?;
            let f = jit.module.declare_func_in_func(fid, b.func);
            let call = b.ins().call(f, &[port, faddr]);
            return Ok((b.inst_results(call)[0], Ty::I64));
        }
        let arg_tys: Vec<Ty> = args.iter().map(|a| a.ty.clone()).collect();
        if let Some(r) = builtin_ret(name, &arg_tys) { r.map_err(|why| crate::lb!(line, "{}", "{}", why))?; }
        match name {
            "Ok" => {
                let v = self.gen_expr(jit, b, &args[0])?;
                let iv = self.convert(b, &v, &Ty::I64);
                let zero = b.ins().iconst(types::I64, 0);
                let f = self.rt_ref(jit, b, "result_new")?;
                let call = b.ins().call(f, &[zero, iv]);
                return Ok((b.inst_results(call)[0], call_ty.clone()));
            }
            "Err" => {
                let v = self.gen_expr(jit, b, &args[0])?;
                let iv = self.convert(b, &v, &Ty::I64);
                let one = b.ins().iconst(types::I64, 1);
                let f = self.rt_ref(jit, b, "result_new")?;
                let call = b.ins().call(f, &[one, iv]);
                return Ok((b.inst_results(call)[0], call_ty.clone()));
            }
            "Some" => {
                let v = self.gen_expr(jit, b, &args[0])?;
                let iv = self.convert(b, &v, &Ty::I64);
                let zero = b.ins().iconst(types::I64, 0);
                let f = self.rt_ref(jit, b, "result_new")?;
                let call = b.ins().call(f, &[zero, iv]);
                return Ok((b.inst_results(call)[0], call_ty.clone()));
            }
            "None" => {
                let zero = b.ins().iconst(types::I64, 0);
                let one = b.ins().iconst(types::I64, 1);
                let f = self.rt_ref(jit, b, "result_new")?;
                let call = b.ins().call(f, &[one, zero]);
                return Ok((b.inst_results(call)[0], call_ty.clone()));
            }
            "put" | "print" => {
                if args.is_empty() { return Err(crate::lb!(line, "{}() requires 1 argument", "{}() 需要 1 个参数", name)); }
                self.gen_print(jit, b, &args[0])?;
                if name == "put" {
                    let nl = jit.data_id_of(b"\n").ok_or_else(|| "内部错误：换行符未预置".to_string())?;
                    let p = data_ptr(jit, b, nl);
                    let f = self.rt_ref(jit, b, "put_str")?;
                    b.ins().call(f, &[p]);
                }
                Ok((b.ins().iconst(types::I64, 0), Ty::Void))
            }
            "len" => match &args[0].ty {
                Ty::Array(_, n) => Ok((b.ins().iconst(types::I64, *n as i64), Ty::I64)),
                Ty::Str => { let v = self.gen_expr(jit, b, &args[0])?; let f = self.rt_ref(jit, b, "str_len")?; let call = b.ins().call(f, &[v.0]); Ok((b.inst_results(call)[0], Ty::I64)) }
                Ty::List(_) | Ty::Set(_) | Ty::Map(..) => {
                    let v = self.gen_expr(jit, b, &args[0])?;
                    let key = match &args[0].ty { Ty::List(_) => "list_len", Ty::Set(_) => "set_len", _ => "map_len" };
                    let f = self.rt_ref(jit, b, key)?;
                    let call = b.ins().call(f, &[v.0]);
                    Ok((b.inst_results(call)[0], Ty::I64))
                }
                other => Err(crate::lb!(line, "len() does not support {}", "len() 不支持 {}", other)),
            },
            "str" | "string" => { let v = self.gen_expr(jit, b, &args[0])?; self.gen_to_str(jit, b, &v) }
            "int" | "i64" => { let v = self.gen_expr(jit, b, &args[0])?; self.gen_to_i64(jit, b, &v) }
            "f64" | "float" => { let v = self.gen_expr(jit, b, &args[0])?; self.gen_to_f64(jit, b, &v) }
            "bool" => { let v = self.gen_expr(jit, b, &args[0])?; self.gen_to_bool(jit, b, &v) }
            "list" | "List" => { let f = self.rt_ref(jit, b, "list_new")?; let epv = match call_ty { Ty::List(e) => crate::codegen::elem_is_ptr(e), _ => 0 }; let ep = b.ins().iconst(types::I64, epv); let call = b.ins().call(f, &[ep]); Ok((b.inst_results(call)[0], call_ty.clone())) }
            "range" => {
                let a = self.gen_expr(jit, b, &args[0])?;
                let av = self.convert(b, &a, &Ty::I64);
                let bv = if args.len() > 1 { let x = self.gen_expr(jit, b, &args[1])?; self.convert(b, &x, &Ty::I64) } else { b.ins().iconst(types::I64, 0) };
                let (lo, hi) = if args.len() > 1 { (av, bv) } else { (bv, av) };
                let f = self.rt_ref(jit, b, "range")?;
                let call = b.ins().call(f, &[lo, hi]);
                Ok((b.inst_results(call)[0], call_ty.clone()))
            }
            "chan" => {
                let f = self.rt_ref(jit, b, "chan_new")?;
                let call = b.ins().call(f, &[]);
                Ok((b.inst_results(call)[0], Ty::I64))
            }
            "chan_send" => {
                let c = self.gen_expr(jit, b, &args[0])?;
                let v = self.gen_expr(jit, b, &args[1])?;
                let iv = self.convert(b, &v, &Ty::I64);
                let f = self.rt_ref(jit, b, "chan_send")?;
                b.ins().call(f, &[c.0, iv]);
                Ok((b.ins().iconst(types::I64, 0), Ty::Void))
            }
            "chan_recv" => {
                let c = self.gen_expr(jit, b, &args[0])?;
                let f = self.rt_ref(jit, b, "chan_recv")?;
                let call = b.ins().call(f, &[c.0]);
                Ok((b.inst_results(call)[0], Ty::I64))
            }
            "sleep" => {
                let a = self.gen_expr(jit, b, &args[0])?;
                let av = self.convert(b, &a, &Ty::I64);
                let f = self.rt_ref(jit, b, "sleep")?;
                b.ins().call(f, &[av]);
                Ok((b.ins().iconst(types::I64, 0), Ty::Void))
            }
            "read_line" | "readline" | "input" => {
                let f = self.rt_ref(jit, b, "read_line")?;
                let call = b.ins().call(f, &[]);
                Ok((b.inst_results(call)[0], Ty::Str))
            }
            "read_int" | "readint" => {
                let f = self.rt_ref(jit, b, "read_int")?;
                let call = b.ins().call(f, &[]);
                Ok((b.inst_results(call)[0], Ty::I64))
            }
            "assert" => {
                let c = self.gen_expr(jit, b, &args[0])?;
                let cv = self.convert(b, &c, &Ty::I64);
                let mv = if args.len() > 1 { let m = self.gen_expr(jit, b, &args[1])?; self.convert(b, &m, &Ty::I64) } else { b.ins().iconst(types::I64, 0) };
                let lv = b.ins().iconst(types::I64, args[0].line as i64);
                let f = self.rt_ref(jit, b, "assert")?;
                b.ins().call(f, &[cv, mv, lv]);
                Ok((b.ins().iconst(types::I64, 0), Ty::Void))
            }
            "set" | "Set" => { let f = self.rt_ref(jit, b, "set_new")?; let epv = match call_ty { Ty::Set(e) => crate::codegen::elem_is_ptr(e), _ => 0 }; let ep = b.ins().iconst(types::I64, epv); let call = b.ins().call(f, &[ep]); Ok((b.inst_results(call)[0], call_ty.clone())) }
            "map" | "Map" | "dict" => { let f = self.rt_ref(jit, b, "map_new")?; let epv = match call_ty { Ty::Map(k, v) => crate::codegen::elem_is_ptr(k) | crate::codegen::elem_is_ptr(v), _ => 0 }; let ep = b.ins().iconst(types::I64, epv); let call = b.ins().call(f, &[ep]); Ok((b.inst_results(call)[0], call_ty.clone())) }
            "push" | "append" => {
                let l = self.gen_expr(jit, b, &args[0])?;
                let v = self.gen_expr(jit, b, &args[1])?;
                let iv = self.convert(b, &v, &Ty::I64);
                let f = self.rt_ref(jit, b, "list_push")?;
                b.ins().call(f, &[l.0, iv]);
                Ok((b.ins().iconst(types::I64, 0), Ty::Void))
            }
            "pop" => { let l = self.gen_expr(jit, b, &args[0])?; let f = self.rt_ref(jit, b, "list_pop")?; let call = b.ins().call(f, &[l.0]); Ok((b.inst_results(call)[0], call_ty.clone())) }
            "at" => {
                let c = self.gen_expr(jit, b, &args[0])?;
                let i = self.gen_expr(jit, b, &args[1])?;
                let ii = self.convert(b, &i, &Ty::I64);
                let (key, ret) = match &args[0].ty { Ty::List(e) => ("list_at", (**e).clone()), Ty::Map(_, v) => ("map_get", (**v).clone()), _ => ("list_at", Ty::I64) };
                let f = self.rt_ref(jit, b, key)?;
                let call = b.ins().call(f, &[c.0, ii]);
                Ok((b.inst_results(call)[0], ret))
            }
            "insert" => {
                let c = self.gen_expr(jit, b, &args[0])?;
                match &args[0].ty {
                    Ty::Map(..) => {
                        let k = self.gen_expr(jit, b, &args[1])?; let v = self.gen_expr(jit, b, &args[2])?;
                        let ik = self.convert(b, &k, &Ty::I64); let iv = self.convert(b, &v, &Ty::I64);
                        let f = self.rt_ref(jit, b, "map_insert")?; b.ins().call(f, &[c.0, ik, iv]);
                    }
                    _ => { let v = self.gen_expr(jit, b, &args[1])?; let iv = self.convert(b, &v, &Ty::I64); let f = self.rt_ref(jit, b, "set_insert")?; b.ins().call(f, &[c.0, iv]); }
                }
                Ok((b.ins().iconst(types::I64, 0), Ty::Void))
            }
            "has" | "contains" if matches!(args[0].ty, Ty::List(..) | Ty::Set(..) | Ty::Map(..) | Ty::Unknown) => {
                let c = self.gen_expr(jit, b, &args[0])?;
                let k = self.gen_expr(jit, b, &args[1])?;
                let ik = self.convert(b, &k, &Ty::I64);
                let key = match &args[0].ty { Ty::Map(..) => "map_has", Ty::Set(..) => "set_has", _ => "list_has" };
                let f = self.rt_ref(jit, b, key)?;
                let call = b.ins().call(f, &[c.0, ik]);
                Ok((b.inst_results(call)[0], Ty::Bool))
            }
            "remove" => {
                let c = self.gen_expr(jit, b, &args[0])?;
                let k = self.gen_expr(jit, b, &args[1])?;
                let ik = self.convert(b, &k, &Ty::I64);
                let key = match &args[0].ty { Ty::Map(..) => "map_remove", Ty::Set(..) => "set_remove", _ => "list_remove" };
                let f = self.rt_ref(jit, b, key)?;
                b.ins().call(f, &[c.0, ik]);
                Ok((b.ins().iconst(types::I64, 0), Ty::Void))
            }
            "keys" => {
                let c = self.gen_expr(jit, b, &args[0])?;
                let f = self.rt_ref(jit, b, "map_keys")?;
                let call = b.ins().call(f, &[c.0]);
                let kt = match &args[0].ty { Ty::Map(k, _) => (**k).clone(), _ => Ty::I64 };
                Ok((b.inst_results(call)[0], Ty::List(Box::new(kt))))
            }
            "values" => {
                let c = self.gen_expr(jit, b, &args[0])?;
                let f = self.rt_ref(jit, b, "map_values")?;
                let call = b.ins().call(f, &[c.0]);
                let vt = match &args[0].ty { Ty::Map(_, v) => (**v).clone(), _ => Ty::I64 };
                Ok((b.inst_results(call)[0], Ty::List(Box::new(vt))))
            }
            "abs" => {
                let v = self.gen_expr(jit, b, &args[0])?;
                match v.1 {
                    Ty::F64 => { let f = self.rt_ref(jit, b, "abs_f")?; let call = b.ins().call(f, &[v.0]); Ok((b.inst_results(call)[0], Ty::F64)) }
                    _ => { let f = self.rt_ref(jit, b, "abs_i")?; let call = b.ins().call(f, &[v.0]); Ok((b.inst_results(call)[0], Ty::I64)) }
                }
            }
            "min" | "max" => {
                let a = self.gen_expr(jit, b, &args[0])?;
                let c2 = self.gen_expr(jit, b, &args[1])?;
                let is_f = a.1 == Ty::F64 || c2.1 == Ty::F64;
                let key = match (name, is_f) { ("min", true) => "min_f", ("max", true) => "max_f", ("min", false) => "min_i", _ => "max_i" };
                let f = self.rt_ref(jit, b, key)?;
                let call = b.ins().call(f, &[a.0, c2.0]);
                Ok((b.inst_results(call)[0], if is_f { Ty::F64 } else { Ty::I64 }))
            }
            "sum" => {
                let l = self.gen_expr(jit, b, &args[0])?;
                let is_f = matches!(&args[0].ty, Ty::List(e) if **e == Ty::F64);
                let key = if is_f { "sum_f" } else { "sum_i" };
                let f = self.rt_ref(jit, b, key)?;
                let call = b.ins().call(f, &[l.0]);
                Ok((b.inst_results(call)[0], if is_f { Ty::F64 } else { Ty::I64 }))
            }
            "substr" => {
                let s = self.gen_expr(jit, b, &args[0])?; let a = self.gen_expr(jit, b, &args[1])?; let n = self.gen_expr(jit, b, &args[2])?;
                let ai = self.convert(b, &a, &Ty::I64); let ni = self.convert(b, &n, &Ty::I64);
                let f = self.rt_ref(jit, b, "str_substr")?;
                let call = b.ins().call(f, &[s.0, ai, ni]);
                Ok((b.inst_results(call)[0], Ty::Str))
            }
            "find" => {
                let s = self.gen_expr(jit, b, &args[0])?; let sub = self.gen_expr(jit, b, &args[1])?;
                let f = self.rt_ref(jit, b, "str_find")?;
                let call = b.ins().call(f, &[s.0, sub.0]);
                Ok((b.inst_results(call)[0], Ty::I64))
            }
            "upper" | "lower" | "trim" => {
                let s = self.gen_expr(jit, b, &args[0])?;
                let key = match name { "upper" => "str_upper", "lower" => "str_lower", _ => "str_trim" };
                let f = self.rt_ref(jit, b, key)?;
                let call = b.ins().call(f, &[s.0]);
                Ok((b.inst_results(call)[0], Ty::Str))
            }
            "repeat" => {
                let s = self.gen_expr(jit, b, &args[0])?; let n = self.gen_expr(jit, b, &args[1])?;
                let ni = self.convert(b, &n, &Ty::I64);
                let f = self.rt_ref(jit, b, "str_repeat")?;
                let call = b.ins().call(f, &[s.0, ni]);
                Ok((b.inst_results(call)[0], Ty::Str))
            }
            "pad_left" | "pad_right" | "lpad" | "rpad" => {
                let s = self.gen_expr(jit, b, &args[0])?;
                let w = self.gen_expr(jit, b, &args[1])?;
                let wi = self.convert(b, &w, &Ty::I64);
                let fv = if args.len() > 2 { let f = self.gen_expr(jit, b, &args[2])?; f.0 } else { b.ins().iconst(types::I64, 0) };
                let key = if matches!(name, "pad_right" | "rpad") { "pad_right" } else { "pad_left" };
                let f = self.rt_ref(jit, b, key)?;
                let call = b.ins().call(f, &[s.0, wi, fv]);
                Ok((b.inst_results(call)[0], Ty::Str))
            }
            "fmt_int" => {
                let x = self.gen_expr(jit, b, &args[0])?;
                let xi = self.convert(b, &x, &Ty::I64);
                let w = self.gen_expr(jit, b, &args[1])?;
                let wi = self.convert(b, &w, &Ty::I64);
                let f = self.rt_ref(jit, b, "fmt_int")?;
                let call = b.ins().call(f, &[xi, wi]);
                Ok((b.inst_results(call)[0], Ty::Str))
            }
            "replace" => {
                let s = self.gen_expr(jit, b, &args[0])?; let a = self.gen_expr(jit, b, &args[1])?; let c = self.gen_expr(jit, b, &args[2])?;
                let f = self.rt_ref(jit, b, "str_replace")?;
                let call = b.ins().call(f, &[s.0, a.0, c.0]);
                Ok((b.inst_results(call)[0], Ty::Str))
            }
            "split" => {
                let s = self.gen_expr(jit, b, &args[0])?; let sep = self.gen_expr(jit, b, &args[1])?;
                let f = self.rt_ref(jit, b, "str_split")?;
                let call = b.ins().call(f, &[s.0, sep.0]);
                Ok((b.inst_results(call)[0], call_ty.clone()))
            }
            "join" => {
                let l = self.gen_expr(jit, b, &args[0])?; let sep = self.gen_expr(jit, b, &args[1])?;
                let f = self.rt_ref(jit, b, "str_join")?;
                let call = b.ins().call(f, &[l.0, sep.0]);
                Ok((b.inst_results(call)[0], Ty::Str))
            }
            "mem_alloc" => { let n = self.gen_expr(jit, b, &args[0])?; let ni = self.convert(b, &n, &Ty::I64); let f = self.rt_ref(jit, b, "mem_alloc")?; let call = b.ins().call(f, &[ni]); Ok((b.inst_results(call)[0], Ty::I64)) }
            "mem_free" => { let p = self.gen_expr(jit, b, &args[0])?; let f = self.rt_ref(jit, b, "mem_free")?; b.ins().call(f, &[p.0]); Ok((b.ins().iconst(types::I64, 0), Ty::Void)) }
            "mem_store_i64" | "mem_store_u8" | "mem_set" => {
                let key = match name { "mem_store_i64" => "mem_store_i64", "mem_store_u8" => "mem_store_u8", _ => "mem_set" };
                let p = self.gen_expr(jit, b, &args[0])?; let o = self.gen_expr(jit, b, &args[1])?; let v = self.gen_expr(jit, b, &args[2])?;
                let oi = self.convert(b, &o, &Ty::I64); let vi = self.convert(b, &v, &Ty::I64);
                let f = self.rt_ref(jit, b, key)?; b.ins().call(f, &[p.0, oi, vi]);
                Ok((b.ins().iconst(types::I64, 0), Ty::Void))
            }
            "mem_load_i64" | "mem_load_u8" => {
                let key = if name == "mem_load_i64" { "mem_load_i64" } else { "mem_load_u8" };
                let p = self.gen_expr(jit, b, &args[0])?; let o = self.gen_expr(jit, b, &args[1])?;
                let oi = self.convert(b, &o, &Ty::I64);
                let f = self.rt_ref(jit, b, key)?;
                let call = b.ins().call(f, &[p.0, oi]);
                Ok((b.inst_results(call)[0], Ty::I64))
            }
            "mem_copy" => {
                let d = self.gen_expr(jit, b, &args[0])?; let s = self.gen_expr(jit, b, &args[1])?; let n = self.gen_expr(jit, b, &args[2])?;
                let ni = self.convert(b, &n, &Ty::I64);
                let f = self.rt_ref(jit, b, "mem_copy")?; b.ins().call(f, &[d.0, s.0, ni]);
                Ok((b.ins().iconst(types::I64, 0), Ty::Void))
            }
            _ => {
                // 用户函数优先于标准库（同名遮蔽）
                if let Some(info) = jit.fns.get(name) {
                    let fid = info.fid; let params = info.params.clone(); let ret = info.ret.clone();
                    let mut vals = Vec::new();
                    for (i, a) in args.iter().enumerate() {
                        let got = self.gen_expr(jit, b, a)?;
                        let want = params.get(i).cloned().unwrap_or(Ty::I64);
                        vals.push(self.convert(b, &got, &want));
                    }
                    let fref = jit.module.declare_func_in_func(fid, b.func);
                    let call = b.ins().call(fref, &vals);
                    if ret == Ty::Void { Ok((b.ins().iconst(types::I64, 0), Ty::Void)) } else { Ok((b.inst_results(call)[0], ret)) }
                } else if let Some(sf) = crate::types::gtlib_fn(name) {
                    let mut vals = Vec::new();
                    for (i, a) in args.iter().enumerate() {
                        let got = self.gen_expr(jit, b, a)?;
                        let want = sf.params.get(i).cloned().unwrap_or(Ty::I64);
                        vals.push(self.convert(b, &got, &want));
                    }
                    let fid = *jit.rt.get(sf.symbol).ok_or_else(|| format!("标准库符号 {} 未注册", sf.symbol))?;
                    let fref = jit.module.declare_func_in_func(fid, b.func);
                    let call = b.ins().call(fref, &vals);
                    if sf.ret == Ty::Void { Ok((b.ins().iconst(types::I64, 0), Ty::Void)) } else { Ok((b.inst_results(call)[0], sf.ret.clone())) }
                } else {
                    Err(crate::lb!(line, "undefined function '{}'", "未定义的函数 '{}'", name))
                }
            }
        }
    }

    pub(crate) fn gen_call_value(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, callee: &Expr, args: &[Expr], e: &Expr) -> Result<(Value, Ty), String> {
        let cb = self.gen_expr(jit, b, callee)?;
        let (ptypes, ret) = match &e.ty {
            Ty::Closure(p, r) => (p.clone(), (**r).clone()),
            _ => match &callee.ty { Ty::Closure(p, r) => (p.clone(), (**r).clone()), _ => (vec![Ty::I64; args.len()], Ty::I64) },
        };
        let fp = b.ins().load(types::I64, MemFlags::new(), cb.0, 0);
        let ncap = ptypes.len().saturating_sub(args.len());
        let mut vals: Vec<Value> = Vec::new();
        for i in 0..ncap {
            let v = b.ins().load(types::I64, MemFlags::new(), cb.0, ((i + 1) * 8) as i32);
            vals.push(v);
        }
        for (i, a) in args.iter().enumerate() {
            let got = self.gen_expr(jit, b, a)?;
            let want = ptypes.get(ncap + i).cloned().unwrap_or(Ty::I64);
            vals.push(self.convert(b, &got, &want));
        }
        let mut sig = jit.module.make_signature();
        // 捕获值按 I64；用户参数按标注/推断类型
        for _ in 0..ncap { sig.params.push(AbiParam::new(types::I64)); }
        for i in 0..args.len() { let t = ptypes.get(ncap + i).cloned().unwrap_or(Ty::I64); sig.params.push(AbiParam::new(cl_ty(&t))); }
        if ret != Ty::Void { sig.returns.push(AbiParam::new(cl_ty(&ret))); }
        let sigref = b.import_signature(sig);
        let call = b.ins().call_indirect(sigref, fp, &vals);
        if ret == Ty::Void { Ok((b.ins().iconst(types::I64, 0), Ty::Void)) } else { Ok((b.inst_results(call)[0], ret)) }
    }

    pub(crate) fn convert(&mut self, b: &mut FunctionBuilder, v: &(Value, Ty), to: &Ty) -> Value {
        if v.1 == *to || to == &Ty::Unknown || v.1 == Ty::Unknown { return v.0; }
        match (to, &v.1) {
            (Ty::F64, t) if !t.is_float() => b.ins().fcvt_from_sint(types::F64, v.0),
            (t, Ty::F64) if !t.is_float() => b.ins().fcvt_to_sint(types::I64, v.0),
            _ => v.0,
        }
    }

    pub(crate) fn gen_binop(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, op: BinOp, a: &(Value, Ty), c: &(Value, Ty), line: usize, safe: bool) -> Result<(Value, Ty), String> {
        // 字符串拼接：`str + str`
        if op == BinOp::Add && a.1 == Ty::Str && c.1 == Ty::Str {
            let f = self.rt_ref(jit, b, "str_concat")?;
            let call = b.ins().call(f, &[a.0, c.0]);
            return Ok((b.inst_results(call)[0], Ty::Str));
        }
        if op.is_cmp() && (a.1 == Ty::Str || c.1 == Ty::Str) {
            let eq = self.gen_eq(jit, b, a, c)?;
            let r = b.ins().uextend(types::I64, eq);
            if matches!(op, BinOp::Ne) { let one = b.ins().iconst(types::I64, 1); return Ok((b.ins().bxor(r, one), Ty::Bool)); }
            return Ok((r, Ty::Bool));
        }
        let use_f = a.1 == Ty::F64 || c.1 == Ty::F64;
        let x = if use_f { self.convert(b, a, &Ty::F64) } else { self.convert(b, a, &Ty::I64) };
        let y = if use_f { self.convert(b, c, &Ty::F64) } else { self.convert(b, c, &Ty::I64) };
        if op.is_cmp() {
            let cc = match op { BinOp::Eq => IntCC::Equal, BinOp::Ne => IntCC::NotEqual, BinOp::Lt => IntCC::SignedLessThan, BinOp::Le => IntCC::SignedLessThanOrEqual, BinOp::Gt => IntCC::SignedGreaterThan, _ => IntCC::SignedGreaterThanOrEqual };
            let fcc = match op { BinOp::Eq => FloatCC::Equal, BinOp::Ne => FloatCC::NotEqual, BinOp::Lt => FloatCC::LessThan, BinOp::Le => FloatCC::LessThanOrEqual, BinOp::Gt => FloatCC::GreaterThan, _ => FloatCC::GreaterThanOrEqual };
            let cmp = if use_f { b.ins().fcmp(fcc, x, y) } else { b.ins().icmp(cc, x, y) };
            return Ok((b.ins().uextend(types::I64, cmp), Ty::Bool));
        }
        if op.is_bit() {
            return Ok(match op {
                BinOp::BitAnd => (b.ins().band(x, y), Ty::I64),
                BinOp::BitOr => (b.ins().bor(x, y), Ty::I64),
                BinOp::BitXor => (b.ins().bxor(x, y), Ty::I64),
                BinOp::Shl => (b.ins().ishl(x, y), Ty::I64),
                _ => (b.ins().sshr(x, y), Ty::I64),
            });
        }
        if matches!(op, BinOp::Div | BinOp::FloorDiv | BinOp::Rem) && !use_f { self.gen_div_zero_check(jit, b, y, line)?; }
        if use_f {
            Ok(match op { BinOp::Add => (b.ins().fadd(x, y), Ty::F64), BinOp::Sub => (b.ins().fsub(x, y), Ty::F64), BinOp::Mul => (b.ins().fmul(x, y), Ty::F64), _ => (b.ins().fdiv(x, y), Ty::F64) })
        } else {
            match op {
                // 加/减/乘：检测有符号溢出，溢出则运行时终止（与编译器后端一致）
                // safe=true：范围分析已证明不会溢出 → 用普通指令，省略检查
                // safe（范围分析已证）或全局关闭检查时，省略溢出检查
                BinOp::Add => if safe || !crate::codegen::overflow_check_enabled_pub() { Ok((b.ins().iadd(x, y), Ty::I64)) } else { let (r, o) = b.ins().sadd_overflow(x, y); self.gen_overflow_check(jit, b, o, line)?; Ok((r, Ty::I64)) }
                BinOp::Sub => if safe || !crate::codegen::overflow_check_enabled_pub() { Ok((b.ins().isub(x, y), Ty::I64)) } else { let (r, o) = b.ins().ssub_overflow(x, y); self.gen_overflow_check(jit, b, o, line)?; Ok((r, Ty::I64)) }
                BinOp::Mul => if safe || !crate::codegen::overflow_check_enabled_pub() { Ok((b.ins().imul(x, y), Ty::I64)) } else { let (r, o) = b.ins().smul_overflow(x, y); self.gen_overflow_check(jit, b, o, line)?; Ok((r, Ty::I64)) }
                BinOp::Div | BinOp::FloorDiv => Ok((b.ins().sdiv(x, y), Ty::I64)),
                _ => Ok((b.ins().srem(x, y), Ty::I64)),
            }
        }
    }

    pub(crate) fn gen_elem_addr(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, base: &Value, ty: &Ty, idx: &Expr, line: usize) -> Result<(Ty, Value), String> {
        let elem = match ty { Ty::Array(el, _) => (**el).clone(), _ => return Err(crate::lb!(line, "{} does not support indexing", "{} 不支持下标", ty)) };
        let iv = self.gen_expr(jit, b, idx)?;
        let i0 = self.convert(b, &iv, &Ty::I64);
        let mut i = i0;
        if let Ty::Array(_, n) = ty {
            // 省略边界检查的两种安全情形：
            //   1) 常量下标且已知在范围内；
            //   2) 下标是 `for v in 0..M` 的循环变量，且数组长度 ≥ M（v 必在范围内）。
            let skip = match &idx.kind {
                ExprKind::Int(v) => *v >= 0 && (*v as usize) < *n,
                ExprKind::Ident(v) => self.bounded.get(v).map(|m| (*m as usize) <= *n).unwrap_or(false),
                _ => false,
            };
            let nl = b.ins().iconst(types::I64, *n as i64);
            i = self.norm_idx(b, i0, nl);
            if !skip {
                self.gen_bounds(jit, b, i, nl, line)?;
            }
        }
        let off = b.ins().imul_imm(i, 8);
        let addr = b.ins().iadd(*base, off);
        Ok((elem, addr))
    }

    /// 负索引归一化：idx < 0 ? idx + len : idx。
    pub(crate) fn norm_idx(&mut self, b: &mut FunctionBuilder, idx: Value, len: Value) -> Value {
        let neg = b.ins().icmp_imm(IntCC::SignedLessThan, idx, 0);
        let add = b.ins().iadd(idx, len);
        b.ins().select(neg, add, idx)
    }

    pub(crate) fn gen_bounds(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, idx: Value, len: Value, line: usize) -> Result<(), String> {
        let bad = self.new_block(b);
        let cont = self.new_block(b);
        // 单次**无符号**比较同时判掉负数与越界（负数视为巨大无符号数）：比两次有符号比较更省
        let oob = b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, idx, len);
        b.ins().brif(oob, bad, &[], cont, &[]);
        b.switch_to_block(bad); self.terminated = false;
        let f = self.rt_ref(jit, b, "bounds")?;
        let l = b.ins().iconst(types::I64, line as i64);
        b.ins().call(f, &[idx, len, l]);
        b.ins().trap(TrapCode::user(1).unwrap());
        b.switch_to_block(cont); self.terminated = false;
        Ok(())
    }

    /// 阶段 4C 预留：故障中止（try 内跳捕获块）。当前统一运行时终止。
    pub(crate) fn emit_fault(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, _code: i64, line: usize, rt_key: &str, trap_code: u8) -> Result<(), String> {
        let f = self.rt_ref(jit, b, rt_key)?;
        let l = b.ins().iconst(types::I64, line as i64);
        b.ins().call(f, &[l]);
        b.ins().trap(TrapCode::user(trap_code).unwrap());
        Ok(())
    }

    pub(crate) fn gen_overflow_check(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, overflow: Value, line: usize) -> Result<(), String> {
        let bad = self.new_block(b);
        let cont = self.new_block(b);
        b.ins().brif(overflow, bad, &[], cont, &[]);
        b.switch_to_block(bad); self.terminated = false;
        self.emit_fault(jit, b, 3, line, "overflow", 3)?;
        b.switch_to_block(cont); self.terminated = false;
        Ok(())
    }

    pub(crate) fn gen_div_zero_check(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, divisor: Value, line: usize) -> Result<(), String> {
        let bad = self.new_block(b);
        let cont = self.new_block(b);
        let nz = b.ins().icmp_imm(IntCC::NotEqual, divisor, 0);
        b.ins().brif(nz, cont, &[], bad, &[]);
        b.switch_to_block(bad); self.terminated = false;
        self.emit_fault(jit, b, 2, line, "div_zero", 2)?;
        b.switch_to_block(cont); self.terminated = false;
        Ok(())
    }

    pub(crate) fn gen_print(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, e: &Expr) -> Result<(), String> {
        let v = self.gen_expr(jit, b, e)?;
        let key = match v.1 { Ty::Str => "put_str", Ty::F64 => "put_f64", Ty::Bool => "put_bool", _ => "put_i64" };
        let f = self.rt_ref(jit, b, key)?;
        b.ins().call(f, &[v.0]);
        Ok(())
    }

    pub(crate) fn gen_to_str(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, v: &(Value, Ty)) -> Result<(Value, Ty), String> {
        let f_new = self.rt_ref(jit, b, "sb_new")?;
        let call = b.ins().call(f_new, &[]);
        let h = b.inst_results(call)[0];
        let push = match v.1 { Ty::F64 => "sb_push_f64", Ty::Str => "sb_push_str", Ty::Bool => "sb_push_bool", _ => "sb_push_i64" };
        let f = self.rt_ref(jit, b, push)?;
        b.ins().call(f, &[h, v.0]);
        let fin = self.rt_ref(jit, b, "sb_finish")?;
        let call = b.ins().call(fin, &[h]);
        Ok((b.inst_results(call)[0], Ty::Str))
    }

    pub(crate) fn gen_to_i64(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, v: &(Value, Ty)) -> Result<(Value, Ty), String> {
        match v.1 {
            Ty::Str => { let f = self.rt_ref(jit, b, "to_i64")?; let call = b.ins().call(f, &[v.0]); Ok((b.inst_results(call)[0], Ty::I64)) }
            Ty::F64 => Ok((b.ins().fcvt_to_sint(types::I64, v.0), Ty::I64)),
            _ => Ok((v.0, Ty::I64)),
        }
    }

    pub(crate) fn gen_to_f64(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, v: &(Value, Ty)) -> Result<(Value, Ty), String> {
        match v.1 {
            Ty::Str => { let f = self.rt_ref(jit, b, "to_f64")?; let call = b.ins().call(f, &[v.0]); Ok((b.inst_results(call)[0], Ty::F64)) }
            Ty::F64 => Ok((v.0, Ty::F64)),
            _ => Ok((b.ins().fcvt_from_sint(types::F64, v.0), Ty::F64)),
        }
    }

    pub(crate) fn gen_to_bool(&mut self, jit: &mut Jit, b: &mut FunctionBuilder, v: &(Value, Ty)) -> Result<(Value, Ty), String> {
        match v.1 {
            Ty::Str => { let f = self.rt_ref(jit, b, "str_nonempty")?; let call = b.ins().call(f, &[v.0]); Ok((b.inst_results(call)[0], Ty::Bool)) }
            Ty::F64 => { let z = b.ins().f64const(0.0); let c = b.ins().fcmp(FloatCC::NotEqual, v.0, z); Ok((b.ins().uextend(types::I64, c), Ty::Bool)) }
            _ => { let c = b.ins().icmp_imm(IntCC::NotEqual, v.0, 0); Ok((b.ins().uextend(types::I64, c), Ty::Bool)) }
        }
    }
}

pub(crate) fn collect_strs_block(b: &Block, out: &mut Vec<Vec<u8>>) {
    for s in b {
        match s {
            Stmt::Let { value, .. } => collect_strs(value, out),
            Stmt::Const { value, .. } => collect_strs(value, out),
            Stmt::Assign { value, .. } => collect_strs(value, out),
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => collect_strs(e, out),
            Stmt::If { cond, then, els, .. } => {
                collect_strs(cond, out);
                collect_strs_block(then, out);
                if let Some(e) = els { collect_strs_block(e, out); }
            }
            Stmt::While { cond, body, .. } => { collect_strs(cond, out); collect_strs_block(body, out); }
            Stmt::ForRange { from, to, body, els, .. } => { collect_strs(from, out); collect_strs(to, out); collect_strs_block(body, out); if let Some(e) = els { collect_strs_block(e, out); } }
            Stmt::ForEach { iter, body, els, .. } => { collect_strs(iter, out); collect_strs_block(body, out); if let Some(e) = els { collect_strs_block(e, out); } }
            Stmt::Block(inner) => collect_strs_block(inner, out),
            Stmt::FieldAssign { value, .. } => collect_strs(value, out),
            Stmt::Throw(e, _) => collect_strs(e, out),
            Stmt::Try { body, catches, fin, .. } => {
                collect_strs_block(body, out);
                for ca in catches {
                    if let Some(g) = &ca.guard { collect_strs(g, out); }
                    collect_strs_block(&ca.body, out);
                }
                if let Some(f) = fin { collect_strs_block(f, out); }
            }
            Stmt::LocalFn(_) => {}
            _ => {}
        }
    }
}

pub(crate) fn collect_strs(e: &Expr, out: &mut Vec<Vec<u8>>) {
    match &e.kind {
        ExprKind::Str(s) => out.push(s.as_bytes().to_vec()),
        ExprKind::Interp(parts) => {
            for p in parts {
                match p {
                    StrPart::Lit(s) => out.push(s.as_bytes().to_vec()),
                    StrPart::Expr(inner) => collect_strs(inner, out),
                }
            }
        }
        ExprKind::Unary(_, a) => collect_strs(a, out),
        ExprKind::Binary(_, a, b) => { collect_strs(a, out); collect_strs(b, out); }
        ExprKind::Call(_, args) => for a in args { collect_strs(a, out); },
        ExprKind::CallValue { callee, args } => { collect_strs(callee, out); for a in args { collect_strs(a, out); } }
        ExprKind::Index(a, b) => { collect_strs(a, out); collect_strs(b, out); }
        ExprKind::Slice(a, b, c) => { collect_strs(a, out); collect_strs(b, out); collect_strs(c, out); }
        ExprKind::ArrayLit(items) => for a in items { collect_strs(a, out); },
        ExprKind::TupleLit(items) => for a in items { collect_strs(a, out); },
        ExprKind::If { cond, then, els } => {
            collect_strs(cond, out);
            collect_strs_block(then, out);
            if let Some(b) = els { collect_strs_block(b, out); }
        }
        ExprKind::Match { subject, arms } => {
            collect_strs(subject, out);
            for arm in arms {
                if let Some(p) = &arm.pat { collect_strs(p, out); }
                if let Some(g) = &arm.guard { collect_strs(g, out); }
                collect_strs_block(&arm.body, out);
            }
        }
        ExprKind::Field(base, _) => collect_strs(base, out),
        ExprKind::StructLit(_, fields) => for (_, v) in fields { collect_strs(v, out); },
        ExprKind::Closure { body, .. } => collect_strs(body, out),
        ExprKind::ClosureNew { captures, .. } => for c in captures { collect_strs(c, out); },
        ExprKind::TryOr { inner, default } => { collect_strs(inner, out); collect_strs(default, out); }
        ExprKind::TryBlock { body, catches, fin } => {
            collect_strs_block(body, out);
            for ca in catches {
                if let Some(g) = &ca.guard { collect_strs(g, out); }
                collect_strs_block(&ca.body, out);
            }
            if let Some(f) = fin { collect_strs_block(f, out); }
        }
        _ => {}
    }
}
