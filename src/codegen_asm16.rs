//! 16 位（x86 real mode）后端：AST -> 机器码（.bin）。
//!
//! 支持的子集（够写小内核/引导程序）：
//! - 顶层 `fn`（无参/有参、返回值）、`put`（串口 COM1）
//! - `int` 变量、算术（+ - * / %）、比较、位运算
//! - `if` / `while` / `for i in a..b`（常量边界）
//! - `return`、函数调用（含递归）
//!
//! 约定：16 位实模式、栈式表达式求值、`ORG 0x7C00`、结果以 `hlt` 收尾。

use crate::ast::*;
use std::collections::HashMap;

const ORG: i64 = 0x7C00;

pub struct Asm16 {
    pub out: Vec<u8>,
    labels: HashMap<String, i64>,
    fixups: Vec<(usize, String, FixKind)>,
    fn_addr: HashMap<String, i64>,
    #[allow(dead_code)] entry: Option<String>,
    /// 字符串常量（内容 -> 在数据段的标签名）
    strs: Vec<(String, Vec<u8>)>,
    /// 标签序号（保证唯一）
    seq: usize,
}

#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)]
enum FixKind { Rel8From(i64), Rel16From(i64), Abs16 }


/// 生成整个程序：顶层函数 + 入口（`main` 或 `_start`）。
pub fn compile(prog: &Program) -> Result<Vec<u8>, String> {
    let fns: Vec<&FnDef> = prog.items.iter().filter_map(|it| match it { Item::Fn(f) => Some(f), _ => None }).collect();
    if fns.is_empty() { return Err("没有可编译的函数".into()); }
    let mut a = Asm16::new();
    // 入口在前：call main; hlt（相对位移用标签回填）
    let entry = if fns.iter().any(|f| f.name == "main") { "main".to_string() } else { fns[fns.len() - 1].name.clone() };
    // 初始化 COM1（0x3F8）：8N1 + FIFO
    a.b(0xBA); a.w(0x3FB); a.b(0xB0); a.b(0x80); a.b(0xEE);  // mov dx,3FB; mov al,80h; out dx,al
    a.b(0xBA); a.w(0x3F8); a.b(0xB0); a.b(0x03); a.b(0xEE);  // mov dx,3F8; mov al,3; out dx,al
    a.b(0xBA); a.w(0x3F9); a.b(0xB0); a.b(0x00); a.b(0xEE);  // mov dx,3F9; mov al,0; out dx,al
    a.b(0xBA); a.w(0x3FB); a.b(0xB0); a.b(0x03); a.b(0xEE);  // mov dx,3FB; mov al,3; out dx,al
    a.b(0xBA); a.w(0x3FA); a.b(0xB0); a.b(0xC7); a.b(0xEE);  // mov dx,3FA; mov al,C7h; out dx,al
    a.b(0xBA); a.w(0x3FC); a.b(0xB0); a.b(0x0B); a.b(0xEE);  // mov dx,3FC; mov al,0Bh; out dx,al
    a.b(0xE8);
    let at = a.out.len();
    a.w(0);
    let from = a.pc();
    a.fixups.push((at, format!("fn__{}", entry), FixKind::Rel16From(from)));
    // 停机：cli; hlt; jmp $-1
    a.b(0xFA);
    a.b(0xF4);
    a.b(0xEB); a.b(0xFE);
    // 生成所有函数
    for f in &fns {
        let addr = a.pc();
        a.fn_addr.insert(f.name.clone(), addr);
        a.label(&format!("fn__{}", f.name));
        gen_fn(&mut a, f)?;
    }
    // 发射字符串常量（数据段）
    let strs = a.strs.clone();
    for (lab, bytes) in strs {
        a.label(&lab);
        for b in bytes { a.b(b); }
    }
    resolve_fixups(&mut a)?;
    // 补 MBR 引导签名（0xAA55 @ 510）
    while a.out.len() < 510 { a.out.push(0); }
    a.out.push(0x55);
    a.out.push(0xAA);
    Ok(a.out)
}

