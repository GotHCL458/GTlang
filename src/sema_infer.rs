//! sema 的表达式类型推断（infer）。

use super::*;

impl Ctx {
    /// 推断并回填表达式类型
    pub(crate) fn infer(&mut self, e: &mut Expr) -> Result<Ty, String> {
        let ty = match &mut e.kind {
            ExprKind::Int(_) => Ty::I64,
            ExprKind::Float(_) => Ty::F64,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Comptime(_) => Ty::I64,
            ExprKind::TupleLit(items) => { let mut ts = Vec::new(); for it in items.iter_mut() { ts.push(self.infer(it)?); } Ty::Tuple(ts) },

            ExprKind::DynBox { trait_name, value } => { self.infer(value)?; Ty::Dyn(trait_name.clone()) },
            ExprKind::MethodOn { recv, method, args } => {
                let rt = self.infer(recv)?;
                for a in args.iter_mut() { self.infer(a)?; }
                // 借用自动解引用：`a := &p; a.方法()` 视作 `p.方法()`。
                let rt = match rt {
                    Ty::Ref(inner) | Ty::RefMut(inner) => *inner,
                    other => other,
                };
                // 查类型的方法返回类型（结构体：fns 里的 类型__方法）
                let ret = match &rt {
                    Ty::Struct(s) => self.fns.get(&format!("{}__{}", s, method)).map(|sig| sig.ret.clone()),
                    Ty::Dyn(tr) => self.trait_methods.get(tr).and_then(|ms| ms.iter().find(|(n, _, _)| n == method).map(|(_, _, r)| r.clone())),
                    Ty::Generic(tp) => self.generic_bounds.iter().find(|(t, _)| t == tp)
                        .and_then(|(_, tr)| self.trait_methods.get(tr))
                        .and_then(|ms| ms.iter().find(|(n, _, _)| n == method).map(|(_, _, r)| r.clone())),
                    _ => None,
                };
                ret.unwrap_or(Ty::Unknown)
            }
            ExprKind::EnumLit(name, variant, args) => {
                // 关联函数 `类型::函数(args)`（不是枚举变体）：查 `类型__函数`
                if !self.enums.contains_key(name) && self.structs.contains_key(name) {
                    let fname = format!("{}__{}", name, variant);
                    if let Some(sig) = self.fns.get(&fname).cloned() {
                        for a in args.iter_mut() { self.infer(a)?; }
                        e.ty = sig.ret.clone();
                        return Ok(sig.ret);
                    }
                }
                let ets: Vec<Ty> = match self.enums.get(name) {
                    Some(vs) => vs.iter().find(|(n, _)| n == variant).map(|(_, ts)| ts.clone()).unwrap_or_default(),
                    None => {
                        // 跨模块：短名 Color 可能对应 模块__Color
                        let suffix = format!("__{}", name);
                        match self.enums.iter().find(|(k, _)| k.ends_with(&suffix)) {
                            Some((_, vs)) => vs.iter().find(|(n, _)| n == variant).map(|(_, ts)| ts.clone()).unwrap_or_default(),
                            None => return Err(crate::lb!(e.line, "undefined enum '{}'", "未定义的枚举 '{}'", name)),
                        }
                    }
                };
                for a in args.iter_mut() { self.infer(a)?; }
                let _ = ets;
                // 若短名在 enums 里没有，但存在 模块__Name，则返回带前缀的全名
                let full = if self.enums.contains_key(name) {
                    name.clone()
                } else {
                    let suffix = format!("__{}", name);
                    self.enums.keys().find(|k| k.ends_with(&suffix)).cloned().unwrap_or_else(|| name.clone())
                };
                Ty::Enum(full)
            }
            ExprKind::ListComp { expr, var, iter, cond } => {
                let it = self.infer(iter)?;
                let elem = match &it {
                    Ty::List(e) | Ty::Array(e, _) | Ty::Set(e) => (**e).clone(),
                    _ => Ty::Unknown,
                };
                self.scopes.push(HashMap::new());
                self.scopes.last_mut().unwrap().insert(var.clone(), VarInfo { ty: elem, mutable: false, explicit: false });
                if let Some(c) = cond { self.infer(c)?; }
                let et = self.infer(expr)?;
                self.scopes.pop();
                Ty::List(Box::new(et))
            }
            ExprKind::CallNamed(_, named) => { for (_, v) in named.iter_mut() { let _ = self.infer(v); } Ty::Unknown }
            ExprKind::Str(_) => Ty::Str,
            ExprKind::Interp(parts) => {
                for p in parts.iter_mut() {
                    if let StrPart::Expr(inner) = p {
                        self.infer(inner)?;
                    }
                }
                Ty::Str
            }
            ExprKind::Ident(n) => match self.lookup(n) {
                Some(t) => t,
                None => {
                    let base = crate::lb!(e.line, "undefined variable '{}'", "未定义的变量 '{}'", n);
                    let mut hint = match closest_name(n, self) {
                        Some(s) => if crate::lang::is_zh() { format!("\x01是否想用 '{}'？", s) } else { format!("\x01did you mean '{}'?", s) },
                        None => String::new(),
                    };
                    // 名字其实是一个函数/内置函数？提示"忘了加 ()"
                    if hint.is_empty() && (self.fns.contains_key(n) || crate::types::is_builtin_name(n)) {
                        hint = if crate::lang::is_zh() { format!("\x01'{}' 是函数，调用它需要加 ()：{}()", n, n) } else { format!("\x01'{}' is a function; call it with () : {}()", n, n) };
                    }
                    return Err(format!("{}{}", base, hint));
                }
            },
            ExprKind::Unary(op, a) => {
                let at = self.infer(a)?;
                let op = *op;
                unary_result(op, &at)
                    .map_err(|why| crate::lb!(e.line, "{}", "{}", why))?
            }
            ExprKind::Binary(op, a, b) => {
                let at = self.infer(a)?;
                let bt = self.infer(b)?;
                let op = *op;
                binary_result(op, &at, &bt)
                    .map_err(|why| crate::lb!(e.line, "{}", "{}", why))?
            }
            ExprKind::Try(inner) => {
                let it = self.infer(inner)?;
                match it {
                    Ty::Result(t, _e) => {
                        // ? 允许在返回 Result 的函数中，或 try 块内（由 try 捕获）
                        if self.try_depth == 0 && !matches!(self.cur_ret, Ty::Result(..)) && self.cur_ret != Ty::Unknown {
                            return Err(crate::lb!(e.line,
                                "'?' can only be used in a function returning Result",
                                "'?' 只能用在返回 Result 的函数中"));
                        }
                        (*t).clone()
                    }
                    Ty::Option(t) => {
                        // ? 用于 Option：None 时提前返回 None，Some 时解包
                        if self.try_depth == 0 && !matches!(self.cur_ret, Ty::Option(_)) && self.cur_ret != Ty::Unknown {
                            return Err(crate::lb!(e.line,
                                "'?' can only be used in a function returning Option",
                                "'?' 只能用在返回 Option(?T) 的函数中"));
                        }
                        (*t).clone()
                    }
                    Ty::Unknown => Ty::Unknown,
                    other => return Err(crate::lb!(e.line,
                        "'?' requires a Result or Option, found {}", "'?' 需要 Result 或 Option，实际是 {}", other)),
                }
            }
            ExprKind::Some(inner) => {
                let it = self.infer(inner)?;
                let t = match &self.cur_ret {
                    Ty::Option(ot) => (**ot).clone(),
                    _ => it,
                };
                Ty::Option(Box::new(t))
            }
            ExprKind::None => {
                let t = match &self.cur_ret {
                    Ty::Option(ot) => (**ot).clone(),
                    _ => Ty::Unknown,
                };
                Ty::Option(Box::new(t))
            }
            ExprKind::TryBlock { body, catches, fin } => {
                self.try_depth += 1;
                check_block(self, body, &mut Vec::new());
                self.try_depth -= 1;
                for ca in catches.iter_mut() {
                    self.scopes.push(std::collections::HashMap::new());
                    if let Some(binding) = &ca.binding {
                        self.lookup_var_mut(binding);
                        self.scopes.last_mut().unwrap().insert(
                            binding.clone(),
                            crate::sema::VarInfo { ty: Ty::Str, mutable: true, explicit: false },
                        );
                    }
                    if let Some(g) = ca.guard.as_mut() { let _ = self.infer(g); }
                    check_block(self, &mut ca.body, &mut Vec::new());
                    self.scopes.pop();
                }
                if let Some(f) = fin { check_block(self, f, &mut Vec::new()); }
                // 值类型取 body 末表达式（简化）
                match body.last_mut() {
                    Some(Stmt::Expr(e)) => self.infer(e)?,
                    _ => Ty::Void,
                }
            }
            ExprKind::Ok(inner) | ExprKind::Err(inner) => {
                let it = self.infer(inner)?;
                let (t, er) = match &self.cur_ret {
                    Ty::Result(rt, re) => ((**rt).clone(), (**re).clone()),
                    _ => (it.clone(), it.clone()),
                };
                Ty::Result(Box::new(t), Box::new(er))
            }
            ExprKind::Call(name, args) => {
                let name = name.clone();
                // web.serve_fn(port, handler)：第二参数是函数名，不做表达式类型检查
                if name == "serve_fn" && args.len() == 2 {
                    let fname = match &args[1].kind {
                        ExprKind::Ident(n) => n.clone(),
                        _ => return Err(crate::lb!(e.line, "serve_fn second arg must be a function name", "serve_fn 第二参数须是函数名")),
                    };
                    if !self.fns.contains_key(&fname) {
                        let keys: Vec<String> = self.fns.keys().cloned().collect();
                        return Err(crate::lb!(e.line, "serve_fn: function '{}' not found; have: {:?}", "serve_fn: 未找到函数 '{}'; 现有: {:?}", fname, keys));
                    }
                    return Ok(Ty::I64);
                }
                // `s.方法(...)`：s 是 dyn Trait 对象 → 查 trait 方法
                if let Some(dot) = name.find('.') {
                    let recv = self.lookup(&name[..dot]);
                    let tr = match recv { Some(Ty::Dyn(tr)) => Some(tr), _ => None };
                    if let Some(tr) = tr {
                        let mname = name[dot+1..].to_string();
                        let ret = self.trait_methods.get(&tr).and_then(|ms| ms.iter().find(|(n, _, _)| *n == mname).map(|(_, _, r)| r.clone()));
                        if let Some(ret) = ret {
                            for a in args.iter_mut().skip(1) { let _ = self.infer(a); }
                            e.ty = ret.clone();
                            return Ok(ret);
                        }
                    }
                }
                // `x.方法(...)`：x 是泛型参数 T 且 T 有 trait 约束 → 查该 trait 的方法
                if let Some(dot) = name.find('.') {
                    let rname = name[..dot].to_string();
                    if let Some(Ty::Generic(tp)) = self.lookup(&rname) {
                        // 遍历 T 的所有约束 trait，找含该方法的
                        let mname = name[dot+1..].to_string();
                        let trs: Vec<String> = self.generic_bounds.iter().filter(|(t, _)| t == &tp).map(|(_, tr)| tr.clone()).collect();
                        for tr in trs {
                            let ret = self.trait_methods.get(&tr).and_then(|ms| ms.iter().find(|(n, _, _)| *n == mname).map(|(_, _, r)| r.clone()));
                            if let Some(ret) = ret {
                                for a in args.iter_mut() { let _ = self.infer(a); }
                                e.ty = ret.clone();
                                return Ok(ret);
                            }
                        }
                    }
                }
                // `obj.方法(...)`：obj 是结构体变量 → 查方法返回类型。
                // 借用自动解引用：obj 是 &T/&mut T 时按 T 查方法。
                if let Some(dot) = name.find('.') {
                    let recv = name[..dot].to_string();
                    let mname = name[dot + 1..].to_string();
                    let rty = self.lookup(&recv).map(|t| match t {
                        Ty::Ref(inner) | Ty::RefMut(inner) => *inner,
                        other => other,
                    });
                    if let Some(Ty::Struct(sty)) = rty {
                        if let Some(sig) = self.fns.get(&format!("{}__{}", sty, mname)).cloned() {
                            for a in args.iter_mut() { let _ = self.infer(a); }
                            e.ty = sig.ret.clone();
                            return Ok(sig.ret);
                        }
                    }
                }
                // `obj.方法(...)`：若 obj 是已知类型的变量，而方法名拼错，给出候选建议。
                if let Some(dot) = name.find('.') {
                    let recv = &name[..dot];
                    let mname = &name[dot + 1..];
                    let rty2 = self.lookup(recv).map(|t| match t {
                        Ty::Ref(inner) | Ty::RefMut(inner) => *inner,
                        other => other,
                    });
                    if let Some(Ty::Struct(sty)) = rty2 {
                        let prefix = format!("{}__", sty);
                        let cands: Vec<String> = self.fns.keys().filter_map(|k| k.strip_prefix(&prefix).map(|s| s.to_string())).collect();
                        if !cands.iter().any(|c| c == mname) {
                            if let Some(sug) = closest_of(mname, cands.into_iter()) {
                                let base = crate::lb!(e.line, "undefined function '{}'", "未定义的函数 '{}'", name);
                                let hint = if crate::lang::is_zh() { format!("\x01是否想用 '{}.{}'？", recv, sug) } else { format!("\x01did you mean '{}.{}'?", recv, sug) };
                                return Err(format!("{}{}", base, hint));
                            }
                        }
                    }
                }
                // 形参当函数调用（高阶函数）：`f(x)` 中 f 是无标注形参 → 放行
                if !name.contains('.') && self.param_names.contains(&name) {
                    for a in args.iter_mut() { let _ = self.infer(a); }
                    e.ty = Ty::Unknown;
                    return Ok(Ty::Unknown);
                }
                let mut arg_tys = Vec::with_capacity(args.len());
                for a in args.iter_mut() {
                    arg_tys.push(self.infer(a)?);
                }
                // push(l, x)：若 l 是"元素未知的 list 变量"，用 x 的类型细化其元素类型。
                // 这让 codegen/jit 能判断"元素是否为堆指针"，从而正确设置 GC 的 elem_ptr。
                if (name == "push" || name == "append" || name == "insert") && args.len() == 2 {
                    if let Some(vty) = arg_tys.get(1).cloned() {
                        match &args[0].kind {
                            ExprKind::Ident(lname) => {
                                if let Some(vi) = self.lookup_var_mut(lname) {
                                    if let Ty::List(e) = &vi.ty {
                                        if matches!(**e, Ty::Unknown) { vi.ty = Ty::List(Box::new(vty.clone())); }
                                    } else if let Ty::Set(e) = &vi.ty {
                                        // set 也要细化：否则 for k in s 取元素时按 Unknown 处理（拿到指针值）。
                                        if matches!(**e, Ty::Unknown) { vi.ty = Ty::Set(Box::new(vty.clone())); }
                                    }
                                }
                            }
                            // 字段：`obj.kids`（obj 是 struct）→ 细化该 struct 字段的元素类型
                            ExprKind::Field(base, fname) => {
                                if let ExprKind::Ident(bn) = &base.kind {
                                    if let Some(Ty::Struct(sname)) = self.lookup(bn) {
                                        let vt = vty.clone();
                                        if let Some(fs) = self.structs.get_mut(&sname) {
                                            for (f, ft) in fs.iter_mut() {
                                                if f == fname {
                                                    if let Ty::List(e) = ft {
                                                        if matches!(**e, Ty::Unknown) { *ft = Ty::List(Box::new(vt.clone())); }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    // 细化后回填 receiver 表达式的类型（供 codegen 用）
                    if let ExprKind::Field(base, fname) = &args[0].kind {
                        if let ExprKind::Ident(bn) = &base.kind {
                            if let Some(Ty::Struct(sname)) = self.lookup(bn) {
                                if let Some(ft) = self.structs.get(&sname).and_then(|fs| fs.iter().find(|(n, _)| n == fname).map(|(_, t)| t.clone())) {
                                    args[0].ty = ft;
                                }
                            }
                        }
                    }
                }
                // Result 构造：Ok(v) / Err(e)
                if name == "Ok" || name == "Err" {
                    if arg_tys.len() != 1 {
                        return Err(crate::lb!(e.line, "{}() takes exactly 1 argument", "{}() 恰好需要 1 个参数", name));
                    }
                    let inner = arg_tys[0].clone();
                    let (t, er) = match &self.cur_ret {
                        Ty::Result(rt, re) => ((**rt).clone(), (**re).clone()),
                        _ => (inner.clone(), inner.clone()),
                    };
                    let ty = Ty::Result(Box::new(t), Box::new(er));
                    e.ty = ty.clone();
                    return Ok(ty);
                }
                // Option 构造：Some(v) / None
                if name == "Some" {
                    if arg_tys.len() != 1 {
                        return Err(crate::lb!(e.line, "Some() takes exactly 1 argument", "Some() 恰好需要 1 个参数"));
                    }
                    let inner = arg_tys[0].clone();
                    let t = match &self.cur_ret {
                        Ty::Option(ot) => (**ot).clone(),
                        _ => inner,
                    };
                    let ty = Ty::Option(Box::new(t));
                    e.ty = ty.clone();
                    return Ok(ty);
                }
                if name == "None" {
                    if !arg_tys.is_empty() {
                        return Err(crate::lb!(e.line, "None takes no arguments", "None 不接受参数"));
                    }
                    let t = match &self.cur_ret {
                        Ty::Option(ot) => (**ot).clone(),
                        _ => Ty::Unknown,
                    };
                    let ty = Ty::Option(Box::new(t));
                    e.ty = ty.clone();
                    return Ok(ty);
                }
                // 用户函数优先于标准库（用户定义同名函数时遮蔽 gtlib）
                if let Some(sig) = self.fns.get(&name).cloned() {
                    if sig.params.len() != arg_tys.len() {
                        return Err(crate::error::msg::arity_mismatch_sig(
                            e.line, &name, sig.params.len(), arg_tys.len(), &sig.params,
                        ).render());
                    }
                    for (i, (want, got)) in sig.params.iter().zip(arg_tys.iter()).enumerate() {
                        if !compatible(want, got) {
                            return Err(crate::error::msg::arg_type_mismatch(
                                e.line, &name, i, want, got,
                            ).render());
                        }
                    }
                    // 形参是 list[dyn T] 而实参是 list[X]：把实参变量细化为形参类型，
                    // 使 push(l, X) 能在 codegen/mono 阶段按 dyn 装箱。
                    for (i, (want, got)) in sig.params.iter().zip(arg_tys.iter()).enumerate() {
                        if let (Ty::List(we), Ty::List(_)) = (want, got) {
                            if matches!(**we, Ty::Dyn(_)) {
                                if i < args.len() {
                                    if let ExprKind::Ident(vn) = &args[i].kind {
                                        let vn = vn.clone();
                                        let want = want.clone();
                                        if let Some(vi) = self.lookup_var_mut(&vn) { vi.ty = want.clone(); }
                                        args[i].ty = want;
                                    }
                                }
                            }
                        }
                    }
                    if sig.ret == Ty::Unknown { Ty::I64 } else { sig.ret }
                } else if let Some(r) = builtin_ret(&name, &arg_tys) {
                    r.map_err(|why| crate::lb!(e.line, "{}", "{}", why))?
                } else if let Some(sf) = crate::types::gtlib_fn(&name) {
                    // 标准库需先 import 对应模块
                    if let Some(dll) = crate::gtlib::dll_of(&name) {
                        if !self.imported_gtlib.iter().any(|m| m == dll) {
                            return Err(crate::lb!(e.line, "module '{}' is not imported", "模块 '{}' 未导入（缺少 import）", dll));
                        }
                    }
                    // 标准库（libGT.dll）：检查元数，参数按需数值提升
                    if sf.params.len() != arg_tys.len() {
                        return Err(crate::error::msg::arity_mismatch(e.line, &name, sf.params.len(), arg_tys.len()).render());
                    }
                    for (want, got) in sf.params.iter().zip(arg_tys.iter()) {
                        if !is_assignable(want, got) {
                            return Err(crate::lb!(e.line, "{}() argument must be {}, found {}", "{}() 参数类型应为 {}，实际是 {}", name, want, got));
                        }
                    }
                    sf.ret
                } else if let Some(vt) = self.lookup(&name) {
                    // 局部变量是闭包：按闭包调用推断
                    match vt {
                        Ty::Closure(_, ret) => (*ret).clone(),
                        _ => return Err(crate::error::msg::undefined_fn(e.line, &name).render()),
                    }
                } else {
                    let sig = match self.fns.get(&name).cloned() {
                        Some(s) => s,
                        None => {
                            let base = crate::error::msg::undefined_fn(e.line, &name).render();
                            // 先看"用户函数"里最近的名字
                            let mut hint = closest_fn(&name, self).map(|s| if crate::lang::is_zh() { format!("\x01是否想用 '{}'？", s) } else { format!("\x01did you mean '{}'?", s) }).unwrap_or_default();
                            // 再看"标准库"里最近的名字（用户可能忘了它来自 stdlib）
                            if hint.is_empty() {
                                let cands: Vec<String> = crate::jit::symbol::BUILTIN_NAMES.iter().chain(crate::jit::symbol::GTLIB_NAMES.iter()).map(|s| s.to_string()).collect();
                                if let Some(s) = closest_of(&name, cands.into_iter()) {
                                    hint = if crate::lang::is_zh() { format!("\x01是否想用标准库函数 '{}'？（该函数由标准库提供，直接调用即可）", s) } else { format!("\x01did you mean the stdlib function '{}'? (it is provided by the standard library; call it directly)", s) };
                                }
                            }
                            return Err(format!("{}{}", base, hint));
                        }
                    };
                    if sig.params.len() != arg_tys.len() {
                        return Err(crate::error::msg::arity_mismatch(
                            e.line, &name, sig.params.len(), arg_tys.len(),
                        ).render());
                    }
                    for (i, (want, got)) in sig.params.iter().zip(arg_tys.iter()).enumerate() {
                        if !compatible(want, got) {
                            return Err(crate::error::msg::arg_type_mismatch(
                                e.line, &name, i, want, got,
                            ).render());
                        }
                    }
                    if sig.ret == Ty::Unknown {
                        // 递归或尚未推断完成：暂按 i64 处理
                        Ty::I64
                    } else {
                        sig.ret
                    }
                }
            }
            ExprKind::Index(base, idx) => {
                let bt = self.infer(base)?;
                let it = self.infer(idx)?;
                match bt {
                    Ty::Array(el, _) => {
                        if it != Ty::Unknown && !it.is_int() {
                            return Err(crate::lb!(e.line, "index must be an integer, found {}", "下标应为整数，实际是 {}", it));
                        }
                        (*el).clone()
                    }
                    Ty::Str => {
                        if it != Ty::Unknown && !it.is_int() {
                            return Err(crate::lb!(e.line, "index must be an integer, found {}", "下标应为整数，实际是 {}", it));
                        }
                        Ty::I64
                    }
                    Ty::List(el) => {
                        if it != Ty::Unknown && !it.is_int() {
                            return Err(crate::lb!(e.line, "list index must be an integer, found {}", "list 下标应为整数，实际是 {}", it));
                        }
                        (*el).clone()
                    }
                    Ty::Map(k, v) => {
                        // map 下标：键须匹配键类型
                        if it != Ty::Unknown && !is_assignable(&k, &it) {
                            return Err(crate::lb!(e.line, "map key must be {}, found {}", "map 键应为 {}，实际是 {}", k, it));
                        }
                        (*v).clone()
                    }
                    Ty::Unknown => Ty::I64,
                    other => {
                        {
                            let base = crate::lb!(e.line, "{} does not support indexing", "{} 不支持下标访问", other);
                            if matches!(other, Ty::I64) {
                                return Err(format!("{}{}", base, if crate::lang::is_zh() { "\x01若它是容器/字符串，请为变量或形参显式标注类型（如 list[int] / str）" } else { "\x01if it is a container/string, annotate the variable or parameter type (e.g. list[int] / str)" }));
                            }
                            return Err(base);
                        }
                    }
                }
            }
            ExprKind::Slice(base, lo, hi) => {
                let bt = self.infer(base)?;
                self.infer(lo)?;
                self.infer(hi)?;
                match bt {
                    Ty::Str => Ty::Str,
                    Ty::List(el) => Ty::List(el),
                    Ty::Array(el, _) => Ty::List(el),
                    Ty::Unknown => Ty::Unknown,
                    other => return Err(crate::lb!(e.line, "{} does not support slicing", "{} 不支持切片", other)),
                }
            }
            ExprKind::ArrayLit(items) => {
                let mut elem: Option<Ty> = None;
                for it in items.iter_mut() {
                    let t = self.infer(it)?;
                    elem = Some(match (&elem, &t) {
                        (None, _) => t,
                        (Some(a), b) => numeric_join(a, b),
                    });
                }
                let elem = elem.unwrap_or(Ty::I64);
                let elem = if elem == Ty::Unknown { Ty::I64 } else { elem };
                Ty::Array(Box::new(elem), items.len())
            }
            ExprKind::If { cond, then, els } => {
                let ct = self.infer(cond)?;
                if ct != Ty::Bool && ct != Ty::Unknown {
                    return Err(crate::lb!(e.line, "condition must be bool, found {}", "条件应为布尔(bool)，实际是 {}", ct));
                }
                let tt = infer_block_ret(self, then)?;
                let et = match els {
                    Some(b) => infer_block_ret(self, b)?,
                    None => Ty::Void,
                };
                if tt == Ty::Void {
                    et
                } else if et == Ty::Void {
                    tt
                } else {
                    numeric_join(&tt, &et)
                }
            }
            ExprKind::Field(base, field) => {
                let bt = self.infer(base)?;
                // 借用自动解引用：`a := &p; a.x` 视作 `p.x`。
                let bt = match bt {
                    Ty::Ref(inner) | Ty::RefMut(inner) => *inner,
                    other => other,
                };
                match bt {
                    Ty::Struct(sname) => {
                        let fty = self
                            .structs
                            .get(&sname)
                            .and_then(|fs| fs.iter().find(|(n, _)| n == field).map(|(_, t)| t.clone()));
                        match fty {
                            Some(t) => t,
                            None => {
                                {
                                    let base = crate::error::msg::no_such_field(e.line, &sname, field).render();
                                    let hint = self.structs.get(&sname).and_then(|fs| closest_of(field, fs.iter().map(|(n, _)| n.clone()))).map(|s| if crate::lang::is_zh() { format!("\x01是否想用 '{}'？", s) } else { format!("\x01did you mean '{}'?", s) }).unwrap_or_default();
                                    return Err(format!("{}{}", base, hint));
                                }
                            }
                        }
                    }
                    Ty::Tuple(ts) => {
                        // 元组下标：`.0`
                        let i: usize = field.parse().unwrap_or(usize::MAX);
                        match ts.get(i) { Some(t) => t.clone(), None => return Err(crate::lb!(e.line, "tuple index {} out of range", "元组下标 {} 越界", field)) }
                    }
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        return Err(crate::lb!(e.line, "{} is not a struct; cannot access field '{}'", "{} 不是结构体，不能访问字段 '{}'", other, field))
                    }
                }
            }
            ExprKind::Match { subject, arms } => {
                let st = self.infer(subject)?;
                let mut result: Option<Ty> = None;
                for arm in arms.iter_mut() {
                    let mut pushed_scope = false;
                    // Result/Option 变体绑定：Ok(v) / Err(e) / Some(v)
                    if let Some(p) = arm.pat.as_ref() {
                        let deep = match_result_binds_deep(p, &st);
                        if !deep.is_empty() {
                            self.scopes.push(HashMap::new());
                            pushed_scope = true;
                            for (bn, bty) in deep {
                                self.scopes.last_mut().unwrap().insert(bn, VarInfo { ty: bty, mutable: false, explicit: false });
                            }
                            if let Some(g) = arm.guard.as_mut() { let _ = self.infer(g); }
                            let bt = infer_block_ret(self, &mut arm.body)?;
                            if pushed_scope { self.scopes.pop(); }
                            result = Some(match (result.take(), &bt) {
                                (None, _) => bt,
                                (Some(prev), Ty::Void) => prev,
                                (Some(prev), _) => if prev == Ty::Void { bt } else { type_join(&prev, &bt) },
                            });
                            continue;
                        }
                    }
                    if let Some((ctor, bind)) = match_result_bind(arm.pat.as_ref()) {
                        let inner = match &st {
                            Ty::Result(t, e) => if ctor == "Ok" { (**t).clone() } else { (**e).clone() },
                            Ty::Option(t) => (**t).clone(),
                            _ => Ty::Unknown,
                        };
                        self.scopes.push(HashMap::new());
                        pushed_scope = true;
                        self.scopes.last_mut().unwrap().insert(bind.clone(), VarInfo { ty: inner, mutable: false, explicit: false });
                    } else if let Some(ExprKind::EnumLit(en, var, binds)) = arm.pat.as_ref().map(|p| &p.kind) {
                        let payload: Vec<Ty> = self.enums.get(en).and_then(|vs| vs.iter().find(|(n, _)| n == var).map(|(_, ts)| ts.clone())).unwrap_or_default();
                        self.scopes.push(HashMap::new());
                        pushed_scope = true;
                        for (i, b) in binds.iter().enumerate() {
                            let ty = payload.get(i).cloned().unwrap_or(Ty::Unknown);
                            if let ExprKind::Ident(bn) = &b.kind {
                                self.scopes.last_mut().unwrap().insert(bn.clone(), VarInfo { ty, mutable: false, explicit: false });
                            } else if let ExprKind::EnumLit(ien, ivar, ibinds) = &b.kind {
                                // 载荷本身是内层 enum 解构（如 E::Has(Shape::Circle(r))）：递归绑定
                                let ipayload: Vec<Ty> = self.enums.get(ien).and_then(|vs| vs.iter().find(|(n, _)| n == ivar).map(|(_, ts)| ts.clone())).unwrap_or_default();
                                for (k, ib) in ibinds.iter().enumerate() {
                                    let ity = ipayload.get(k).cloned().unwrap_or(Ty::Unknown);
                                    if let ExprKind::Ident(bn) = &ib.kind {
                                        self.scopes.last_mut().unwrap().insert(bn.clone(), VarInfo { ty: ity, mutable: false, explicit: false });
                                    } else {
                                        for (bn, bty) in match_result_binds_deep(ib, &ity) {
                                            self.scopes.last_mut().unwrap().insert(bn, VarInfo { ty: bty, mutable: false, explicit: false });
                                        }
                                    }
                                }
                            } else {
                                // 载荷本身是解构模式（如 E::A(Some(v)) / E::A(Ok(v))）：递归绑定
                                for (bn, bty) in match_result_binds_deep(b, &ty) {
                                    self.scopes.last_mut().unwrap().insert(bn, VarInfo { ty: bty, mutable: false, explicit: false });
                                }
                            }
                        }
                    } else if let Some(ExprKind::Ident(bname)) = arm.pat.as_ref().map(|p| &p.kind) {
                        // 裸标识符模式：绑定主体值（如 `match v { n if n > 0 => ... }`）。
                        // 但若该名字已在作用域中（外层变量），保持旧的"比较/守卫"语义。
                        let already = self.lookup(bname).is_some();
                        if already {
                            let pt = self.infer(arm.pat.as_mut().unwrap())?;
                            if pt != Ty::Unknown && st != Ty::Unknown && !compatible(&st, &pt) && !compatible(&pt, &st) {
                                return Err(crate::lb!(arm.line, "match pattern type {} does not match subject {}", "match 模式类型 {} 与主体 {} 不匹配", pt, st));
                            }
                        } else {
                            self.scopes.push(HashMap::new());
                            pushed_scope = true;
                            self.scopes.last_mut().unwrap().insert(bname.clone(), VarInfo { ty: st.clone(), mutable: false, explicit: false });
                            if let Some(p) = arm.pat.as_mut() {
                                p.ty = st.clone();
                            }
                        }
                    } else if let Some(p) = arm.pat.as_mut() {
                        let pt = self.infer(p)?;
                        if pt != Ty::Unknown && st != Ty::Unknown && !compatible(&st, &pt) && !compatible(&pt, &st) {
                            return Err(crate::lb!(arm.line, "match pattern type {} does not match subject {}", "match 模式类型 {} 与主体 {} 不匹配", pt, st));
                        }
                    }
                    if let Some((lo, hi)) = arm.range.as_mut() {
                        let lt = self.infer(lo)?;
                        let ht = self.infer(hi)?;
                        if lt != Ty::Unknown && !lt.is_int() { return Err(crate::lb!(arm.line, "range start must be integer", "范围起点应为整数")); }
                        if ht != Ty::Unknown && !ht.is_int() { return Err(crate::lb!(arm.line, "range end must be integer", "范围终点应为整数")); }
                    }
                    if let Some(g) = arm.guard.as_mut() {
                        let gt = self.infer(g)?;
                        if gt != Ty::Bool && gt != Ty::Unknown {
                            return Err(crate::lb!(arm.line, "match guard must be bool", "match 守卫应为布尔"));
                        }
                    }
                    let bt = infer_block_ret(self, &mut arm.body)?;
                    // 解构/绑定模式推入的 scope 需弹出
                    if pushed_scope {
                        self.scopes.pop();
                    }
                    result = Some(match (result.take(), &bt) {
                        (None, _) => bt,
                        (Some(prev), Ty::Void) => prev,
                        (Some(prev), _) => {
                            if prev == Ty::Void {
                                bt
                            } else {
                                type_join(&prev, &bt)
                            }
                        }
                    });
                }
                // 穷尽性检查（仅当有明确类型且无 _ 通配时）
                if let Some(msg) = check_match_exhaustive(&st, arms, self) {
                    return Err(crate::lb!(e.line, "{}", "{}", msg));
                }
                result.unwrap_or(Ty::Void)
            }
            ExprKind::Borrow { mutable, inner } => {
                let it = self.infer(inner)?;
                if *mutable { Ty::RefMut(Box::new(it)) } else { Ty::Ref(Box::new(it)) }
            }
            ExprKind::ClosureNew { fn_name, captures } => {
                let mut cap_tys: Vec<Ty> = Vec::new();
                for c in captures.iter_mut() {
                    cap_tys.push(self.infer(c)?);
                }
                // 回填捕获参数类型（供 __closure_N 的形参推断使用）
                self.capture_types.insert(fn_name.clone(), cap_tys);
                // 闭包类型取自提升后的函数签名（captures + params）。
                // 注意：params 必须包含捕获值——后端靠 len(params) - len(args) 推算捕获个数。
                let (params, ret) = match self.fns.get(fn_name) {
                    Some(sig) => (sig.params.clone(), sig.ret.clone()),
                    None => (vec![Ty::I64; captures.len()], Ty::I64),
                };
                Ty::Closure(params, Box::new(if ret == Ty::Void { Ty::I64 } else { ret }))
            }
            ExprKind::CallValue { callee, args } => {
                // 形参当闭包调用：`f(x)` 中 f 是无标注形参 → 返回类型未知（放行）
                if let ExprKind::Ident(n) = &callee.kind {
                    if self.param_names.contains(n) {
                        for a in args.iter_mut() { let _ = self.infer(a); }
                        e.ty = Ty::Unknown;
                        return Ok(Ty::Unknown);
                    }
                }
                let ct = self.infer(callee)?;
                for a in args.iter_mut() {
                    self.infer(a)?;
                }
                match ct {
                    Ty::Closure(_, ret) => (*ret).clone(),
                    Ty::Unknown => Ty::I64,
                    other => {
                        return Err(crate::lb!(e.line, "{} is not callable", "{} 不是可调用的闭包", other))
                    }
                }
            }
            ExprKind::Closure { params, param_tys, ret_ty, body, line } => {
                // 闭包：参数类型可用标注覆盖，未标注则从 body 推断
                self.scopes.push(HashMap::new());
                for (i, p) in params.iter().enumerate() {
                    let ty = param_tys.get(i).cloned().flatten().unwrap_or(Ty::Unknown);
                    self.scopes.last_mut().unwrap().insert(p.clone(), VarInfo { ty, mutable: true, explicit: false });
                }
                let bt = self.infer(body)?;
                self.scopes.pop();
                let ptypes: Vec<Ty> = params.iter().enumerate().map(|(i, _)| param_tys.get(i).cloned().flatten().unwrap_or(Ty::Unknown)).collect();
                let _ = line;
                let rt = ret_ty.clone().unwrap_or(if bt == Ty::Void { Ty::I64 } else { bt });
                Ty::Closure(ptypes, Box::new(rt))
            }
            ExprKind::StructLit(name, fields) => {
                let declared = match self.structs.get(name) {
                    Some(fs) => fs.clone(),
                    None => {
                        let base = crate::error::msg::undefined_type(e.line, name).render();
                        let mut cands: Vec<String> = self.structs.keys().cloned().collect();
                        cands.extend(self.enums.keys().cloned());
                        cands.extend(self.trait_methods.keys().cloned());
                        let hint = closest_of(name, cands.into_iter()).map(|s| if crate::lang::is_zh() { format!("\x01是否想用 '{}'？", s) } else { format!("\x01did you mean '{}'?", s) }).unwrap_or_default();
                        return Err(format!("{}{}", base, hint));
                    }
                };
                // 每个提供的字段：类型必须兼容
                for (fname, v) in fields.iter_mut() {
                    let vt = self.infer(v)?;
                    match declared.iter().find(|(n, _)| n == fname) {
                        None => {
                            {
                                let base = crate::error::msg::no_such_field(e.line, name, fname).render();
                                let hint = declared.iter().find_map(|_| None).or_else(|| closest_of(fname, declared.iter().map(|(n, _)| n.clone()))).map(|s| if crate::lang::is_zh() { format!("\x01是否想用 '{}'？", s) } else { format!("\x01did you mean '{}'?", s) }).unwrap_or_default();
                                return Err(format!("{}{}", base, hint));
                            }
                        }
                        Some((_, want)) => {
                            if !compatible(want, &vt) {
                                return Err(crate::lb!(e.line, "field '{}.{}' must be {}, found {}", "字段 '{}.{}' 应为 {}，实际是 {}", name, fname, want, vt));
                            }
                        }
                    }
                }
                // 新增：检查是否遗漏了必填字段
                let missing: Vec<&str> = declared
                    .iter()
                    .filter(|(n, _)| !fields.iter().any(|(f, _)| f == n))
                    .map(|(n, _)| n.as_str())
                    .collect();
                if !missing.is_empty() {
                    return Err(crate::error::CompileError::new(
                        crate::error::ErrorCode::BadStructLit,
                        e.line,
                        format!("结构体 '{}' 缺少字段：{}", name, missing.join("、")),
                    ).with_hint("补齐所有字段，或用默认值").render());
                }
                Ty::Struct(name.clone())
            }
        };
        e.ty = ty.clone();
        Ok(ty)
    }
}
