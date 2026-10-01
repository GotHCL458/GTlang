use super::*;

/// 赋值运算符 → 对应的复合运算（`=` 映射为 `Some(None)`，非赋值返回 None）
pub(crate) fn assign_op(tok: &Tok) -> Option<Option<BinOp>> {
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
pub(crate) fn prec_of(op: BinOp) -> u8 {
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
pub(crate) fn interp(raw: &str, line: usize) -> Result<ExprKind, String> {
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
pub(crate) fn desugar_destructure(names: Vec<String>, value: Expr, mutable: bool, line: usize) -> Stmt {
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
pub(crate) fn desugar_unpack_assign(names: Vec<String>, value: Expr, line: usize) -> Stmt {
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
pub(crate) fn split_or_pattern(e: &Expr) -> Vec<Expr> {
    match &e.kind {
        ExprKind::Binary(BinOp::BitOr, a, b) => {
            let mut v = split_or_pattern(a);
            v.extend(split_or_pattern(b));
            v
        }
        _ => vec![e.clone()],
    }
}
