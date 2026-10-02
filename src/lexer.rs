//! GTLang 词法分析器。
//!
//! 支持：中文/任意 Unicode 标识符、`_` 数字分隔符、0x/0b/0o 进制、
//! 浮点与指数、带 `{}` 插值的字符串、`//` 注释（与整除运算符消歧）。

use crate::ast::{ParseError, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Int(i64),
    Float(f64),
    /// 普通字符串：内容已解转义，其中的 `$name` / `${expr}` 由语法阶段展开
    Str(String),
    /// 原始字符串 `r"..."`：不解转义、不做插值
    RawStr(String),
    Punct(String),
    Eof,
}

/// 标识符首字符：字母 / `_` / 任意非 ASCII（含中文）
pub fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_alphabetic() || (ch as u32) > 0x7F
}

/// 标识符后续字符
pub fn is_ident_cont(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric() || (ch as u32) > 0x7F
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    /// 该 token 之前是否有换行（用于 `+`/`-` 换行歧义消解）
    pub nl_before: bool,
    /// 源码字节区间
    pub span: Span,
}

pub const KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "const", "if", "else", "while", "for", "in", "return", "break",
    "continue", "true", "false", "pub", "import", "as", "mod", "elif", "struct", "impl",
    "match", "extern", "trait", "do", "do", "do", "do", "do", "do",
    // 异常控制流（try/expt/fily 与 try/except/finally 互为别名）
    "try", "expt", "except", "fily", "finally", "throw", "raise",
    // 兜底
    // 内联汇编
    // 泛型约束
    "where",
];

pub fn is_keyword(s: &str) -> bool {
    KEYWORDS.contains(&s)
}

/// 多字符运算符按长度倒序排列，保证最长匹配
const OPS: &[&str] = &[
    "|>", "<<=", ">>=", "&=", "|=", "^=", ":=", "::", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=",
    "*=", "/=", "%=", "..", "->", "=>", "//", "<<", ">>", "++", "--", "+", "-", "*", "/", "%", "(", ")", "{", "}",
    "[", "]", ",", ":", ";", ".", "=", "<", ">", "!", "&", "|", "^", "~", "?", "@",
];

pub fn lex(src: &str) -> Result<Vec<Token>, ParseError> {
    Lexer::new(src, true).run()
}

/// 词法分析表达式片段（字符串插值 `{...}` 内部）。
/// 该模式下不识别注释，使 `{a // b}` 能按整除运算符解析，
/// 避免 `//` 被当注释吞掉后静默产生错误结果。
pub fn lex_expr_text(src: &str) -> Result<Vec<Token>, ParseError> {
    Lexer::new(src, false).run()
}

struct Lexer {
    c: Vec<char>,
    /// char 下标 -> 字节偏移（尾部含一个哨兵）
    off: Vec<usize>,
    i: usize,
    line: usize,
    /// 是否识别 `#` 行注释
    comments: bool,
    /// 宽容模式：位于同一行的 `//` 之后时开启。
    ///
    /// `//` 既可能是整除运算符、也可能是行注释起始，词法阶段无法判定，于是把
    /// 两种可能都切出来交给语法阶段。但注释正文里什么字符都可能有（`\`、`$`…），
    /// 因此 `//` 之后的同行内容遇到无法识别的字符时按单字符吞掉而不报错。
    lenient: bool,
}

