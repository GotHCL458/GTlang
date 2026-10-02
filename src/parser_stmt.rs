//! Parser 的语句解析。

use super::*;

impl Parser {

    // ---------- 语句 ----------

    /// 解析 for 循环后可选 `else { ... }`（无 break 正常结束时执行）。
    pub(crate) fn parse_for_else(&mut self) -> Result<Option<Block>, String> {
        if self.at_ident("else") {
            self.bump();
            Ok(Some(self.block()?))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn block(&mut self) -> Result<Block, String> {
        self.expect_punct("{")?;
        let mut b = Vec::new();
        loop {
            self.skip_line_comment();
            if self.at_punct("}") {
                break;
            }
            if self.at_eof() {
                return Err("文件意外结束：块缺少 '}'".into());
            }
            b.push(self.stmt()?);
        }
        self.expect_punct("}")?;
        Ok(b)
    }

    /// 解析 `try { body } expt ... fily { fin }` 的公共部分（调用前已在 `try` 上）。
    pub(crate) fn try_parts(&mut self) -> Result<(Block, Vec<CatchArm>, Option<Block>), String> {
        self.bump(); // try
        let body = self.block()?;
        let mut catches: Vec<CatchArm> = Vec::new();
        loop {
            if self.at_ident("expt") || self.at_ident("except") {
                let cline = self.line();
                self.bump();
                let mut binding = None;
                if matches!(&self.cur().tok, Tok::Ident(_)) && !self.at_punct("{") {
                    binding = Some(self.ident("错误绑定")?);
                }
                let guard = if self.at_ident("if") {
                    self.bump();
                    Some(self.expr(0)?)
                } else {
                    None
                };
                let cbody = self.block()?;
                catches.push(CatchArm { binding, label: None, guard, body: cbody, line: cline });
            } else {
                break;
            }
        }
        let fin = if self.at_ident("fily") || self.at_ident("finally") {
            self.bump();
            Some(self.block()?)
        } else {
            None
        };
        Ok((body, catches, fin))
    }

    /// `try { ... } ...` 作为表达式（块值）。
    pub(crate) fn try_expr(&mut self, line: usize) -> Result<Expr, String> {
        let (body, catches, fin) = self.try_parts()?;
        Ok(Expr::new(ExprKind::TryBlock { body, catches, fin }, line))
    }

    pub(crate) fn stmt(&mut self) -> Result<Stmt, String> {
        let line = self.line();

        if self.at_ident("let") {
            self.bump();
            let mutable = self.eat_ident("mut");
            if self.at_punct("(") {
                self.bump();
                let mut names = Vec::new();
                while !self.at_punct(")") {
                    names.push(self.ident("解构变量")?);
                    if !self.eat_punct(",") { break; }
                }
                self.expect_punct(")")?;
                self.expect_punct("=")?;
                let value = self.expr(0)?;
                return Ok(desugar_destructure(names, value, mutable, line));
            }
            let name = self.ident("变量名")?;
            let ty = if self.eat_punct(":") { Some(self.parse_type()?) } else { None };
            self.expect_punct("=")?;
            let value = self.expr(0)?;
            return Ok(Stmt::Let { name, ty, value, mutable, line });
        }

        // `go f(args)`：新线程调用函数
        if self.at_ident("go") {
            self.bump();
            let func = self.ident("函数名")?;
            self.expect_punct("(")?;
            let mut args = Vec::new();
            while !self.at_punct(")") {
                args.push(self.expr(0)?);
                if !self.eat_punct(",") { break; }
            }
            self.expect_punct(")")?;
            return Ok(Stmt::Go { func, args, line });
        }

        if self.at_ident("if") {
            let (cond, then, els) = self.if_parts()?;
            return Ok(Stmt::If { cond, then, els, line });
        }

        // 函数内 const 声明
        if self.at_ident("const") {
            self.bump();
            let name = self.ident("const 名称")?;
            let ty = if self.eat_punct(":") { Some(self.parse_type()?) } else { None };
            self.expect_punct("=")?;
            let value = self.expr(0)?;
            return Ok(Stmt::Const { name, ty, value, line });
        }

        // 嵌套函数：fn 声明在语句位置
        if self.at_ident("fn") {
            self.bump();
            let f = self.fn_def(false)?;
            return Ok(Stmt::LocalFn(f));
        }

        // 内联汇编已移除
        if self.at_ident("asm") {
            return Err(crate::lb!(self.line(), "inline asm has been removed", "内联汇编已移除"));
        }

        // `throw e` / `raise e`
        if self.at_ident("throw") || self.at_ident("raise") {
            self.bump();
            let e = self.expr(0)?;
            return Ok(Stmt::Throw(e, line));
        }

        // `try { ... } expt ... fily { ... }`
        if self.at_ident("try") {
            let (body, catches, fin) = self.try_parts()?;
            return Ok(Stmt::Try { body, catches, fin, line });
        }

        // `do { body } while cond`
        if self.at_ident("do") {
            self.bump();
            let body = self.block()?;
            if !self.eat_ident("while") {
                return Err(crate::lb!(self.line(), "do block must be followed by 'while'", "do 块后必须跟 while"));
            }
            let saved = self.no_struct_lit;
            self.no_struct_lit = true;
            let cond = self.expr(0);
            self.no_struct_lit = saved;
            let cond = cond?;
            return Ok(Stmt::DoWhile { body, cond, line });
        }

        // do { body } while cond
        if self.at_ident("do") {
            self.bump();
            let body = self.block()?;
            if !self.eat_ident("while") {
                return Err(crate::lb!(self.line(), "do block must be followed by while", "do 块后必须跟 while"));
            }
            let saved = self.no_struct_lit;
            self.no_struct_lit = true;
            let cond = self.expr(0);
            self.no_struct_lit = saved;
            let cond = cond?;
            return Ok(Stmt::DoWhile { body, cond, line });
        }

        // do-while
        if self.at_ident("do") {
            self.bump();
            let body = self.block()?;
            if !self.eat_ident("while") {
                return Err(crate::lb!(self.line(), "do must be followed by while", "do 后必须跟 while"));
            }
            let saved = self.no_struct_lit;
            self.no_struct_lit = true;
            let cond = self.expr(0);
            self.no_struct_lit = saved;
            let cond = cond?;
            return Ok(Stmt::DoWhile { body, cond, line });
        }

        // do-while
        if self.at_ident("do") {
            self.bump();
            let body = self.block()?;
            if !self.eat_ident("while") {
                return Err(crate::lb!(self.line(), "do must be followed by while", "do 后必须跟 while"));
            }
            let saved = self.no_struct_lit;
            self.no_struct_lit = true;
            let cond = self.expr(0);
            self.no_struct_lit = saved;
            let cond = cond?;
            return Ok(Stmt::DoWhile { body, cond, line });
        }

        // do-while
        if self.at_ident("do") {
            self.bump();
            let body = self.block()?;
            if !self.eat_ident("while") {
                return Err(crate::lb!(self.line(), "do must be followed by while", "do 后必须跟 while"));
            }
            let saved = self.no_struct_lit;
            self.no_struct_lit = true;
            let cond = self.expr(0);
            self.no_struct_lit = saved;
            let cond = cond?;
            return Ok(Stmt::DoWhile { body, cond, line });
        }

        // do-while
        if self.at_ident("do") {
            self.bump();
            let body = self.block()?;
            if !self.eat_ident("while") {
                return Err(crate::lb!(self.line(), "do must be followed by while", "do 后必须跟 while"));
            }
            let saved = self.no_struct_lit;
            self.no_struct_lit = true;
            let cond = self.expr(0);
            self.no_struct_lit = saved;
            let cond = cond?;
            return Ok(Stmt::DoWhile { body, cond, line });
        }

        // 标签循环：`label: for/while ...`
        if let Tok::Ident(lbl) = &self.cur().tok {
            if matches!(&self.peek_at(1).tok, Tok::Punct(p) if p == ":")
                && matches!(&self.peek_at(2).tok, Tok::Ident(n) if n == "for" || n == "while") {
                let lbl = lbl.clone();
                self.bump(); // label
                self.bump(); // :
                let inner = self.stmt()?;
                return Ok(Stmt::Labeled { label: lbl, inner: Box::new(inner), line });
            }
        }

        // `loop N { body }`：重复 N 次（计数循环）—— 等价于 for _ in 0..N
        if self.at_ident("loop") {
            self.bump();
            let saved = self.no_struct_lit;
            self.no_struct_lit = true;
            let n = self.expr(0);
            self.no_struct_lit = saved;
            let n = n?;
            let body = self.block()?;
            // 用隐藏变量 `__loop_i` 计数（不污染用户作用域语义：仅作条件）
            return Ok(Stmt::ForRange { var: "__loop_i".to_string(), from: Expr::new(ExprKind::Int(0), line), to: n, body, els: None, line });
        }

        if self.at_ident("while") {
            self.bump();
            let saved = self.no_struct_lit;
            self.no_struct_lit = true;
            let cond = self.expr(0);
            self.no_struct_lit = saved;
            let cond = cond?;
            let body = self.block()?;
            return Ok(Stmt::While { cond, body, line });
        }

        if self.at_ident("for") {
            self.bump();
            let var = self.ident("循环变量")?;
            if !self.eat_ident("in") {
                return Err(crate::lb!(self.line(), "for loop is missing 'in'", "for 循环缺少 'in'"));
            }
            let saved = self.no_struct_lit;
            self.no_struct_lit = true;
            let first = self.expr(0);
            self.no_struct_lit = saved;
            let first = first?;
            if self.eat_punct("..") {
                let saved2 = self.no_struct_lit;
                self.no_struct_lit = true;
                let to = self.expr(0);
                self.no_struct_lit = saved2;
                let to = to?;
                let body = self.block()?;
                let els = self.parse_for_else()?;
                return Ok(Stmt::ForRange { var, from: first, to, body, els, line });
            }
            let body = self.block()?;
            let els = self.parse_for_else()?;
            return Ok(Stmt::ForEach { var, iter: first, body, els, line });
        }

        if self.eat_ident("return") {
            if self.at_punct("}") {
                return Ok(Stmt::Return(None, line));
            }
            let e = self.expr(0)?;
            return Ok(Stmt::Return(Some(e), line));
        }

        if self.eat_ident("break") {
            let lbl = match &self.cur().tok { Tok::Ident(n) => { let n = n.clone(); self.bump(); Some(n) } _ => None };
            return Ok(Stmt::Break(lbl, line));
        }
        if self.eat_ident("continue") {
            let lbl = match &self.cur().tok { Tok::Ident(n) => { let n = n.clone(); self.bump(); Some(n) } _ => None };
            return Ok(Stmt::Continue(lbl, line));
        }

        if self.at_punct("{") {
            return Ok(Stmt::Block(self.block()?));
        }

        // 解包赋值 `a, b = expr`（desugar 为元组 + 逐字段赋值）
        if let Tok::Ident(_) = &self.cur().tok {
            if matches!(&self.peek_at(1).tok, Tok::Punct(p) if p == ",") {
                // 预扫描：`a, b, ... =`
                let mut j = 0;
                let mut names: Vec<String> = Vec::new();
                loop {
                    if let Tok::Ident(n) = &self.peek_at(j).tok { names.push(n.clone()); } else { break; }
                    if matches!(&self.peek_at(j + 1).tok, Tok::Punct(p) if p == ",") {
                        j += 2;
                    } else {
                        break;
                    }
                }
                if names.len() >= 2 && matches!(&self.peek_at(j + 1).tok, Tok::Punct(p) if p == "=") {
                    for _ in 0..=j { self.bump(); }
                    self.bump(); // =
                    // 右值：可无括号元组 `b, a`，也可显式元组 `(b, a)`
                    let mut vals = vec![self.expr(0)?];
                    while self.eat_punct(",") {
                        if self.at_punct(";") || self.at_eof() || self.at_punct("}") { break; }
                        vals.push(self.expr(0)?);
                    }
                    let value = if vals.len() == 1 { vals.pop().unwrap() } else { Expr::new(ExprKind::TupleLit(vals), line) };
                    return Ok(desugar_unpack_assign(names, value, line));
                }
            }
        }

        // `name := expr`（推导类型声明）
        if let Some(name) = match &self.cur().tok { Tok::Ident(n) => Some(n.clone()), _ => None } {
            if matches!(&self.peek_at(1).tok, Tok::Punct(p) if p == ":=") {
                self.bump(); // 标识符
                self.bump(); // :=
                let value = self.expr(0)?;
                return Ok(Stmt::Let { name, ty: None, value, mutable: true, line });
            }

            // 数组元素赋值：`a[i] = v` / `a[i] op= v`
            if self.looks_like_index_assign() {
                let name = name.clone();
                self.bump(); // 标识符
                self.bump(); // [
                let idx = self.expr(0)?;
                self.expect_punct("]")?;
                let op = assign_op(&self.cur().tok)
                    .ok_or_else(|| crate::lb!(self.line(), "array element assignment is missing '='", "数组元素赋值缺少 '='"))?;
                self.bump();
                let value = self.expr(0)?;
                return Ok(Stmt::Assign {
                    name,
                    index: Some(idx),
                    op,
                    value,
                    line,
                });
            }

            // 自增/自减：`x++` / `x--`（改写为 x = x + 1 / x = x - 1）
            let incdec = match &self.peek_at(1).tok { Tok::Punct(p) if p == "++" || p == "--" => Some(p.clone()), _ => None };
            if let Some(p) = incdec {
                {
                    let name = name.clone();
                    self.bump(); // 标识符
                    self.bump(); // ++ / --
                    let one = Expr::new(ExprKind::Int(1), line);
                    let cur = Expr::new(ExprKind::Ident(name.clone()), line);
                    let op = if p == "++" { BinOp::Add } else { BinOp::Sub };
                    let value = Expr::new(ExprKind::Binary(op, Box::new(cur), Box::new(one)), line);
                    return Ok(Stmt::Assign { name, index: None, op: None, value, line });
                }
            }

            // 复合赋值
            if let Tok::Punct(p) = &self.peek_at(1).tok {
                if let Some(op) = assign_op(&Tok::Punct(p.clone())) {
                    let name = name.clone();
                    self.bump();
                    self.bump();
                    let value = self.expr(0)?;
                    return Ok(Stmt::Assign { name, index: None, op, value, line });
                }
            }

            // 字段赋值：`obj.field = v` / `obj.field op= v`
            if matches!(&self.peek_at(1).tok, Tok::Punct(p) if p == ".") {
                if let Tok::Ident(field) = self.peek_at(2).tok.clone() {
                    if let Tok::Punct(p) = &self.peek_at(3).tok {
                        if let Some(op) = assign_op(&Tok::Punct(p.clone())) {
                            let obj = name.clone();
                            self.bump(); // obj
                            self.bump(); // .
                            self.bump(); // field
                            self.bump(); // op
                            let value = self.expr(0)?;
                            return Ok(Stmt::FieldAssign { obj, field, op, value, line });
                        }
                    }
                }
            }
        }

        // 表达式语句
        let e = self.expr(0)?;
        Ok(Stmt::Expr(e))
    }

    /// 前瞻判断当前位置是否是 `标识符 [ ... ] = ...` 或 `标识符 [ ... ] op= ...`。
    ///
    /// 用前瞻而不是「先解析再回退」，避免解析器需要回溯。
    pub(crate) fn looks_like_index_assign(&self) -> bool {
        if !matches!(self.cur().tok, Tok::Ident(_)) {
            return false;
        }
        if !matches!(&self.peek_at(1).tok, Tok::Punct(p) if p == "[") {
            return false;
        }
        // 从 `[` 开始做括号配平，找到与之匹配的 `]`
        let mut depth = 0i32;
        let mut j = self.pos + 1;
        while j < self.toks.len() {
            match &self.toks[j].tok {
                Tok::Punct(p) if p == "[" => depth += 1,
                Tok::Punct(p) if p == "]" => {
                    depth -= 1;
                    if depth == 0 {
                        // `]` 之后必须是赋值运算符
                        return self
                            .toks
                            .get(j + 1)
                            .map(|t| assign_op(&t.tok).is_some())
                            .unwrap_or(false);
                    }
                }
                Tok::Eof => return false,
                _ => {}
            }
            j += 1;
        }
        false
    }

    /// `if cond { } elif ... { } elif ... { } else { }` 的公共部分。
    /// `elif` 是 `else if` 的语法糖，二者等价。
    pub(crate) fn if_parts(&mut self) -> Result<(Expr, Block, Option<Block>), String> {
        self.eat_ident("if");
        let saved = self.no_struct_lit;
        self.no_struct_lit = true;
        let cond = self.expr(0);
        self.no_struct_lit = saved;
        let cond = cond?;
        let then = self.block()?;
        let els = if self.eat_ident("elif") {
            // elif → 嵌套 if 作为 else 分支
            let line = self.line();
            let (c2, t2, e2) = self.if_parts_after_elif()?;
            Some(vec![Stmt::If { cond: c2, then: t2, els: e2, line }])
        } else if self.eat_ident("else") {
            if self.at_ident("if") {
                let line = self.line();
                let (c2, t2, e2) = self.if_parts()?;
                Some(vec![Stmt::If { cond: c2, then: t2, els: e2, line }])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok((cond, then, els))
    }

    /// `elif cond { } ...`：已在 elif 之后，解析条件与块及后续分支。
    pub(crate) fn if_parts_after_elif(&mut self) -> Result<(Expr, Block, Option<Block>), String> {
        let cond = self.expr(0)?;
        let then = self.block()?;
        let els = if self.eat_ident("elif") {
            let line = self.line();
            let (c2, t2, e2) = self.if_parts_after_elif()?;
            Some(vec![Stmt::If { cond: c2, then: t2, els: e2, line }])
        } else if self.eat_ident("else") {
            if self.at_ident("if") {
                let line = self.line();
                let (c2, t2, e2) = self.if_parts()?;
                Some(vec![Stmt::If { cond: c2, then: t2, els: e2, line }])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok((cond, then, els))
    }

}

