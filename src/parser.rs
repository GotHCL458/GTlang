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
    let mut p = Parser { toks, pos: 0, src_line_base: 0, no_struct_lit: false, type_params: Vec::new(), depth: 0 };
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
    let mut p = Parser { toks, pos: 0, src_line_base: line, no_struct_lit: false, type_params: Vec::new(), depth: 0 };
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
    /// 递归深度（防止深嵌套导致栈溢出）
    depth: usize,
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

    /// 递归深度检查（防深嵌套栈溢出）。返回 Err 时应向上传播。
    fn enter_depth(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > 128 {
            return Err(crate::lb!(self.line(), "expression/block nesting too deep (max 128)", "表达式/块的嵌套过深（上限 128）"));
        }
        Ok(())
    }
    fn leave_depth(&mut self) { self.depth = self.depth.saturating_sub(1); }

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
            Tok::Decimal(s) => format!("十进制 {}", s),
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
}

#[path = "parser_item.rs"]
mod parser_item;

#[cfg(test)]
mod parser_tests {

use super::parse_program;

fn ok(src: &str) {
    if let Err(e) = parse_program(src) {
        panic!("should parse: {} --- {}", e.msg, src);
    }
}

fn err(src: &str, needle: &str) {
    match parse_program(src) {
        Ok(_) => panic!("should fail: {}", src),
        Err(e) => assert!(e.msg.contains(needle), "expected {:?} in {:?}", needle, e.msg),
    }
}

#[test]
fn basic_fn_parses() {
    ok("fn main() { put(1) }");
    ok("fn 加(a: int, b: int) -> int { return a + b }");
    ok("fn main() { x := 1 + 2 * 3 }");
}

#[test]
fn closures_parse() {
    ok("fn main() { f := |x: int| x + 1 }");
    ok("fn main() { f := |a: int, b: int| a * b }");
    ok("fn main() { f := |x: int| -> int { x * x } }");
}

#[test]
fn any_expression_as_callee_parses() {
    ok("fn main() { fs := list()  put(fs[0](1)) }");
    ok("fn main() { f := |x: int| x  put((f)(1)) }");
}

#[test]
fn first_class_fn_name_parses() {
    ok("fn 加一(x: int) -> int { x + 1 }  fn main() { g := 加一  put(g(1)) }");
    ok("fn 加一(x: int) -> int { x + 1 }  fn main() { put(应用(加一, 1)) }");
}

#[test]
fn generic_and_where_parse() {
    ok("fn 恒等[T](x: T) -> T { x }");
    ok("fn max[T](a: T, b: T) -> T where T: Ord { a }");
    ok("fn f[T: A + B](x: T) -> int { 0 }");
}

#[test]
fn struct_enum_trait_parse() {
    ok("struct 点 { x: int  y: int }");
    ok("enum 形状 { Circle(f64) Rect(f64, f64) Unit }");
    ok("trait 面积 { fn area(self) -> f64 }");
    ok("impl 面积 for 圆 { fn area(self) -> f64 { 1.0 } }");
}

#[test]
fn control_flow_parses() {
    ok("fn main() { if true { put(1) } elif false { put(2) } else { put(3) } }");
    ok("fn main() { i := 0  while i < 3 { i = i + 1 } }");
    ok("fn main() { loop 3 { put(1) } }");
    ok("fn main() { for i in 0..10 { put(i) } }");
    ok("fn main() { for v in list() { put(v) } }");
}

#[test]
fn match_parses() {
    ok("fn main() { m := match 1 { 1 => { put(1) }  _ => { put(2) } } }");
    ok("fn main() { m := match 5 { 1..10 => { put(1) }  _ => { put(2) } } }");
}

#[test]
fn loop_without_count_reports_friendly() {
    err("fn main() { loop { put(1) } }", "loop");
}

#[test]
fn unbalanced_reports_error() {
    // 缺 '}' 时报"文件意外结束"（友好提示）
    err("fn main() { put(1)", "意外结束");
    err("fn main() {", "意外结束");
}

#[test]
fn deeply_nested_is_rejected_not_overflow() {
    // 300 层括号应报错而非爆栈
    let src = format!("fn main() {{ x := {}1{} }}", "(".repeat(300), ")".repeat(300));
    err(&src, "deep");
}

#[test]
fn long_binary_chain_is_rejected() {
    let src = format!("fn main() {{ x := {}1 }}", "1 + ".repeat(500));
    err(&src, "deep");
}


}
#[path = "parser_type.rs"]
mod parser_type;
#[path = "parser_stmt.rs"]
mod parser_stmt;
#[path = "parser_expr.rs"]
mod parser_expr;
#[path = "parser_util.rs"]
mod parser_util;
pub(crate) use parser_util::*;

