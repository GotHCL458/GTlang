//! GTLang 递归下降语法分析器。

use crate::ast::*;
use crate::lexer::{is_ident_cont, is_ident_start, lex, lex_expr_text, is_keyword, Tok, Token};

pub fn parse_program(src: &str) -> Result<Program, ParseError> {
    match parse_program_multi(src) {
        Ok(p) => Ok(p),
        Err(errs) => Err(errs.into_iter().next().unwrap()),
    }
}

/// 多错误版本：收集**全部**语法错误（panic-mode 恢复）。
pub fn parse_program_multi(src: &str) -> Result<Program, Vec<ParseError>> {
    let (main_src, cblock) = match crate::cblock::extract(src) {
        Ok(x) => x,
        Err(e) => return Err(vec![ParseError::new(e, Span::default())]),
    };
    let toks = match lex(&main_src) {
        Ok(t) => t,
        Err(e) => return Err(vec![e]),
    };
    let mut p = Parser { toks, pos: 0, src_line_base: 0, no_struct_lit: false, type_params: Vec::new() };
    let (mut prog, errs) = p.program_multi();
    prog.cblock = cblock.code;
    prog.cfuncs = cblock.funcs;
    let mut externs: Vec<crate::cblock::CFn> = Vec::new();
    for item in &prog.items {
        if let Item::ExternC(fns) = item {
            for f in fns {
                externs.push(crate::cblock::CFn {
                    name: f.name.clone(),
                    params: f.params.clone(),
                    ret: f.ret.clone(),
                    head: (0, 0),
                });
            }
        }
    }
    prog.cfuncs.extend(externs);
    if errs.is_empty() { Ok(prog) } else {
        Err(errs.into_iter().map(|(m, sp)| ParseError::new(m, sp)).collect())
    }
}

/// 解析一段独立表达式文本（用于字符串插值 `{...}` 内部）
pub fn parse_expr_str(text: &str, line: usize) -> Result<Expr, ParseError> {
    let toks = lex_expr_text(text)?;
    let mut p = Parser { toks, pos: 0, src_line_base: line, no_struct_lit: false, type_params: Vec::new() };
    let e = match p.expr(0) {
        Ok(v) => v,
        Err(m) => return Err(ParseError::new(m, p.cur().span)),
    };
    if !p.at_eof() {
        return Err(ParseError::new(
            crate::lb!(line, "trailing content in interpolation expression", "插值表达式解析有剩余内容"),
            p.cur().span,
        ));
    }
    Ok(e)
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    /// 子解析器（插值）使用的行号基准
    src_line_base: usize,
    /// 禁止结构体字面量：在 `for ... in EXPR {` / `if EXPR {` 等位置，
    /// `{` 属于块而非结构体字面量，避免 `for v in a { ... }` 被误解析。
    no_struct_lit: bool,
    /// 当前函数作用域内的泛型类型参数名（用于 `parse_type` 产出 Ty::Generic）
    type_params: Vec<String>,
}

impl Parser {
    // ---------- 基础工具 ----------

    fn cur(&self) -> &Token {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }

    fn peek_at(&self, n: usize) -> &Token {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)]
    }

    fn line(&self) -> usize {
        let l = self.cur().line;
        if self.src_line_base > 0 {
            self.src_line_base + l - 1
        } else {
            l
        }
    }

    fn at_eof(&self) -> bool {
        matches!(self.cur().tok, Tok::Eof)
    }

    fn bump(&mut self) {
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
    }

    fn at_punct(&self, s: &str) -> bool {
        matches!(&self.cur().tok, Tok::Punct(p) if p == s)
    }

    fn at_ident(&self, s: &str) -> bool {
        matches!(&self.cur().tok, Tok::Ident(n) if n == s)
    }

    fn eat_punct(&mut self, s: &str) -> bool {
        if self.at_punct(s) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_ident(&mut self, s: &str) -> bool {
        if self.at_ident(s) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect_punct(&mut self, s: &str) -> Result<(), String> {
        if self.eat_punct(s) {
            Ok(())
        } else {
            Err(crate::lb!(self.line(), "expected '{}', found {}", "应为 '{}'，实际是 {}", s, self.describe()))
        }
    }

    fn describe(&self) -> String {
        match &self.cur().tok {
            Tok::Ident(n) => format!("标识符 '{}'", n),
            Tok::Int(v) => format!("整数 {}", v),
            Tok::Float(v) => format!("浮点 {}", v),
            Tok::Str(_) => "字符串".into(),
            Tok::RawStr(_) => "原始字符串".into(),
            Tok::Punct(p) => format!("'{}'", p),
            Tok::Eof => "文件结尾".into(),
        }
    }

    fn ident(&mut self, what: &str) -> Result<String, String> {
        match &self.cur().tok {
            Tok::Ident(n) => {
                let n = n.clone();
                self.bump();
                Ok(n)
            }
            _ => Err(crate::lb!(self.line(), "{} must be an identifier, found {}", "{}应为标识符，实际是 {}", what, self.describe())),
        }
    }

    // ---------- 顶层 ----------

    /// 在「语句 / 顶层」位置把 `//` 到行尾当行注释跳过。
    ///
    /// `//` 同时是整除运算符，所以只在**不处于表达式中间**的位置才当注释：
    /// - `// 说明`（行首）、`} // 说明` → 注释
    /// - `x := a // 5`（表达式中间）→ 整除，交给 `expr()` 正常消费
    ///
    /// 需要行内注释时用 `#`（词法阶段处理，任何位置都认）。
    fn skip_line_comment(&mut self) {
        while self.at_punct("//") {
            self.bump();
            // 吃掉本行剩余 token（下一个 token 带 nl_before 说明已换行）
            while !self.at_eof() && !self.cur().nl_before {
                self.bump();
            }
        }
    }

    /// 跳到下一个顶层声明起始、下一行开头，或 EOF（语法错误恢复同步点）。
    fn sync_top_level(&mut self) {
        // 先前进一个 token，避免死循环（当前位置本身是同步点）
        self.bump();
        while !self.at_eof() {
            if crate::parser_recover::is_item_start(&self.cur().tok) {
                return;
            }
            // 跨行边界：顶层每行通常一个项，遇到"行首"即可停止
            if self.cur().nl_before {
                return;
            }
            self.bump();
        }
    }

    /// 收集顶层全部语法错误（panic-mode 恢复）。
    pub fn program_multi(&mut self) -> (Program, Vec<(String, Span)>) {
        let mut items = Vec::new();
        let mut imports = Vec::new();
        let mut errors: Vec<(String, Span)> = Vec::new();
        while !self.at_eof() {
            self.skip_line_comment();
            if self.at_eof() {
                break;
            }
            let _sp = self.cur().span;
            let _r: Result<(), String> = (|| {
            // 允许 `pub` 前缀
            let is_pub = self.eat_ident("pub");
            // `@derive(Eq, Debug)` 注解
            let mut derives: Vec<String> = Vec::new();
            while self.at_punct("@") {
                self.bump();
                let dname = self.ident("注解名")?;
                if dname == "test" {
                    continue;
                }
                if dname == "derive" {
                    self.expect_punct("(")?;
                    while !self.at_punct(")") {
                        derives.push(self.ident("派生 trait 名")?);
                        if !self.eat_punct(",") { break; }
                    }
                    self.expect_punct(")")?;
                } else {
                    return Err(crate::lb!(self.line(), "unknown attribute '@{}'", "未知注解 '@{}'", dname));
                }
            }

            if self.eat_ident("import") {
                if is_pub {
                    return Err(crate::lb!(self.line(), "`import` cannot be marked `pub`", "`import` 不能带 `pub`"));
                }
                imports.push(self.import_decl()?);
            } else if self.eat_ident("fn") {
                items.push(Item::Fn(self.fn_def(is_pub)?));
            } else if self.eat_ident("const") {
                let line = self.line();
                let name = self.ident("const 名称")?;
                let ty = if self.eat_punct(":") { Some(self.parse_type()?) } else { None };
                self.expect_punct("=")?;
                let value = self.expr(0)?;
                items.push(Item::Const { name, ty, value, line, is_pub });
            } else if self.eat_ident("extern") {
                // extern "C" { fn ... }
                if let Tok::Str(abi) = self.cur().tok.clone() {
                    self.bump();
                    if abi != "C" && abi != "c" {
                        return Err(crate::lb!(self.line(), "only extern \"C\" is supported", "暂只支持 extern \"C\""));
                    }
                }
                self.expect_punct("{")?;
                let mut fns = Vec::new();
                loop {
                    self.skip_line_comment();
                    if self.at_punct("}") {
                        break;
                    }
                    if self.at_eof() {
                        return Err("extern 块缺少 '}'".into());
                    }
                    if !self.eat_ident("fn") {
                        return Err(crate::lb!(self.line(), "extern block only supports fn", "extern 块内只支持 fn"));
                    }
                    let name = self.ident("函数名")?;
                    self.expect_punct("(")?;
                    let mut params = Vec::new();
                    while !self.at_punct(")") {
                        let _pname = self.ident("参数名")?;
                        let ty = if self.eat_punct(":") { Some(self.parse_type()?) } else { None };
                        params.push(ty.unwrap_or(Ty::I64));
                        if !self.eat_punct(",") {
                            break;
                        }
                    }
                    self.expect_punct(")")?;
                    let ret = if self.eat_punct("->") {
                        self.eat_punct("!");
                        if self.at_punct("{") || self.at_punct("}") {
                            Ty::Void
                        } else {
                            self.parse_type()?
                        }
                    } else {
                        Ty::Void
                    };
                    fns.push(ExternFn { name, params, ret });
                }
                self.expect_punct("}")?;
                items.push(Item::ExternC(fns));
            } else if self.eat_ident("macro") {
                let line = self.line();
                let name = self.ident("宏名")?;
                self.expect_punct("(")?;
                let mut params = Vec::new();
                while !self.at_punct(")") {
                    params.push(self.ident("宏参数")?);
                    if !self.eat_punct(",") { break; }
                }
                self.expect_punct(")")?;
                self.expect_punct("{")?;
                let body = self.expr(0)?;
                self.expect_punct("}")?;
                items.push(Item::Macro { name, params, body, line });
            } else if self.eat_ident("enum") {
                items.push(Item::Enum(self.enum_def(is_pub, derives.clone())?));
            } else if self.eat_ident("struct") {
                items.push(Item::Struct(self.struct_def(is_pub, derives.clone())?));
            } else if self.eat_ident("trait") {
                items.push(Item::Trait(self.trait_def(is_pub)?));
            } else if self.eat_ident("impl") {
                let line = self.line();
                // 可选泛型参数：`impl[T, U] ...`
                let mut impl_tps: Vec<String> = Vec::new();
                if self.at_punct("[") {
                    self.bump();
                    while !self.at_punct("]") {
                        impl_tps.push(self.ident("类型参数名")?);
                        if !self.eat_punct(",") { break; }
                    }
                    self.expect_punct("]")?;
                }
                let saved_tp = std::mem::take(&mut self.type_params);
                self.type_params = impl_tps.clone();
                // `impl Trait for Type` 或 `impl Type`
                let first = self.ident("类型名或 trait 名")?;
                let (trait_name, ty) = if self.eat_ident("for") {
                    let t = self.ident("类型名")?;
                    (Some(first), t)
                } else {
                    (None, first)
                };
                self.expect_punct("{")?;
                let mut methods = Vec::new();
                loop {
                    self.skip_line_comment();
                    if self.at_punct("}") {
                        break;
                    }
                    if self.at_eof() {
                        return Err("impl 块缺少 '}'".into());
                    }
                    let mpub = self.eat_ident("pub");
                    if !self.eat_ident("fn") {
                        return Err(crate::lb!(self.line(), "impl block only supports fn", "impl 块内只支持 fn"));
                    }
                    methods.push(self.fn_def(mpub)?);
                }
                self.expect_punct("}")?;
                self.type_params = saved_tp;
                if let Some(tn) = trait_name {
                    items.push(Item::TraitImpl { type_params: impl_tps, trait_name: tn, ty, methods, line });
                } else {
                    items.push(Item::Impl { type_params: impl_tps, ty, methods, line });
                }
            } else if self.eat_ident("macro") {
                let line = self.line();
                let name = self.ident("宏名")?;
                self.expect_punct("(")?;
                let mut params = Vec::new();
                while !self.at_punct(")") {
                    params.push(self.ident("宏参数")?);
                    if !self.eat_punct(",") { break; }
                }
                self.expect_punct(")")?;
                self.expect_punct("{")?;
                let body = self.expr(0)?;
                self.expect_punct("}")?;
                items.push(Item::Macro { name, params, body, line });
            } else if self.eat_ident("enum") {
                items.push(Item::Enum(self.enum_def(is_pub, derives.clone())?));
            } else if self.eat_ident("struct") {
                items.push(Item::Struct(self.struct_def(is_pub, derives.clone())?));
            } else {
                return Err(crate::lb!(
                    self.line(),
                    "unexpected token '{}' at top level; expected a declaration (fn / struct / enum / trait / impl / macro / const / import / extern)",
                    "顶层出现意外符号 '{}'；期望声明（fn / struct / enum / trait / impl / macro / const / import / extern）",
                    self.describe()
                ));
            }
            Ok(())
            })();
            if let Err(e) = _r {
                errors.push((e, self.cur().span));
                self.sync_top_level();
            }
        }
        // cblock / cfuncs 由 parse_program 在外层回填（内联 C 块）
        (Program { items, imports, ..Default::default() }, errors)
    }

    /// 解析一条 `import` 声明：
    ///   import a.b.c            → path=[a,b,c]
    ///   import a.b as x         → path=[a,b], alias=x
    ///   import math.vector
    ///   import "path.gt" as 插件 → is_file=true
    fn import_decl(&mut self) -> Result<Import, String> {
        let line = self.line();
        // C 头导入：`import c "foo.h" [as 别名]`
        if self.at_ident("c") {
            // 仅当其后是字符串时才当作 C 头导入（否则 "c" 可能是模块名）
            if matches!(&self.peek_at(1).tok, Tok::Str(_) | Tok::RawStr(_)) {
                self.bump(); // c
                let s = match self.cur().tok.clone() {
                    Tok::Str(s) | Tok::RawStr(s) => s,
                    _ => unreachable!(),
                };
                self.bump();
                let alias = if self.eat_ident("as") { Some(self.ident("别名")?) } else { None };
                return Ok(Import { is_c_header: true, path: vec![s], alias, is_file: true, line });
            }
        }
        // 字符串路径形式
        if let Tok::Str(s) | Tok::RawStr(s) = &self.cur().tok {
            let s = s.clone();
            self.bump();
            let alias = if self.eat_ident("as") { Some(self.ident("别名")?) } else { None };
            return Ok(Import { is_c_header: false, path: vec![s], alias, is_file: true, line });
        }
        // 路径段形式：ident ((`.` | `::`) ident)*
        let mut path = vec![self.ident("模块名")?];
        loop {
            if self.at_punct(".") || self.at_punct("::") {
                self.bump();
                path.push(self.ident("模块名")?);
            } else {
                break;
            }
        }
        let alias = if self.eat_ident("as") { Some(self.ident("别名")?) } else { None };
        Ok(Import { is_c_header: false, path, alias, is_file: false, line })
    }

    /// `trait 名字 { fn 方法(params) -> R ... }`
    fn trait_def(&mut self, is_pub: bool) -> Result<TraitDef, String> {
        let line = self.line();
        let name = self.ident("trait 名")?;
        self.expect_punct("{")?;
        let mut methods = Vec::new();
        let mut defaults: Vec<(String, Vec<(String, Ty)>, Ty, Block)> = Vec::new();
        loop {
            self.skip_line_comment();
            if self.at_punct("}") {
                break;
            }
            if self.at_eof() {
                return Err("trait 块缺少 '}'".into());
            }
            self.eat_ident("pub");
            if !self.eat_ident("fn") {
                return Err(crate::lb!(self.line(), "trait block only supports fn signatures", "trait 块内只支持 fn 签名"));
            }
            let mname = self.ident("方法名")?;
            self.expect_punct("(")?;
            let mut ptypes = Vec::new();
            let mut pnames: Vec<(String, Ty)> = Vec::new();
            while !self.at_punct(")") {
                if self.at_punct("&") {
                    self.bump();
                    self.eat_ident("mut");
                }
                let pn = self.ident("参数名")?;
                let ty = if self.eat_punct(":") { self.parse_type()? } else { Ty::I64 };
                ptypes.push(ty.clone());
                pnames.push((pn, ty));
                if !self.eat_punct(",") {
                    break;
                }
            }
            self.expect_punct(")")?;
            let ret = if self.eat_punct("->") {
                self.eat_punct("!");
                if self.at_punct("{") || self.at_punct("}") || self.at_punct(",") {
                    Ty::Void
                } else {
                    self.parse_type()?
                }
            } else {
                Ty::Void
            };
            // 默认方法：带 body
            if self.at_punct("{") {
                let body = self.block()?;
                defaults.push((mname.clone(), pnames, ret.clone(), body));
            } else {
                self.eat_punct(";");
                self.eat_punct(",");
            }
            methods.push((mname, ptypes, ret));
        }
        self.expect_punct("}")?;
        Ok(TraitDef { name, methods, defaults, line, is_pub })
    }

    /// `struct Name { f: T, ... }`（字段类型可省略）
    /// `enum 名 { V1(T1, T2) V2 ... }`
    fn enum_def(&mut self, is_pub: bool, derives: Vec<String>) -> Result<EnumDef, String> {
        let line = self.line();
        let name = self.ident("枚举名")?;
        let mut type_params: Vec<String> = Vec::new();
        if self.at_punct("[") {
            self.bump();
            while !self.at_punct("]") {
                type_params.push(self.ident("类型参数名")?);
                if !self.eat_punct(",") { break; }
            }
            self.expect_punct("]")?;
        }
        let saved_tp = std::mem::take(&mut self.type_params);
        self.type_params = type_params.clone();
        self.expect_punct("{")?;
        let mut variants = Vec::new();
        loop {
            self.skip_line_comment();
            if self.at_punct("}") { break; }
            if self.at_eof() { return Err("enum 缺少 '}'".into()); }
            let vname = self.ident("变体名")?;
            let mut payload = Vec::new();
            if self.eat_punct("(") {
                while !self.at_punct(")") {
                    payload.push(self.parse_type()?);
                    if !self.eat_punct(",") { break; }
                }
                self.expect_punct(")")?;
            }
            variants.push((vname, payload));
            self.eat_punct(",");
        }
        self.expect_punct("}")?;
        self.type_params = saved_tp;
        Ok(EnumDef { name, type_params, variants, derives, line, is_pub })
    }

    fn struct_def(&mut self, is_pub: bool, derives: Vec<String>) -> Result<StructDef, String> {
        let line = self.line();
        let name = self.ident("结构体名")?;
        // 泛型参数：`struct 盒[T, U]`
        let mut type_params: Vec<String> = Vec::new();
        if self.at_punct("[") {
            self.bump();
            while !self.at_punct("]") {
                type_params.push(self.ident("类型参数名")?);
                if !self.eat_punct(",") { break; }
            }
            self.expect_punct("]")?;
        }
        let saved_tp = std::mem::take(&mut self.type_params);
        self.type_params = type_params.clone();
        self.expect_punct("{")?;
        let mut fields = Vec::new();
        loop {
            self.skip_line_comment();
            if self.at_punct("}") {
                break;
            }
            if self.at_eof() {
                return Err("struct 缺少 '}'".into());
            }
            let fline = self.line();
            let fname = self.ident("字段名")?;
            let fty = if self.eat_punct(":") { Some(self.parse_type()?) } else { None };
            fields.push((fname, fty, fline));
            self.eat_punct(",");
        }
        self.expect_punct("}")?;
        self.type_params = saved_tp;
        Ok(StructDef { name, type_params, fields, derives, line, is_pub })
    }

    fn fn_def(&mut self, is_pub: bool) -> Result<FnDef, String> {
        let line = self.line();
        let name = self.ident("函数名")?;
        // 泛型类型参数：`fn f[T, U](...)`
        let mut type_params: Vec<String> = Vec::new();
        let mut inline_bounds: Vec<(String, String)> = Vec::new();
        if self.at_punct("[") {
            self.bump();
            while !self.at_punct("]") {
                let tp = self.ident("类型参数名")?;
                // 内联约束 `[T: Trait]`
                if self.eat_punct(":") {
                    let tr = self.ident("trait 名")?;
                    inline_bounds.push((tp.clone(), tr));
                }
                type_params.push(tp);
                if !self.eat_punct(",") {
                    break;
                }
            }
            self.expect_punct("]")?;
        }
        // 让 `parse_type` 把本函数的类型参数识别为 Ty::Generic
        let saved_tp = std::mem::take(&mut self.type_params);
        self.type_params = type_params.clone();
        self.expect_punct("(")?;
        let mut params = Vec::new();
        while !self.at_punct(")") {
            let pline = self.line();
            // `self` / `&self` / `&mut self` 接收者
            if self.at_punct("&") {
                self.bump();
                self.eat_ident("mut");
            }
            let pname = self.ident("参数名")?;
            let ty = if self.eat_punct(":") { Some(self.parse_type()?) } else { None };
            let default = if self.eat_punct("=") { Some(self.expr(0)?) } else { None };
            params.push(Param { name: pname, ty, default, line: pline });
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct(")")?;

        let ret = if self.eat_punct("->") {
            self.eat_punct("!"); // `-> !T` / `-> !`
            if self.at_punct("{") {
                None
            } else {
                Some(self.parse_type()?)
            }
        } else {
            None
        };

        // `where T: Trait, U: Trait2`（与内联 `[T: Trait]` 合并）
        let mut bounds: Vec<(String, String)> = inline_bounds;
        if self.eat_ident("where") {
            loop {
                let tp = self.ident("类型参数")?;
                self.expect_punct(":")?;
                let tr = self.ident("trait 名")?;
                bounds.push((tp, tr));
                if !self.eat_punct(",") {
                    break;
                }
            }
        }
        let body = self.block()?;
        self.type_params = saved_tp;
        Ok(FnDef { name, type_params, params, ret, ret_ty: Ty::Unknown, body, line, is_pub, bounds })
    }

    fn parse_type(&mut self) -> Result<Ty, String> {
        // 可选类型 `?T` → Option[T]
        if self.eat_punct("?") {
            let inner = self.parse_type()?;
            return Ok(Ty::Option(Box::new(inner)));
        }
        // 借用类型 `&T` / `&mut T`
        if self.eat_punct("&") {
            let is_mut = self.eat_ident("mut");
            let inner = self.parse_type()?;
            return Ok(if is_mut { Ty::RefMut(Box::new(inner)) } else { Ty::Ref(Box::new(inner)) });
        }
        // `dyn Trait`：trait 对象
        if self.eat_ident("dyn") {
            let tname = self.ident("trait 名")?;
            return Ok(Ty::Dyn(tname));
        }
        // 定长数组 [T; N]
        if self.eat_punct("[") {
            let elem = self.parse_type()?;
            self.expect_punct(";")?;
            let n = match &self.cur().tok {
                Tok::Int(v) => *v as usize,
                _ => return Err(crate::lb!(self.line(), "array length must be an integer constant", "数组长度应为整数常量")),
            };
            self.bump();
            self.expect_punct("]")?;
            return Ok(Ty::Array(Box::new(elem), n));
        }
        let name = self.ident("类型名")?;
        // 泛型类型参数优先（在 `[T]` 之前判断）
        if self.type_params.contains(&name) {
            return Ok(Ty::Generic(name));
        }
        // 参数化容器：`list[T]` / `set[T]` / `map[K,V]`
        if self.at_punct("[") {
            self.bump();
            let first = self.parse_type()?;
            let ty = match name.as_str() {
                "list" | "List" => Ty::List(Box::new(first)),
                "set" | "Set" => Ty::Set(Box::new(first)),
                "map" | "Map" | "dict" => {
                    self.expect_punct(",")?;
                    let second = self.parse_type()?;
                    Ty::Map(Box::new(first), Box::new(second))
                }
                "Result" | "result" => {
                    self.expect_punct(",")?;
                    let second = self.parse_type()?;
                    Ty::Result(Box::new(first), Box::new(second))
                }
                _ => {
                    // 未知参数化类型：忽略参数，按名字处理
                    first
                }
            };
            self.expect_punct("]")?;
            return Ok(ty);
        }
        Ok(Ty::from_name(&name).unwrap_or_else(|| Ty::Struct(name)))
    }

    // ---------- 语句 ----------

    /// 解析 for 循环后可选 `else { ... }`（无 break 正常结束时执行）。
    fn parse_for_else(&mut self) -> Result<Option<Block>, String> {
        if self.at_ident("else") {
            self.bump();
            Ok(Some(self.block()?))
        } else {
            Ok(None)
        }
    }

    fn block(&mut self) -> Result<Block, String> {
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
    fn try_parts(&mut self) -> Result<(Block, Vec<CatchArm>, Option<Block>), String> {
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
    fn try_expr(&mut self, line: usize) -> Result<Expr, String> {
        let (body, catches, fin) = self.try_parts()?;
        Ok(Expr::new(ExprKind::TryBlock { body, catches, fin }, line))
    }

    fn stmt(&mut self) -> Result<Stmt, String> {
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
    fn looks_like_index_assign(&self) -> bool {
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
    fn if_parts(&mut self) -> Result<(Expr, Block, Option<Block>), String> {
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
    fn if_parts_after_elif(&mut self) -> Result<(Expr, Block, Option<Block>), String> {
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

    // ---------- 表达式 ----------

    fn expr(&mut self, min_prec: u8) -> Result<Expr, String> {
        let mut lhs = self.unary()?;
        loop {
            // 三元：`a if cond else b`（Python 风格，最低优先级）
            if self.at_ident("if") && min_prec == 0 && !self.cur().nl_before {
                let line = self.line();
                self.bump();
                let saved = self.no_struct_lit;
                self.no_struct_lit = true;
                let cond = self.expr(0);
                self.no_struct_lit = saved;
                let cond = cond?;
                if !self.eat_ident("else") {
                    return Err(crate::lb!(self.line(), "ternary expects 'else'", "三元表达式缺少 'else'"));
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
            // 管道：`x |> f` → `f(x)`（最低优先级，左结合）
            if self.at_punct("|>") && min_prec == 0 {
                let line = self.line();
                self.bump();
                let rhs = self.expr(1)?;
                // 右值应是"函数调用样式"：`f` 或 `f(a, b)` → 把 lhs 作为首参插入
                let call = match rhs.kind {
                    ExprKind::Call(name, mut args) => { let mut v = vec![lhs]; v.append(&mut args); Expr::new(ExprKind::Call(name, v), line) }
                    ExprKind::Ident(name) => Expr::new(ExprKind::Call(name, vec![lhs]), line),
                    other => Expr::new(other, line), // 非调用：保持（类型检查会报错）
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
            let line = self.line();
            self.bump();
            let rhs = self.expr(prec + 1)?;
            lhs = Expr::new(ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), line);
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, String> {

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

    fn postfix(&mut self, mut e: Expr) -> Result<Expr, String> {
        loop {
            let line = self.line();
            // Result 传播：`expr?`
            if self.at_punct("?") {
                self.bump();
                e = Expr::new(ExprKind::Try(Box::new(e)), line);
                continue;
            }
            // 兜底：`expr or { 默认值 }` / `expr or 默认值`
            if self.at_ident("or") {
                self.bump();
                let default = if self.at_punct("{") {
                    // `{ block }` 作为默认值块：用 `if true { block }` 承载块值语义
                    let blk = self.block()?;
                    Expr::new(
                        ExprKind::If {
                            cond: Box::new(Expr::new(ExprKind::Bool(true), line)),
                            then: blk,
                            els: None,
                        },
                        line,
                    )
                } else {
                    self.expr(0)?
                };
                e = Expr::new(ExprKind::TryOr { inner: Box::new(e), default: Box::new(default) }, line);
                continue;
            }
            // 字段访问：`expr.field`（但不要吞掉限定名 `mod.fn`，那已在 primary 折叠）
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
                let callee = match &e.kind {
                    ExprKind::Ident(n) => Some(n.clone()),
                    ExprKind::Field(base, f) => {
                        if let ExprKind::Ident(h) = &base.kind {
                            Some(format!("{}.{}", h, f))
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(name) = callee {
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
                    if named.is_empty() {
                        e = Expr::new(ExprKind::Call(name, args), line);
                    } else {
                        // 混用位置 + 命名：位置参数放前
                        let mut all: Vec<(String, Expr)> = Vec::new();
                        for a in args { all.push((String::new(), a)); }
                        all.extend(named);
                        e = Expr::new(ExprKind::CallNamed(name, all), line);
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
    fn match_expr(&mut self, line: usize) -> Result<Expr, String> {
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
                Some(self.expr(0)?)
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
    fn closure_expr(&mut self, line: usize) -> Result<Expr, String> {
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
    fn struct_lit(&mut self, name: String, line: usize) -> Result<Expr, String> {
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

    fn primary(&mut self) -> Result<Expr, String> {
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
                if is_keyword(&name) {
                    return Err(crate::lb!(line, "keyword '{}' cannot be used as an expression", "关键字 '{}' 不能作为表达式", name));
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

/// 赋值运算符 → 对应的复合运算（`=` 映射为 `Some(None)`，非赋值返回 None）
fn assign_op(tok: &Tok) -> Option<Option<BinOp>> {
    let p = match tok {
        Tok::Punct(p) => p.as_str(),
        _ => return None,
    };
    Some(match p {
        "=" => None,
        "+=" => Some(BinOp::Add),
        "-=" => Some(BinOp::Sub),
        "*=" => Some(BinOp::Mul),
        "/=" => Some(BinOp::Div),
        "%=" => Some(BinOp::Rem),
        "&=" => Some(BinOp::BitAnd),
        "|=" => Some(BinOp::BitOr),
        "^=" => Some(BinOp::BitXor),
        "<<=" => Some(BinOp::Shl),
        ">>=" => Some(BinOp::Shr),
        _ => return None,
    })
}

/// 二元运算符优先级（数值越大结合越紧）。
/// 位运算按 C 的习惯排在比较之下、逻辑之上，移位高于按位与或。
fn prec_of(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 1,
        BinOp::And => 2,
        BinOp::BitOr => 3,
        BinOp::BitXor => 4,
        BinOp::BitAnd => 5,
        BinOp::Eq | BinOp::Ne => 6,
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 7,
        BinOp::Shl | BinOp::Shr => 8,
        BinOp::Add | BinOp::Sub => 9,
        BinOp::Mul | BinOp::Div | BinOp::FloorDiv | BinOp::Rem => 10,
    }
}

/// 把字符串字面量按 `$` 插值拆成「字面片段 + 表达式」。
///
/// 支持两种形式：
/// - `$name`     —— 简单标识符
/// - `${expr}`   —— 任意表达式（花括号可嵌套）
///
/// 需要字面 `$` 时写 `\$`（词法阶段已还原为 `$`，这里用 `\$` 转义后同理）。
fn interp(raw: &str, line: usize) -> Result<ExprKind, String> {
    let chars: Vec<char> = raw.chars().collect();
    let mut parts: Vec<StrPart> = Vec::new();
    let mut lit = String::new();
    let mut i = 0;

    while i < chars.len() {
        // `\$`：字面美元符号（词法阶段刻意保留反斜杠，让转义在这里生效，
        // 否则 `\$name` 会先退化成 `$name` 又被当成插值）
        if chars[i] == '\\' && i + 1 < chars.len() && chars[i + 1] == '$' {
            lit.push('$');
            i += 2;
            continue;
        }

        if chars[i] != '$' {
            lit.push(chars[i]);
            i += 1;
            continue;
        }

        // `${expr}`
        if i + 1 < chars.len() && chars[i + 1] == '{' {
            let start = i + 2;
            let mut j = start;
            let mut depth = 1;
            while j < chars.len() {
                match chars[j] {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if j >= chars.len() {
                return Err(crate::lb!(line, "unterminated string interpolation '${{'", "字符串插值 '${{' 未闭合"));
            }
            let inner: String = chars[start..j].iter().collect();
            let inner = inner.trim().to_string();
            if !lit.is_empty() {
                parts.push(StrPart::Lit(std::mem::take(&mut lit)));
            }
            if !inner.is_empty() {
                let sub = parse_expr_str(&inner, line).map_err(|e| e.msg)?;
                parts.push(StrPart::Expr(Box::new(sub)));
            }
            i = j + 1;
            continue;
        }

        // `$name`
        if i + 1 < chars.len() && is_ident_start(chars[i + 1]) {
            let start = i + 1;
            let mut j = start;
            while j < chars.len() && is_ident_cont(chars[j]) {
                j += 1;
            }
            let name: String = chars[start..j].iter().collect();
            if !lit.is_empty() {
                parts.push(StrPart::Lit(std::mem::take(&mut lit)));
            }
            let sub = parse_expr_str(&name, line).map_err(|e| e.msg)?;
            parts.push(StrPart::Expr(Box::new(sub)));
            i = j;
            continue;
        }

        // 单独的 `$`：按字面量处理
        lit.push('$');
        i += 1;
    }

    if !lit.is_empty() {
        parts.push(StrPart::Lit(lit));
    }
    match parts.len() {
        0 => Ok(ExprKind::Str(String::new())),
        1 => match &parts[0] {
            StrPart::Lit(s) => Ok(ExprKind::Str(s.clone())),
            StrPart::Expr(e) => Ok(e.kind.clone()),
        },
        _ => Ok(ExprKind::Interp(parts)),
    }
}



/// 解构 let (a, b) = expr -> 拆成 tmp := expr; a := tmp.0; b := tmp.1
fn desugar_destructure(names: Vec<String>, value: Expr, mutable: bool, line: usize) -> Stmt {
    let tmp = format!("__dst{}", line);
    let mut stmts: Block = Vec::new();
    stmts.push(Stmt::Let { name: tmp.clone(), ty: None, value, mutable: false, line });
    for (i, n) in names.into_iter().enumerate() {
        let base = Expr::new(ExprKind::Ident(tmp.clone()), line);
        let field = Expr::new(ExprKind::Field(Box::new(base), i.to_string()), line);
        stmts.push(Stmt::Let { name: n, ty: None, value: field, mutable, line });
    }
    Stmt::Block(stmts)
}

/// 解包赋值 `a, b = expr` -> { tmp := expr; a = tmp.0; b = tmp.1 }（原地交换靠 tmp 保留旧值）。
fn desugar_unpack_assign(names: Vec<String>, value: Expr, line: usize) -> Stmt {
    let tmp = format!("__unpack{}", line);
    let mut stmts: Block = Vec::new();
    stmts.push(Stmt::Let { name: tmp.clone(), ty: None, value, mutable: false, line });
    for (i, n) in names.into_iter().enumerate() {
        let base = Expr::new(ExprKind::Ident(tmp.clone()), line);
        let field = Expr::new(ExprKind::Field(Box::new(base), i.to_string()), line);
        stmts.push(Stmt::Assign { name: n, index: None, op: None, value: field, line });
    }
    Stmt::Block(stmts)
}

/// 把 `a | b | c` 模式拆成 [a, b, c]（仅顶层 BitOr）。
fn split_or_pattern(e: &Expr) -> Vec<Expr> {
    match &e.kind {
        ExprKind::Binary(BinOp::BitOr, a, b) => {
            let mut v = split_or_pattern(a);
            v.extend(split_or_pattern(b));
            v
        }
        _ => vec![e.clone()],
    }
}