impl Lexer {
    fn new(src: &str, comments: bool) -> Self {
        let mut c = Vec::new();
        let mut off = Vec::new();
        let mut b = 0usize;
        for ch in src.chars() {
            c.push(ch);
            off.push(b);
            b += ch.len_utf8();
        }
        off.push(b); // 哨兵：允许用 end==len 表示区间结尾
        Lexer { c, off, i: 0, line: 1, comments, lenient: false }
    }

    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.c.get(self.i + n).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.i += 1;
        if ch == '\n' {
            self.line += 1;
        }
        Some(ch)
    }

    /// 判断是否为标识符首字符：字母 / `_` / 任意非 ASCII（含中文）
    fn is_ident_start(ch: char) -> bool {
        is_ident_start(ch)
    }

    fn is_ident_cont(ch: char) -> bool {
        is_ident_cont(ch)
    }

    fn run(mut self) -> Result<Vec<Token>, ParseError> {
        let mut out: Vec<Token> = Vec::new();
        loop {
            // 跳空白，并记录是否跨行
            let mut nl = false;
            loop {
                match self.peek() {
                    Some('\n') => {
                        nl = true;
                        self.bump();
                    }
                    Some(ch) if ch.is_whitespace() => {
                        self.bump();
                    }
                    _ => break,
                }
            }

            let line = self.line;
            let start_i = self.i;
            // 换行即退出宽容模式（`//` 的影响只限同一行）
            if nl {
                self.lenient = false;
            }
            let ch = match self.peek() {
                Some(c) => c,
                None => {
                    out.push(Token {
                        tok: Tok::Eof,
                        line,
                        nl_before: nl,
                        span: self.span_at(start_i),
                    });
                    break;
                }
            };

            // `#` 行注释：任何位置都认（不会与运算符冲突）。
            // 插值子表达式里关闭（comments=false），避免 `{a # b}` 被截断。
            if self.comments && ch == '#' {
                while let Some(c) = self.peek() {
                    if c == '\n' {
                        break;
                    }
                    self.bump();
                }
                continue;
            }

            // 注意：`//` 不再在这里当注释，一律作为整除运算符交给语法阶段。
            // 语法阶段只在「语句/顶层位置」把 `//` 到行尾当注释跳过，
            // 这样 `x := a // 5` 会真正做整除（旧规则会静默变成 `x := a`）。
            let is_raw = ch == 'r' && self.peek_at(1) == Some('"');
            let tok = if Self::is_ident_start(ch) && !is_raw {
                let start = self.i;
                while let Some(c) = self.peek() {
                    if Self::is_ident_cont(c) {
                        self.bump();
                    } else {
                        break;
                    }
                }
                Tok::Ident(self.c[start..self.i].iter().collect())
            } else {
                // 各 helper 内部仍返回 String；错误区间统一取「出错 token 的起始位置」
                let r = if is_raw && !self.lenient {
                    self.lex_raw_string(line)
                } else if ch.is_ascii_digit() {
                    self.lex_number()
                } else if ch == '"' && !self.lenient {
                    // lenient（`//` 之后）时不启动字符串扫描：
                    // 注释正文里可能只有一个孤立的引号，会吞掉后续多行
                    self.lex_string(line)
                } else if ch == '\'' && !self.lenient {
                    self.lex_char()
                } else if ch == '/' && self.peek_at(1) == Some('/') {
                    // 同行出现 `//` → 之后的同行内容进入宽容模式，
                    // 可能是行注释正文，也可能仍是整除（留给语法阶段判定）
                    self.lenient = true;
                    self.lex_punct(line)
                } else {
                    self.lex_punct(line)
                };
                match r {
                    Ok(t) => t,
                    Err(m) => return Err(ParseError::new(m, self.span_at(start_i))),
                }
            };

            out.push(Token {
                tok,
                line,
                nl_before: nl,
                span: self.span_at(start_i),
            });
        }
        Ok(out)
    }

    /// 以 char 下标 i 为首字符的字节区间
    fn span_at(&self, i: usize) -> Span {
        let last = self.off.len() - 1;
        let s = self.off[i.min(last)];
        let e = self.off[(i + 1).min(last)];
        Span::new(s, e.max(s + 1))
    }

    fn lex_number(&mut self) -> Result<Tok, String> {
        let start = self.i;
        // 进制前缀
        if self.peek() == Some('0') {
            match self.peek_at(1) {
                Some('x') | Some('X') | Some('b') | Some('B') | Some('o') | Some('O') => {
                    let radix = match self.peek_at(1).unwrap() {
                        'x' | 'X' => 16u32,
                        'b' | 'B' => 2,
                        _ => 8,
                    };
                    self.bump();
                    self.bump();
                    let ds = self.i;
                    while let Some(c) = self.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    let text: String =
                        self.c[ds..self.i].iter().filter(|c| **c != '_').collect();
                    let v = i64::from_str_radix(&text, radix)
                        .map_err(|_| crate::lb!(self.line, "invalid numeric literal '0{}{}'", "非法数字字面量 '0{}{}'",
                            if radix == 16 { "x" } else if radix == 2 { "b" } else { "o" }, text))?;
                    return Ok(Tok::Int(v));
                }
                _ => {}
            }
        }

        let mut is_float = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '_' {
                self.bump();
            } else {
                break;
            }
        }
        // 小数点：`.5` 与 `..`（区间）需区分。
        // 贪婪吃掉后续的 `.` + 数字段，使 `0.1.0` 成为单个 token
        // 并在 parse::<f64>() 处报"非法浮点字面量"（而非被切成字段访问）。
        while self.peek() == Some('.')
            && self.peek_at(1).map_or(false, |c| c.is_ascii_digit())
        {
            is_float = true;
            self.bump();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || c == '_' {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        // 指数
        if matches!(self.peek(), Some('e') | Some('E')) {
            let sign = matches!(self.peek_at(1), Some('+') | Some('-'));
            let digit_at = if sign { 2 } else { 1 };
            if self.peek_at(digit_at).map_or(false, |c| c.is_ascii_digit()) {
                is_float = true;
                self.bump();
                if sign {
                    self.bump();
                }
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() || c == '_' {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
        }

        let text: String = self.c[start..self.i].iter().filter(|c| **c != '_').collect();
        if is_float {
            text.parse::<f64>()
                .map(Tok::Float)
                .map_err(|_| crate::lb!(self.line, "invalid float literal '{}'", "非法浮点字面量 '{}'", text))
        } else {
            match text.parse::<i64>() {
                Ok(v) => Ok(Tok::Int(v)),
                // 允许 i64::MIN 的绝对值（9223372036854775808）；parser 的一元负号特判处理
                Err(_) if text == "9223372036854775808" => Ok(Tok::Int(i64::MIN)),
                Err(_) => Err(crate::lb!(self.line, "integer overflow '{}'", "整数溢出 '{}'", text)),
            }
        }
    }

    /// `r"..."`：原始字符串，不解转义、不做插值（因此内部不能含 `"`）
    fn lex_raw_string(&mut self, line: usize) -> Result<Tok, String> {
        self.bump(); // r
        self.bump(); // 开引号
        let mut s = String::new();
        loop {
            match self.bump() {
                None => return Err(crate::lb!(line, "unterminated raw string", "原始字符串未闭合")),
                Some('"') => break,
                Some(c) => s.push(c),
            }
        }
        Ok(Tok::RawStr(s))
    }

    fn lex_string(&mut self, line: usize) -> Result<Tok, String> {
        self.bump(); // 开引号
        let mut s = String::new();
        loop {
            match self.bump() {
                None => return Err(crate::lb!(line, "unterminated string", "字符串未闭合")),
                Some('"') => break,
                Some('\\') => {
                    let e = self
                        .bump()
                        .ok_or_else(|| crate::lb!(line, "unterminated string", "字符串未闭合"))?;
                    s.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '0' => '\0',
                        'e' => '\u{1b}', // ESC：便于手写 ANSI 彩色序列
                        // `\$` 需要字面 `$`。这里保留反斜杠，交给语法阶段的插值扫描器
                        // 处理——否则 `\$name` 会先变成 `$name` 又被插值，转义失效。
                        '$' => {
                            s.push('\\');
                            s.push('$');
                            continue;
                        }
                        '\\' => '\\',
                        '"' => '"',
                        '\'' => '\'',
                        '{' => '{',
                        '}' => '}',
                        other => {
                            return Err(crate::lb!(line, "unknown escape '\\{}'", "未知转义 '\\{}'", other))
                        }
                    });
                }
                Some(c) => s.push(c),
            }
        }
        Ok(Tok::Str(s))
    }

    fn lex_char(&mut self) -> Result<Tok, String> {
        self.bump(); // 开引号
        let line = self.line;
        let v = match self.bump() {
            None => return Err(crate::lb!(line, "unterminated char literal", "字符字面量未闭合")),
            Some('\\') => match self.bump() {
                Some('n') => '\n' as i64,
                Some('t') => '\t' as i64,
                Some('0') => 0,
                Some('\\') => '\\' as i64,
                Some('\'') => '\'' as i64,
                Some(other) => other as i64,
                None => return Err(crate::lb!(line, "unterminated char literal", "字符字面量未闭合")),
            },
            Some(c) => c as i64,
        };
        match self.bump() {
            Some('\'') => Ok(Tok::Int(v)),
            _ => Err(crate::lb!(line, "char literal must be a single character", "字符字面量应为单字符")),
        }
    }

    fn lex_punct(&mut self, line: usize) -> Result<Tok, String> {
        let rest: String = self.c[self.i..].iter().take(3).collect();
        for op in OPS {
            if rest.starts_with(op) {
                for _ in 0..op.chars().count() {
                    self.bump();
                }
                return Ok(Tok::Punct((*op).to_string()));
            }
        }
        // 宽容模式（`//` 之后的同行内容）：注释正文里什么字符都可能有，
        // 按单字符吞掉，不因无法识别而报错；语义上的错误留给语法阶段。
        if self.lenient {
            if let Some(c) = self.bump() {
                return Ok(Tok::Punct(c.to_string()));
            }
        }
        Err(crate::lb!(line, "unrecognized character '{}'", "无法识别的字符 '{}'", self.peek().unwrap_or('?')))
    }
}