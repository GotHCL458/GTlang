//! Parser 的表达式解析。

use super::*;

impl Parser {
    // ---------- 表达式 ----------

    pub(crate) fn expr(&mut self, min_prec: u8) -> Result<Expr, String> {
        self.enter_depth()?;
        let r = self.expr_inner(min_prec);
        self.leave_depth();
        r
    }

    fn expr_inner(&mut self, min_prec: u8) -> Result<Expr, String> {
        let mut lhs = self.unary()?;
        loop {
            // 三元：`a if cond else b`（Python 风格，最低优先级）
            // 注意：只有当 `if ... else ...` 确实构成三元时才消费；否则
            // （如 `a := 1  if cond { ... }` —— if 是下一条语句）回退，交给语句层。
            if self.at_ident("if") && min_prec == 0 && !self.cur().nl_before {
                let save_pos = self.pos;
                let line = self.line();
                self.bump();
                let saved = self.no_struct_lit;
                self.no_struct_lit = true;
                let cond = self.expr(0);
                self.no_struct_lit = saved;
                let cond = cond?;
                if !self.eat_ident("else") {
                    // 不是三元（缺 else）：回退到 if 之前，让语句层把 if 当语句处理。
                    self.pos = save_pos;
                    break;
                }
                let els = self.expr(0)?;
                // `a if c else b` → if c { a } else { b }
                lhs = Expr::new(ExprKind::If {
                    cond: Box::new(cond),
                    then: vec![Stmt::Expr(lhs)],
                    els: Some(vec![Stmt::Expr(els)]),
                }, line);
                continue;
            }
            // `or` 语法已移除
            // 空合并：`a ?? b` → `match a { Some(v) => v, None => b }`（Option）
            if self.at_punct("??") && min_prec == 0 {
                let line = self.line();
                self.bump();
                let fallback = self.expr(1)?;
                let v = format!("__qq_v{}", line);
                let vexpr = || Expr::new(ExprKind::Ident(v.clone()), line);
                let some_pat = Expr::new(ExprKind::Call("Some".to_string(), vec![vexpr()]), line);
                let arms = vec![
                    MatchArm { pat: Some(some_pat), range: None, guard: None,
                        body: vec![Stmt::Expr(vexpr())], line },
                    MatchArm { pat: None, range: None, guard: None,
                        body: vec![Stmt::Expr(fallback)], line },
                ];
                lhs = Expr::new(ExprKind::Match { subject: Box::new(lhs), arms }, line);
                continue;
            }
            // 管道：`x |> f` → `f(x)`（最低优先级，左结合）
            if self.at_punct("|>") && min_prec == 0 {
                let line = self.line();
                self.bump();
                // 右值应为可调用表达式：`f` / `f(a)` / `obj.m` / 带参数列表的表达式。
                // 不在此展开：把 lhs 作为首参插入，并把"右侧表达式"整体作为 callee（值调用），
                // 这样 `i |> i + 1` 会变成 `(i + 1)(i)`，由类型检查报"不可调用"，
                // 而不是静默丢掉 lhs（旧实现会产出 `i + 1`，造成无声的错值）。
                let rhs = self.expr(1)?;
                let call = match rhs.kind {
                    ExprKind::Call(name, mut args) => { let mut v = vec![lhs]; v.append(&mut args); Expr::new(ExprKind::Call(name, v), line) }
                    ExprKind::Ident(name) => Expr::new(ExprKind::Call(name, vec![lhs]), line),
                    // 其他表达式：作为 callee 做值调用，交由 sema 判定可调用性
                    other => Expr::new(ExprKind::CallValue { callee: Box::new(Expr::new(other, line)), args: vec![lhs] }, line),
                };
                lhs = call;
                continue;
            }
            // 成员测试：`k in container`（低优先级，等价 has(container, k)）
            if self.at_ident("in") && min_prec <= 3 {
                let line = self.line();
                self.bump();
                let rhs = self.expr(4)?;
                lhs = Expr::new(ExprKind::Call("has".to_string(), vec![rhs, lhs]), line);
                continue;
            }
            let op = match &self.cur().tok {
                Tok::Punct(p) => match BinOp::from_str(p) {
                    Some(o) => o,
                    None => break,
                },
                _ => break,
            };
            let prec = prec_of(op);
            if prec < min_prec {
                break;
            }
            // 换行歧义：可作一元前缀的运算符、以及 `//`（行注释起始）若另起一行，
            // 视为新语句/新注释，不接上一行。这样
            //   `x := a // 5`（同一行）→ 整除
            //   `// 说明`（下一行行首）→ 注释，不会被上一行表达式吞掉
            let sp = match &self.cur().tok {
                Tok::Punct(p) => p.as_str(),
                _ => "",
            };
            if self.cur().nl_before && matches!(sp, "+" | "-" | "*" | "&" | "!" | "~" | "//") {
                break;
            }
            // 长二元链（a+b+c+...）构造"左深"AST；限制长度以防 Drop 递归爆栈
            self.enter_depth()?;
            let line = self.line();
            self.bump();
            let rhs = self.expr(prec + 1)?;
            lhs = Expr::new(ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), line);
        }
        Ok(lhs)
    }

    pub(crate) fn unary(&mut self) -> Result<Expr, String> {

        let line = self.line();
        // 借用表达式：`&x` / `&mut x`（`&&` 是逻辑与，不在此处理）
        if self.at_punct("&") && !(matches!(&self.peek_at(1).tok, Tok::Punct(p) if p == "&")) {
            self.bump();
            let mutable = self.eat_ident("mut");
            let e = self.unary()?;
            return Ok(Expr::new(ExprKind::Borrow { mutable, inner: Box::new(e) }, line));
        }
        if self.at_punct("-") {
            self.bump();
            // `-9223372036854775808`（i64::MIN）：直接返回，避免"取负溢出"
            if let Tok::Int(v) = self.cur().tok {
                if v == i64::MIN {
                    self.bump();
                    return Ok(Expr::new(ExprKind::Int(i64::MIN), line));
                }
            }
            let e = self.unary()?;
            return Ok(Expr::new(ExprKind::Unary(UnOp::Neg, Box::new(e)), line));
        }
        if self.at_punct("!") {
            self.bump();
            let e = self.unary()?;
            return Ok(Expr::new(ExprKind::Unary(UnOp::Not, Box::new(e)), line));
        }
        if self.at_punct("~") {
            self.bump();
            let e = self.unary()?;
            return Ok(Expr::new(ExprKind::Unary(UnOp::BitNot, Box::new(e)), line));
        }
        let e = self.primary()?;
        self.postfix(e)
    }

    pub(crate) fn postfix(&mut self, mut e: Expr) -> Result<Expr, String> {
        loop {
            let line = self.line();
            // Result 传播：`expr?`
            if self.at_punct("?") {
                self.bump();
                e = Expr::new(ExprKind::Try(Box::new(e)), line);
                continue;
            }
            // 字段访问：`expr.field`（但不要吞掉限定名 `mod.fn`，那已在 primary 折叠）
            // 可选链：`a?.b` → `match a { Some(v) => Some(v.b), None => None }`
            if self.at_punct("?.") {
                let line = self.line();
                self.bump();
                let field = match &self.cur().tok {
                    Tok::Ident(n) => { let s = n.clone(); self.bump(); s }
                    _ => return Err(crate::lb!(self.line(), "'?.' needs a field name", "'?.' 后需要字段名")),
                };
                let v = format!("__oc_v{}", line);
                let vexpr = || Expr::new(ExprKind::Ident(v.clone()), line);
                let field_expr = Expr::new(ExprKind::Field(Box::new(vexpr()), field), line);
                let some_pat = Expr::new(ExprKind::Call("Some".to_string(), vec![vexpr()]), line);
                let some_body = Expr::new(ExprKind::Call("Some".to_string(), vec![field_expr]), line);
                let arms = vec![
                    MatchArm { pat: Some(some_pat), range: None, guard: None,
                        body: vec![Stmt::Expr(some_body)], line },
                    MatchArm { pat: None, range: None, guard: None,
                        body: vec![Stmt::Expr(Expr::new(ExprKind::None, line))], line },
                ];
                e = Expr::new(ExprKind::Match { subject: Box::new(e), arms }, line);
                continue;
            }
            if self.at_punct(".") {
                self.bump();
                let f = match &self.cur().tok {
                    Tok::Int(n) if *n >= 0 => { let s = n.to_string(); self.bump(); s }
                    _ => self.ident("字段名")?,
                };
                e = Expr::new(ExprKind::Field(Box::new(e), f), line);
                continue;
            }
            if self.at_punct("[") {
                self.bump();
                let saved = self.no_struct_lit;
                self.no_struct_lit = false;
                let idx = self.expr(0);
                self.no_struct_lit = saved;
                let idx = idx?;
                // 切片 `s[lo..hi]`
                if self.eat_punct("..") {
                    let hi = self.expr(0)?;
                    self.expect_punct("]")?;
                    e = Expr::new(ExprKind::Slice(Box::new(e), Box::new(idx), Box::new(hi)), line);
                    continue;
                }
                self.expect_punct("]")?;
                e = Expr::new(ExprKind::Index(Box::new(e), Box::new(idx)), line);
                continue;
            }
            if self.at_punct("(") {
                // 支持 `f(...)`（Ident）与 `类型.方法(...)` / `obj.方法(...)`（Field）
                // 折叠纯点分链（Ident/Field 逐层）为全名：a(...)、a.b(...)、boot.fs.write(...)
                fn dotted_name(e: &Expr) -> Option<String> {
                    match &e.kind {
                        ExprKind::Ident(n) => Some(n.clone()),
                        ExprKind::Field(base, f) => dotted_name(base).map(|h| format!("{}.{}", h, f)),
                        _ => None,
                    }
                }
                let callee = dotted_name(&e);
                // 方法链 `expr.方法(...)`（expr 是任意表达式，如另一个调用结果）。
                // 但纯点分链（base 也是 Ident/Field）已在 callee 里折叠成全名，不算方法链。
                let chain: Option<(Box<Expr>, String)> = match &e.kind {
                    ExprKind::Field(base, f) if dotted_name(base).is_none() => Some((base.clone(), f.clone())),
                    _ => None,
                };
                // 任意表达式作 callee（如 `fs[0](5)`、`(工厂())(x)`）→ 间接调用 CallValue
                let indirect = callee.is_none() && chain.is_none();
                if callee.is_some() || chain.is_some() || indirect {
                    self.bump();
                    let saved = self.no_struct_lit;
                    self.no_struct_lit = false;
                    let mut args = Vec::new();
                    let mut named: Vec<(String, Expr)> = Vec::new();
                    while !self.at_punct(")") {
                        // 命名参数 `name: expr`（后随 `,` 或 `)`）
                        let is_named = matches!(&self.cur().tok, Tok::Ident(_))
                            && matches!(&self.peek_at(1).tok, Tok::Punct(p) if p == ":")
                            && !matches!(&self.peek_at(2).tok, Tok::Punct(p) if p == ":");
                        if is_named {
                            let n = if let Tok::Ident(n) = &self.cur().tok { n.clone() } else { unreachable!() };
                            self.bump(); // name
                            self.bump(); // :
                            let v = self.expr(0)?;
                            named.push((n, v));
                        } else {
                            let a = self.expr(0);
                            match a {
                                Ok(v) => args.push(v),
                                Err(e) => {
                                    self.no_struct_lit = saved;
                                    return Err(e);
                                }
                            }
                        }
                        if !self.eat_punct(",") {
                            break;
                        }
                    }
                    self.no_struct_lit = saved;
                    self.expect_punct(")")?;
                    if let Some((recv, method)) = chain {
                        // 方法链：`expr.方法(args)` → MethodOn（mono 降级为 类型__方法(recv, ...)）
                        e = Expr::new(ExprKind::MethodOn { recv, method, args }, line);
                    } else if indirect {
                        // 间接调用：callee 是任意表达式（索引/调用结果/括号表达式…）
                        let callee = std::mem::replace(&mut e, Expr::new(ExprKind::None, line));
                        e = Expr::new(ExprKind::CallValue { callee: Box::new(callee), args }, line);
                    } else {
                        let name = callee.unwrap();
                        if named.is_empty() {
                            e = Expr::new(ExprKind::Call(name, args), line);
                        } else {
                            // 混用位置 + 命名：位置参数放前
                            let mut all: Vec<(String, Expr)> = Vec::new();
                            for a in args { all.push((String::new(), a)); }
                            all.extend(named);
                            e = Expr::new(ExprKind::CallNamed(name, all), line);
                        }
                    }
                    continue;
                }
                break;
            }
            break;
        }
        Ok(e)
    }

    /// `match subject { pat => block, ..., _ => block }`
    pub(crate) fn match_expr(&mut self, line: usize) -> Result<Expr, String> {
        self.eat_ident("match");
        let saved = self.no_struct_lit;
        self.no_struct_lit = true;
        let subject = self.expr(0);
        self.no_struct_lit = saved;
        let subject = subject?;
        self.expect_punct("{")?;
        let mut arms = Vec::new();
        loop {
            self.skip_line_comment();
            if self.at_punct("}") {
                break;
            }
            if self.at_eof() {
                return Err("match 缺少 '}'".into());
            }
            let aline = self.line();
            // 模式：`_` 通配，否则一个字面量/表达式
            let pat = if self.at_ident("_") {
                self.bump();
                None
            } else {
                // 用 min_prec=1 解析模式：屏蔽三元 `a if c else b` 与 `or`/`|>`，
                // 否则守卫 `pat if cond =>` 会被当成缺 else 的三元。
                Some(self.expr(1)?)
            };
            // 范围模式 `lo..hi`
            let range = if self.eat_punct("..") {
                let hi = self.expr(0)?;
                let lo = pat.clone().ok_or_else(|| "range pattern needs a start".to_string())?;
                Some((lo, hi))
            } else {
                None
            };
            let pat = if range.is_some() { None } else { pat };
            // 守卫 `if cond`
            let guard = if self.eat_ident("if") {
                Some(self.expr(0)?)
            } else {
                None
            };
            if !self.eat_punct("=>") {
                return Err(crate::lb!(self.line(), "match arm is missing '=>'", "match 分支缺少 '=>'"));
            }
            let body = self.block()?;
            // OR 模式 a | b | c 拆成多个同 body 分支
            if let Some(p) = &pat {
                let parts = split_or_pattern(p);
                if parts.len() > 1 {
                    for part in parts {
                        arms.push(MatchArm { pat: Some(part), range: None, guard: guard.clone(), body: body.clone(), line: aline });
                    }
                    self.eat_punct(",");
                    continue;
                }
            }
            arms.push(MatchArm { pat, range, guard, body, line: aline });
            self.eat_punct(",");
        }
        self.expect_punct("}")?;
        Ok(Expr::new(ExprKind::Match { subject: Box::new(subject), arms }, line))
    }

    /// 闭包字面量：`|x, y| expr` 或 `|| expr`
    pub(crate) fn closure_expr(&mut self, line: usize) -> Result<Expr, String> {
        self.expect_punct("|")?;
        let mut params = Vec::new();
        let mut param_tys: Vec<Option<Ty>> = Vec::new();
        while !self.at_punct("|") {
            params.push(self.ident("闭包参数")?);
            param_tys.push(if self.eat_punct(":") { Some(self.parse_type()?) } else { None });
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct("|")?;
        let ret_ty = if self.eat_punct("->") { Some(self.parse_type()?) } else { None };
        // body：单个表达式或块
        let body = if self.at_punct("{") {
            // 块 → 取块尾表达式为闭包值
            let blk = self.block()?;
            Expr::new(ExprKind::If {
                cond: Box::new(Expr::new(ExprKind::Bool(true), line)),
                then: blk,
                els: None,
            }, line)
        } else {
            self.expr(0)?
        };
        Ok(Expr::new(ExprKind::Closure { params, param_tys, ret_ty, body: Box::new(body), line }, line))
    }

    /// 结构体字面量 `Name { field: expr, ... }`
    pub(crate) fn struct_lit(&mut self, name: String, line: usize) -> Result<Expr, String> {
        self.expect_punct("{")?;
        let mut fields = Vec::new();
        loop {
            self.skip_line_comment();
            if self.at_punct("}") {
                break;
            }
            if self.at_eof() {
                return Err("结构体字面量缺少 '}'".into());
            }
            let fname = self.ident("字段名")?;
            self.expect_punct(":")?;
            let v = self.expr(0)?;
            fields.push((fname, v));
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct("}")?;
        Ok(Expr::new(ExprKind::StructLit(name, fields), line))
    }

    pub(crate) fn primary(&mut self) -> Result<Expr, String> {
        let line = self.line();
        // 闭包字面量 `|params| body` 或 `|| body`
        if self.at_punct("|") {
            return self.closure_expr(line);
        }
        match self.cur().tok.clone() {
            Tok::Int(v) => {
                self.bump();
                Ok(Expr::new(ExprKind::Int(v), line))
            }
            Tok::Float(v) => {
                self.bump();
                Ok(Expr::new(ExprKind::Float(v), line))
            }
            Tok::Str(raw) => {
                self.bump();
                Ok(Expr::new(interp(&raw, line)?, line))
            }
            Tok::RawStr(s) => {
                // 原始字符串：不解转义、不插值
                self.bump();
                Ok(Expr::new(ExprKind::Str(s), line))
            }
            Tok::Ident(name) if name == "dyn" => {
                // `dyn Trait(v)`：装箱为 trait 对象
                self.bump();
                let tname = self.ident("trait 名")?;
                self.expect_punct("(")?;
                let v = self.expr(0)?;
                self.expect_punct(")")?;
                Ok(Expr::new(ExprKind::DynBox { trait_name: tname, value: Box::new(v) }, line))
            }
            Tok::Ident(name) => {
                if name == "true" || name == "false" {
                    self.bump();
                    return Ok(Expr::new(ExprKind::Bool(name == "true"), line));
                }
                if name == "None" {
                    self.bump();
                    return Ok(Expr::new(ExprKind::None, line));
                }
                if name == "try" {
                    return self.try_expr(line);
                }
                if name == "if" {
                    let (cond, then, els) = self.if_parts()?;
                    return Ok(Expr::new(
                        ExprKind::If { cond: Box::new(cond), then, els },
                        line,
                    ));
                }
                if name == "match" {
                    return self.match_expr(line);
                }
                if name == "comptime" && matches!(&self.peek_at(1).tok, Tok::Punct(p) if p == "{") {
                    self.bump(); // comptime
                    let body = self.block()?;
                    return Ok(Expr::new(ExprKind::Comptime(body), line));
                }
                if name == "do" && matches!(&self.peek_at(1).tok, Tok::Punct(p) if p == "{") {
                    self.bump(); // do
                    let body = self.block()?;
                    return Ok(Expr::new(ExprKind::If {
                        cond: Box::new(Expr::new(ExprKind::Bool(true), line)),
                        then: body,
                        els: None,
                    }, line));
                }
                if is_keyword(&name) {
                    return Err(crate::lb!(line, "keyword '{}' cannot be used as an expression", "关键字 '{}' 不能作为表达式", name));
                }
                // 箭头函数语法糖：`x => expr`（等价 `|x| expr`）
                if matches!(&self.peek_at(1).tok, Tok::Punct(p) if p == "=>") {
                    self.bump(); // x
                    self.bump(); // =>
                    let body = self.expr(0)?;
                    return Ok(Expr::new(ExprKind::Closure {
                        params: vec![name],
                        param_tys: vec![None],
                        ret_ty: None,
                        body: Box::new(body),
                        line,
                    }, line));
                }
                self.bump();
                // 结构体字面量：`Name { field: v, ... }`
                // 在 for/if/while 的条件位置禁用（那里的 `{` 是块）
                if self.at_punct("{") && !self.no_struct_lit {
                    return self.struct_lit(name, line);
                }
                // 限定结构体字面量：`模块.类型 { ... }`（点分名字 + 大括号）
                if self.at_punct(".") && !self.no_struct_lit {
                    // 探测 `. 名字 {`
                    if let Tok::Ident(_) = &self.peek_at(1).tok {
                        if matches!(&self.peek_at(2).tok, Tok::Punct(p) if p == "{") {
                            self.bump(); // .
                            let seg = self.ident("类型名")?;
                            let full = format!("{}.{}", name, seg);
                            return self.struct_lit(full, line);
                        }
                    }
                }
                // 枚举构造：`Name::Variant(args)` / `Name::Variant`
                if self.at_punct("::") {
                    if let Tok::Ident(seg) = &self.peek_at(1).tok {
                        let seg = seg.clone();
                        // Name::Variant（可带括号载荷）→ EnumLit
                        {
                            self.bump(); // ::
                            self.bump(); // Variant
                            let mut args = Vec::new();
                            if self.eat_punct("(") {
                                while !self.at_punct(")") {
                                    args.push(self.expr(0)?);
                                    if !self.eat_punct(",") { break; }
                                }
                                self.expect_punct(")")?;
                            }
                            return Ok(Expr::new(ExprKind::EnumLit(name, seg, args), line));
                        }
                    }
                }
                // 限定名：`a.b.c` / `a::b::c` 折叠成点分名字 "a.b.c"
                let mut full = name;
                while self.at_punct("::") {
                    self.bump();
                    let seg = self.ident("限定名段")?;
                    full.push('.');
                    full.push_str(&seg);
                }
                Ok(Expr::new(ExprKind::Ident(full), line))
            }
            Tok::Punct(ref p) if p == "(" => {
                self.bump();
                let saved = self.no_struct_lit;
                self.no_struct_lit = false;
                let first = self.expr(0);
                self.no_struct_lit = saved;
                let first = first?;
                if self.at_punct(",") {
                    let mut items = vec![first];
                    while self.eat_punct(",") {
                        if self.at_punct(")") { break; }
                        items.push(self.expr(0)?);
                    }
                    self.expect_punct(")")?;
                    return Ok(Expr::new(ExprKind::TupleLit(items), line));
                }
                self.expect_punct(")")?;
                Ok(first)
            }
            Tok::Punct(ref p) if p == "[" => {
                self.bump();
                // 列表推导：`[expr for v in iter (if cond)?]`
                let first = self.expr(0)?;
                if self.at_ident("for") {
                    self.bump();
                    let var = self.ident("推导变量")?;
                    if !self.eat_ident("in") {
                        return Err(crate::lb!(self.line(), "list comprehension expects 'in'", "列表推导缺少 'in'"));
                    }
                    let iter = self.expr(1)?;
                    let cond = if self.eat_ident("if") { Some(Box::new(self.expr(0)?)) } else { None };
                    self.expect_punct("]")?;
                    return Ok(Expr::new(ExprKind::ListComp { expr: Box::new(first), var, iter: Box::new(iter), cond }, line));
                }
                let mut items = vec![first];
                while self.eat_punct(",") {
                    if self.at_punct("]") { break; }
                    items.push(self.expr(0)?);
                }
                self.expect_punct("]")?;
                Ok(Expr::new(ExprKind::ArrayLit(items), line))
            }
            _ => Err(crate::lb!(line, "expected an expression, found {}", "期望表达式，实际是 {}", self.describe())),
        }
    }
}

