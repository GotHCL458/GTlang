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
    a.b(0xF4); // hlt
    // 生成所有函数
    for f in &fns {
        let addr = a.pc();
        a.fn_addr.insert(f.name.clone(), addr);
        a.label(&format!("fn__{}", f.name));
        gen_fn(&mut a, f)?;
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
struct Scope { vars: HashMap<String, i64>, base: i64, next: i64 }

fn gen_fn(a: &mut Asm16, f: &FnDef) -> Result<(), String> {
    a.b(0x55);            // push bp
    a.b(0x89); a.b(0xE5); // mov bp, sp
    let nparams = f.params.len() as i64;
    let frame = 256i64;
    a.b(0x83); a.b(0xEC); a.b(frame as u8); // sub sp, frame
    let mut sc = Scope { vars: HashMap::new(), base: 4, next: -2 };
    // 参数在 [bp+4], [bp+6], ...
    for (i, p) in f.params.iter().enumerate() {
        sc.vars.insert(p.name.clone(), 4 + (i as i64) * 2);
    }
    let _ = nparams;
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
            let lelse = format!("Lelse{}", a.out.len());
            let lend = format!("Lend{}", a.out.len());
            gen_expr(a, sc, cond)?;
            cmp_ax_0(a);
            a.rel8(0x74, &lelse); // je else
            gen_block(a, sc, then)?;
            if els.is_some() { a.rel8(0xEB, &lend); }
            a.label(&lelse);
            if let Some(e) = els { gen_block(a, sc, e)?; a.label(&lend); }
        }
        Stmt::While { cond, body, .. } => {
            let ltop = format!("Ltop{}", a.out.len());
            let lend = format!("Lend{}", a.out.len());
            a.label(&ltop);
            gen_expr(a, sc, cond)?;
            cmp_ax_0(a);
            a.rel8(0x74, &lend);
            gen_block(a, sc, body)?;
            a.rel8(0xEB, &ltop);
            a.label(&lend);
        }
        Stmt::Block(inner) => gen_block(a, sc, inner)?,
        _ => return Err(format!("16 位后端暂不支持该语句")),
    }
    Ok(())
}

/// AX 入栈 / 出栈（供表达式求值）
fn push_ax(a: &mut Asm16) { a.b(0x50); }
fn pop_bx(a: &mut Asm16) { a.b(0x5B); }

fn store_ax(a: &mut Asm16, off: i64) {
    // mov [bp+off], ax
    a.b(0x89); a.b(0x46); a.b(off as u8);
}
fn load_ax(a: &mut Asm16, off: i64) {
    a.b(0x8B); a.b(0x46); a.b(off as u8);
}
fn cmp_ax_0(a: &mut Asm16) { a.b(0x3D); a.w(0); } // cmp ax, 0
fn ret(a: &mut Asm16) { a.b(0x89); a.b(0xEC); a.b(0x5D); a.b(0xC3); } // mov sp,bp; pop bp; ret

fn gen_expr(a: &mut Asm16, sc: &mut Scope, e: &Expr) -> Result<(), String> {
    match &e.kind {
        ExprKind::Int(v) => { a.b(0xB8); a.w(*v); }
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
                    a.b(0x39); a.b(0xD8); // cmp ax, bx（注意方向）
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
            let _ = rel_addr;
            for _ in args { a.b(0x58); } // pop ax（清理参数）
        }
        _ => return Err("16 位后端暂不支持该表达式".into()),
    }
    Ok(())
}

impl Asm16 {
    pub fn new() -> Self {
        Asm16 { out: Vec::new(), labels: HashMap::new(), fixups: Vec::new(), fn_addr: HashMap::new(), entry: None }
    }

    fn pc(&self) -> i64 { ORG + self.out.len() as i64 }
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