fn resolve_fixups(a: &mut Asm16) -> Result<(), String> {
    let fx = a.fixups.clone();
    for (at, target, kind) in fx {
        let tgt = *a.labels.get(&target).ok_or_else(|| format!("未定义标签 '{}'", target))?;
        match kind {
            FixKind::Rel8From(from) => {
                let rel = (tgt - from) as i8;
                a.out[at] = rel as u8;
            }
            FixKind::Rel16From(from) => {
                let rel = (tgt - from) as u16;
                a.out[at] = (rel & 0xFF) as u8;
                a.out[at + 1] = ((rel >> 8) & 0xFF) as u8;
            }
            FixKind::Abs16 => {
                a.out[at] = (tgt as u16 & 0xFF) as u8;
                a.out[at + 1] = ((tgt as u16 >> 8) & 0xFF) as u8;
            }
        }
    }
    Ok(())
}

/// 变量表：名字 -> (bp 相对偏移)
#[allow(dead_code)]
struct Scope { vars: HashMap<String, i64>, base: i64, next: i64, breaks: Vec<String>, continues: Vec<String> }

/// 统计函数体内的局部变量数（Let/Const/ForRange 变量），用于预留栈帧。
fn count_locals(b: &Block) -> usize {
    let mut n = 0;
    for s in b {
        match s {
            Stmt::Let { .. } | Stmt::Const { .. } => n += 1,
            Stmt::ForRange { body, .. } => { n += 1 + count_locals(body); }
            Stmt::If { then, els, .. } => { n += count_locals(then); if let Some(e) = els { n += count_locals(e); } }
            Stmt::While { body, .. } | Stmt::DoWhile { body, .. } | Stmt::Block(body) => { n += count_locals(body); }
            _ => {}
        }
    }
    n
}
fn gen_fn(a: &mut Asm16, f: &FnDef) -> Result<(), String> {
    a.b(0x55);            // push bp
    a.b(0x89); a.b(0xE5); // mov bp, sp
    // 预扫描局部变量数，预留栈帧（每个 2 字节，留余量）
    let nlocals = count_locals(&f.body);
    let frame = ((nlocals + 8) * 2) as i64;
    a.b(0x83); a.b(0xEC); a.b(frame as u8); // sub sp, frame
    let mut sc = Scope { vars: HashMap::new(), base: 4, next: -2, breaks: Vec::new(), continues: Vec::new() };
    // 参数在 [bp+4], [bp+6], ...
    for (i, p) in f.params.iter().enumerate() {
        sc.vars.insert(p.name.clone(), 4 + (i as i64) * 2);
    }
    gen_block(a, &mut sc, &f.body)?;
    a.b(0x89); a.b(0xEC); // mov sp, bp
    a.b(0x5D);           // pop bp
    a.b(0xC3);           // ret
    Ok(())
}


fn gen_block(a: &mut Asm16, sc: &mut Scope, b: &Block) -> Result<(), String> {
    for s in b { gen_stmt(a, sc, s)?; }
    Ok(())
}

fn alloc(sc: &mut Scope) -> i64 { let off = sc.next; sc.next -= 2; off }

