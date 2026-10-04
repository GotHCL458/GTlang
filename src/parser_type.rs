//! Parser 的类型解析。

use super::*;

impl Parser {
    pub(crate) fn parse_type(&mut self) -> Result<Ty, String> {
        self.enter_depth()?;
        let r = self.parse_type_inner();
        self.leave_depth();
        r
    }

    fn parse_type_inner(&mut self) -> Result<Ty, String> {
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
        // `impl Trait`（返回位置）：擦除为 trait 对象（等价 dyn Trait）
        if self.eat_ident("impl") {
            let tname = self.ident("trait 名")?;
            return Ok(Ty::Dyn(tname));
        }
        // 元组类型 (T1, T2, ...)
        if self.at_punct("(") {
            self.bump();
            let mut ts: Vec<Ty> = Vec::new();
            if !self.at_punct(")") {
                loop {
                    ts.push(self.parse_type()?);
                    if self.eat_punct(",") {
                        if self.at_punct(")") { break; }
                        continue;
                    }
                    break;
                }
            }
            self.expect_punct(")")?;
            if ts.len() == 1 { return Ok(ts.into_iter().next().unwrap()); }
            return Ok(Ty::Tuple(ts));
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
            // 关联类型 `T::Item`：复用 Generic（mono 单态化时替换为具体类型）
            if self.eat_punct("::") {
                let assoc = self.ident("关联类型名")?;
                return Ok(Ty::Generic(format!("{}::{}", name, assoc)));
            }
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
                "Option" | "option" => Ty::Option(Box::new(first)),
                _ => {
                    // 用户泛型 struct/别名（如 `盒[T]`）：保留外层名字，
                    // 参数由 mono 的单态化从字面量字段推导。此前返回内层 T 是错的。
                    let _ = first;
                    Ty::Struct(name.clone())
                }
            };
            self.expect_punct("]")?;
            return Ok(ty);
        }
        Ok(Ty::from_name(&name).unwrap_or_else(|| Ty::Struct(name)))
    }
}

