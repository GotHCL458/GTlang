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
#[path = "parser_type.rs"]
mod parser_type;
#[path = "parser_stmt.rs"]
mod parser_stmt;
#[path = "parser_expr.rs"]
mod parser_expr;
#[path = "parser_util.rs"]
mod parser_util;
pub(crate) use parser_util::*;