fn gen_stmt(a: &mut Asm16, sc: &mut Scope, s: &Stmt) -> Result<(), String> {
    match s {
        Stmt::Let { name, value, .. } | Stmt::Const { name, value, .. } => {
            gen_expr(a, sc, value)?;      // 结果在 AX
            let off = alloc(sc);
            store_ax(a, off);
            sc.vars.insert(name.clone(), off);
        }
        Stmt::Assign { name, value, index: None, op, .. } => {
            let off = *sc.vars.get(name).ok_or_else(|| format!("未定义变量 '{}'", name))?;
            gen_expr(a, sc, value)?;
            if let Some(_o) = op { /* += 等：先取旧值再运算（暂略） */ }
            store_ax(a, off);
        }
        Stmt::Expr(e) => { gen_expr(a, sc, e)?; }
        Stmt::Return(Some(e), _) => { gen_expr(a, sc, e)?; ret(a); }
        Stmt::Return(None, _) => { ret(a); }
        Stmt::If { cond, then, els, .. } => {
            let lelse = a.new_label("Lelse");
            let lend = a.new_label("Lend");
            gen_expr(a, sc, cond)?;
            cmp_ax_0(a);
            a.rel8(0x74, &lelse); // je else
            gen_block(a, sc, then)?;
            if els.is_some() { a.b(0xE9); let at = a.out.len(); a.w(0); let f = a.pc(); a.fixups.push((at, lend.clone(), FixKind::Rel16From(f))); }
            a.label(&lelse);
            if let Some(e) = els { gen_block(a, sc, e)?; a.label(&lend); }
        }
        Stmt::While { cond, body, .. } => {
            let ltop = a.new_label("Ltop");
            let lend = a.new_label("Lend");
            a.label(&ltop);
            gen_expr(a, sc, cond)?;
            cmp_ax_0(a);
            a.rel8(0x74, &lend);
            sc.breaks.push(lend.clone()); sc.continues.push(ltop.clone());
            gen_block(a, sc, body)?;
            sc.breaks.pop(); sc.continues.pop();
            a.b(0xE9); let atw = a.out.len(); a.w(0); let fw = a.pc(); a.fixups.push((atw, ltop.clone(), FixKind::Rel16From(fw)));
            a.label(&lend);
        }
        // for i in a..b：i 从 a 到 b-1
        Stmt::ForRange { var, from, to, body, .. } => {
            let ltop = a.new_label("Lfr");
            let lend = a.new_label("Lfe");
            gen_expr(a, sc, from)?;
            let off = alloc(sc);
            store_ax(a, off);
            sc.vars.insert(var.clone(), off);
            a.label(&ltop);
            load_ax(a, off);
            push_ax(a);
            gen_expr(a, sc, to)?;
            a.b(0x89); a.b(0xC3);        // mov bx, ax
            pop_ax(a);
            a.b(0x39); a.b(0xD8);        // cmp ax, bx
            a.b(0x0F); a.b(0x8D);       // jge rel16
            let at = a.out.len(); a.w(0);
            let frompc = a.pc();
            a.fixups.push((at, lend.clone(), FixKind::Rel16From(frompc)));
            // continue 跳到"自增处"（linc）
            let linc = a.new_label("Linc");
            sc.breaks.push(lend.clone()); sc.continues.push(linc.clone());
            gen_block(a, sc, body)?;
            sc.breaks.pop(); sc.continues.pop();
            a.label(&linc);
            load_ax(a, off);
            a.b(0x40);                   // inc ax
            store_ax(a, off);
            // 回跳可能超 rel8：用 rel16
            a.b(0xE9);
            let atb = a.out.len(); a.w(0);
            let fb = a.pc();
            a.fixups.push((atb, ltop.clone(), FixKind::Rel16From(fb)));
            a.label(&lend);
        }
        Stmt::DoWhile { body, cond, .. } => {
            let ltop = a.new_label("Ldw");
            let lend = a.new_label("Lde");
            a.label(&ltop);
            sc.breaks.push(lend.clone()); sc.continues.push(ltop.clone());
            gen_block(a, sc, body)?;
            sc.breaks.pop(); sc.continues.pop();
            gen_expr(a, sc, cond)?;
            cmp_ax_0(a);
            a.b(0x0F); a.b(0x85);        // jne rel16
            let at = a.out.len(); a.w(0);
            let frompc = a.pc();
            a.fixups.push((at, ltop.clone(), FixKind::Rel16From(frompc)));
        }
        Stmt::Break(..) => {
            let t = sc.breaks.last().cloned().ok_or_else(|| "break 不在循环中".to_string())?;
            // 用 rel16 跳转（目标可能较远）
            a.b(0xE9);
            let at = a.out.len(); a.w(0);
            let frompc = a.pc();
            a.fixups.push((at, t, FixKind::Rel16From(frompc)));
        }
        Stmt::Continue(..) => {
            let t = sc.continues.last().cloned().ok_or_else(|| "continue 不在循环中".to_string())?;
            a.b(0xE9);
            let at = a.out.len(); a.w(0);
            let frompc = a.pc();
            a.fixups.push((at, t, FixKind::Rel16From(frompc)));
        }
        Stmt::Block(inner) => gen_block(a, sc, inner)?,
        _ => return Err(format!("16 位后端暂不支持该语句")),
    }
    Ok(())
}

