//! Parser 的顶层项解析（函数/结构体/枚举/trait/import）。

use super::*;

impl Parser {
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
                let mut assoc_bind: Vec<(String, Ty)> = Vec::new();
                loop {
                    self.skip_line_comment();
                    if self.at_punct("}") {
                        break;
                    }
                    if self.at_eof() {
                        return Err("impl 块缺少 '}'".into());
                    }
                    let mpub = self.eat_ident("pub");
                    // 关联类型绑定：`type Item = X`
                    if self.eat_ident("type") {
                        let an = self.ident("关联类型名")?;
                        self.expect_punct("=")?;
                        let at = self.parse_type()?;
                        self.eat_punct(";");
                        self.eat_punct(",");
                        assoc_bind.push((an, at));
                        continue;
                    }
                    if !self.eat_ident("fn") {
                        return Err(crate::lb!(self.line(), "impl block only supports fn/type", "impl 块内只支持 fn/type"));
                    }
                    methods.push(self.fn_def(mpub)?);
                }
                self.expect_punct("}")?;
                self.type_params = saved_tp;
                if let Some(tn) = trait_name {
                    items.push(Item::TraitImpl { type_params: impl_tps, trait_name: tn, ty, methods, assoc_bind, line });
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
    pub(crate) fn import_decl(&mut self) -> Result<Import, String> {
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
    pub(crate) fn trait_def(&mut self, is_pub: bool) -> Result<TraitDef, String> {
        let line = self.line();
        let name = self.ident("trait 名")?;
        self.expect_punct("{")?;
        let mut methods = Vec::new();
        let mut defaults: Vec<(String, Vec<(String, Ty)>, Ty, Block)> = Vec::new();
        let mut assoc: Vec<String> = Vec::new();
        loop {
            self.skip_line_comment();
            if self.at_punct("}") {
                break;
            }
            if self.at_eof() {
                return Err("trait 块缺少 '}'".into());
            }
            self.eat_ident("pub");
            // 关联类型声明：`type Item`
            if self.eat_ident("type") {
                let an = self.ident("关联类型名")?;
                self.eat_punct(";");
                self.eat_punct(",");
                assoc.push(an);
                continue;
            }
            if !self.eat_ident("fn") {
                return Err(crate::lb!(self.line(), "trait block only supports fn signatures", "trait 块内只支持 fn 签名"));
            }
            let mname = self.ident("方法名")?;
            self.expect_punct("(")?;
            let mut ptypes = Vec::new();
            let mut pnames: Vec<(String, Ty)> = Vec::new();
            let mut first_is_self = false;
            while !self.at_punct(")") {
                if self.at_punct("&") {
                    self.bump();
                    self.eat_ident("mut");
                }
                let pn = self.ident("参数名")?;
                if ptypes.is_empty() && pn == "self" { first_is_self = true; }
                let ty = if self.eat_punct(":") { self.parse_type()? } else { Ty::I64 };
                ptypes.push(ty.clone());
                pnames.push((pn, ty));
                if !self.eat_punct(",") {
                    break;
                }
            }
            self.expect_punct(")")?;
            // trait 方法第一个参数必须是 self
            if !first_is_self && !self.at_punct("{") {
                return Err(crate::lb!(self.line(), "trait method '{}' must take 'self' as first parameter", "trait 方法 '{}' 的第一个参数必须是 'self'", mname));
            }
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
        Ok(TraitDef { name, methods, defaults, assoc, line, is_pub })
    }

    /// `struct Name { f: T, ... }`（字段类型可省略）
    /// `enum 名 { V1(T1, T2) V2 ... }`
    pub(crate) fn enum_def(&mut self, is_pub: bool, derives: Vec<String>) -> Result<EnumDef, String> {
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

    pub(crate) fn struct_def(&mut self, is_pub: bool, derives: Vec<String>) -> Result<StructDef, String> {
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

    pub(crate) fn fn_def(&mut self, is_pub: bool) -> Result<FnDef, String> {
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

}