/// AX 入栈 / 出栈（供表达式求值）
fn push_ax(a: &mut Asm16) { a.b(0x50); }
fn pop_bx(a: &mut Asm16) { a.b(0x5B); }
fn pop_ax(a: &mut Asm16) { a.b(0x58); }

fn store_ax(a: &mut Asm16, off: i64) {
    // mov [bp+off], ax
    a.b(0x89); a.b(0x46); a.b(off as u8);
}
fn load_ax(a: &mut Asm16, off: i64) {
    a.b(0x8B); a.b(0x46); a.b(off as u8);
}
fn cmp_ax_0(a: &mut Asm16) { a.b(0x3D); a.w(0); } // cmp ax, 0
fn ret(a: &mut Asm16) { a.b(0x89); a.b(0xEC); a.b(0x5D); a.b(0xC3); } // mov sp,bp; pop bp; ret


/// 字符串常量表：内容 -> 标签名（在数据段末尾统一发射）
fn str_label(s: &str) -> String {
    let mut h: u32 = 2166136261;
    for b in s.bytes() { h = (h ^ b as u32).wrapping_mul(16777619); }
    format!("str_{:08x}", h)
}
fn gen_expr(a: &mut Asm16, sc: &mut Scope, e: &Expr) -> Result<(), String> {
    match &e.kind {
        ExprKind::Int(v) => { a.b(0xB8); a.w(*v); }
        ExprKind::Bool(v) => { a.b(0xB8); a.w(if *v { 1 } else { 0 }); }
        ExprKind::Str(s) => {
            // 字符串常量：登记到数据段，运行时 AX = 其地址
            let lab = str_label(s);
            if !a.strs.iter().any(|(l, _)| l == &lab) {
                let mut bytes = s.as_bytes().to_vec();
                bytes.push(0);
                a.strs.push((lab.clone(), bytes));
            }
            a.b(0xB8);
            let at = a.out.len(); a.w(0);
            a.fixups.push((at, lab, FixKind::Abs16));
        }
        ExprKind::Index(base, idx) => {
            // 字符串/数组按字节取（字符串）或按 2 字节（数组）取
            gen_expr(a, sc, base)?;      // AX = 基址
            push_ax(a);
            gen_expr(a, sc, idx)?;       // AX = 下标
            a.b(0x89); a.b(0xC3);        // mov bx, ax（BX = 下标）
            // 数组元素 2 字节：下标 ×2（字符串按字节，不乘）
            let is_arr = matches!(base.ty, Ty::Array(..) | Ty::List(..));
            if is_arr { a.b(0xD1); a.b(0xE3); } // shl bx, 1
            pop_ax(a);                   // AX = 基址
            a.b(0x01); a.b(0xD8);        // add ax, bx（字节偏移）
            a.b(0x89); a.b(0xC3);        // mov bx, ax（BX = 基址+偏移）
            a.b(0x8A); a.b(0x07);        // mov al, [bx]
            a.b(0x0F); a.b(0xB6); a.b(0xC0); // movzx ax, al
        }
        ExprKind::Match { subject, arms } => {
            // match 作为表达式：逐 arm 比较，命中则求其 body 值（AX）
            let lend = a.new_label("Lme");
            gen_expr(a, sc, subject)?;
            let subj_off = alloc(sc);
            store_ax(a, subj_off);
            for arm in arms {
                let lnext = a.new_label("Lmn");
                if let Some(pat) = &arm.pat {
                    load_ax(a, subj_off);
                    push_ax(a);
                    gen_expr(a, sc, pat)?;
                    a.b(0x89); a.b(0xC3);
                    pop_ax(a);
                    a.b(0x39); a.b(0xD8);
                    a.b(0x75);
                    let at = a.out.len(); a.b(0);
                    let f = a.pc();
                    a.fixups.push((at, lnext.clone(), FixKind::Rel8From(f)));
                }
                if let Some((lo, hi)) = &arm.range {
                    load_ax(a, subj_off);
                    push_ax(a);
                    gen_expr(a, sc, lo)?;
                    a.b(0x89); a.b(0xC3);
                    pop_ax(a);
                    a.b(0x39); a.b(0xD8);
                    a.b(0x7C);
                    let a1 = a.out.len(); a.b(0);
                    let f1 = a.pc();
                    a.fixups.push((a1, lnext.clone(), FixKind::Rel8From(f1)));
                    load_ax(a, subj_off);
                    push_ax(a);
                    gen_expr(a, sc, hi)?;
                    a.b(0x89); a.b(0xC3);
                    pop_ax(a);
                    a.b(0x39); a.b(0xD8);
                    a.b(0x7D);
                    let a2 = a.out.len(); a.b(0);
                    let f2 = a.pc();
                    a.fixups.push((a2, lnext.clone(), FixKind::Rel8From(f2)));
                }
                if let Some(g) = &arm.guard {
                    gen_expr(a, sc, g)?;
                    cmp_ax_0(a);
                    a.b(0x74);
                    let a3 = a.out.len(); a.b(0);
                    let f3 = a.pc();
                    a.fixups.push((a3, lnext.clone(), FixKind::Rel8From(f3)));
                }
                // body 的最后表达式即 arm 值（gen_block 里 Stmt::Expr 会留下 AX）
                gen_block(a, sc, &arm.body)?;
                a.b(0xE9); let atm = a.out.len(); a.w(0); let fm = a.pc(); a.fixups.push((atm, lend.clone(), FixKind::Rel16From(fm)));
                a.label(&lnext);
            }
            a.label(&lend);
        }
        ExprKind::ArrayLit(items) => {
            // 数组：在栈上为每个元素分配 2 字节（连续，向下增长）
            let mut offs = Vec::new();
            for _ in items.iter() { offs.push(alloc(sc)); }
            let n = items.len();
            for (i, it) in items.iter().enumerate() {
                gen_expr(a, sc, it)?;
                // 倒序存放：a[0] 放在最低地址（offs 最后一个）
                store_ax(a, offs[n - 1 - i]);
            }
            // AX = 首元素地址（最低地址 = 最后一个 alloc）
            let base = offs[n - 1];
            a.b(0x8D); a.b(0x46); a.b(base as u8); // lea ax, [bp+base]
        }
        ExprKind::Ident(n) => {
            let off = *sc.vars.get(n).ok_or_else(|| format!("未定义变量 '{}'", n))?;
            load_ax(a, off);
        }
        ExprKind::Binary(op, l, r) => {
            gen_expr(a, sc, l)?; push_ax(a);
            gen_expr(a, sc, r)?;
            pop_bx(a); // BX = 左值, AX = 右值
            match op {
                BinOp::Add => { a.b(0x01); a.b(0xD8); } // add ax, bx
                BinOp::Sub => { a.b(0x29); a.b(0xD8); } // sub ax, bx
                BinOp::Mul => { a.b(0x0F); a.b(0xAF); a.b(0xC3); } // imul ax, bx
                BinOp::Div | BinOp::FloorDiv => { a.b(0x99); a.b(0xF7); a.b(0xFB); } // cwd; idiv bx
                BinOp::Rem => { a.b(0x99); a.b(0xF7); a.b(0xFB); a.b(0x89); a.b(0xD0); } // cwd; idiv bx; mov ax,dx
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    a.b(0x39); a.b(0xC3); // cmp bx, ax（BX 是左值、AX 是右值）
                    let cc: u8 = match op {
                        BinOp::Eq => 0x94, BinOp::Ne => 0x95, BinOp::Lt => 0x9C,
                        BinOp::Le => 0x9E, BinOp::Gt => 0x9F, _ => 0x9D,
                    };
                    a.b(0x0F); a.b(cc); a.b(0xC0); // setcc al
                    a.b(0x0F); a.b(0xB6); a.b(0xC0); // movzx ax, al
                }
                _ => return Err("16 位后端暂不支持该运算符".into()),
            }
        }
        ExprKind::Call(name, args) => {
            if name == "put" {
                if let Some(ExprKind::Int(v)) = args.first().map(|x| &x.kind) {
                    // 直接输出一个字符（v 作为 ASCII）
                    a.b(0xB0); a.b(*v as u8);            // mov al, v
                    a.b(0xBA); a.w(0x3F8);              // mov dx, 0x3F8
                    a.b(0xEE);                          // out dx, al
                    a.b(0xB8); a.w(0);                  // mov ax, 0
                    return Ok(());
                }
                // 变量/表达式：求值到 AX，再经 COM1 输出低字节
                if let Some(arg) = args.first() {
                    gen_expr(a, sc, arg)?;
                    a.b(0x50);                      // push ax
                    a.b(0xBA); a.w(0x3F8);           // mov dx, 0x3F8
                    a.b(0x58);                      // pop ax
                    a.b(0xEE);                      // out dx, al
                    a.b(0xB8); a.w(0);               // mov ax, 0
                    return Ok(());
                }
                return Ok(());
            }
            // 普通函数调用：参数压栈（逆序），call，清理
            for arg in args.iter().rev() { gen_expr(a, sc, arg)?; push_ax(a); }
            let rel_addr = 0i64;
            a.b(0xE8);
            let at = a.out.len();
            a.w(0);
            let from = a.pc();
            a.fixups.push((at, format!("fn__{}", name), FixKind::Rel16From(from)));
            if std::env::var("GTC_DBG").is_ok() { eprintln!("[asm16] call {} at={} from={:#x}", name, at, from); }
            let _ = rel_addr;
            // 清理参数：callee 的 ret 已弹返回地址，这里只弹掉压入的参数。
            // 返回值已在 AX，弹参用"弹到 BX"避免覆盖 AX。
            for _ in 0..args.len() { a.b(0x5B); } // pop bx × n
        }
        _ => return Err("16 位后端暂不支持该表达式".into()),
    }
    Ok(())
}

impl Asm16 {
    pub fn new() -> Self {
        Asm16 { out: Vec::new(), labels: HashMap::new(), fixups: Vec::new(), fn_addr: HashMap::new(), entry: None, strs: Vec::new(), seq: 0 }
    }

    fn pc(&self) -> i64 { ORG + self.out.len() as i64 }
    fn new_label(&mut self, prefix: &str) -> String { let s = self.seq; self.seq += 1; format!("{}{}", prefix, s) }
    fn b(&mut self, x: u8) { self.out.push(x); }
    fn w(&mut self, x: i64) { self.out.extend_from_slice(&((x as u16) & 0xFFFF).to_le_bytes()); }

    fn label(&mut self, name: &str) { let a = self.pc(); self.labels.insert(name.to_string(), a); }
    fn rel8(&mut self, op: u8, target: &str) { self.b(op); let at = self.out.len(); self.b(0); let from = self.pc(); self.fixups.push((at, target.to_string(), FixKind::Rel8From(from))); }

    #[allow(dead_code)]
    fn patch(&mut self, entry: &str) -> Result<(), String> {
        let base = self.fn_addr.get(entry).copied().ok_or_else(|| format!("入口函数 '{}' 不存在", entry))?;
        let _ = base;
        Ok(())
    }
}


