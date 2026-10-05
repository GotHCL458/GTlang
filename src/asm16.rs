//! 内置 16 位（x86 real mode）汇编器：`.asm` 文本 -> 机器码。
//!
//! 支持的基础语法（够写引导库与小程序）：
//! - 段指示：`BITS 16`、`ORG 0x7C00`
//! - 标签：`name:`；`equ` 常量：`X equ 3`
//! - 数据：`db` / `dw` / `dd` / `times N db X`
//! - 指令子集：mov / add / sub / cmp / jmp / jcc / call / ret / push / pop /
//!   int / out / in / xor / and / or / test / inc / dec / shl / shr / hlt / cli / sti / nop
//! - 操作数：寄存器（8/16 位）、立即数、`[地址]`、`[reg]`、`[reg+off]`

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
enum Op {
    Sym(String),
    Reg8(&'static str),
    Reg16(&'static str),
    Imm(i64),
    Mem(Vec<MemPart>),
}

#[derive(Debug, Clone, PartialEq)]
enum MemPart {
    Reg(&'static str),
    Disp(i64),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Item {
    Label(String),
    Instr(String, Vec<Op>, usize),
    Data(Vec<DataItem>, usize),
    Org(i64),
    Bits(u8),
    Equ(String, i64),
    /// `times <expr> <data>`：expr 在第二遍按当前 pc/org 求值（支持 $ / $$ / A-B）
    TimesExpr(String, Box<DataItem>, usize),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum DataItem { Bytes(Vec<i64>), Str(Vec<u8>), Words(Vec<i64>), Dwords(Vec<i64>), Times(i64, Box<DataItem>) }

pub fn assemble(text: &str) -> Result<Vec<u8>, String> {
    let items = parse(text)?;
    emit(&items)
}

fn reg16(name: &str) -> Option<&'static str> {
    Some(match name {
        "ax" => "ax", "bx" => "bx", "cx" => "cx", "dx" => "dx",
        "si" => "si", "di" => "di", "bp" => "bp", "sp" => "sp",
        _ => return None,
    })
}

fn reg16_code(name: &str) -> u8 {
    match name { "ax" => 0, "cx" => 1, "dx" => 2, "bx" => 3, "sp" => 4, "bp" => 5, "si" => 6, "di" => 7, _ => 0 }
}

fn reg8(name: &str) -> Option<&'static str> {
    Some(match name {
        "al" => "al", "bl" => "bl", "cl" => "cl", "dl" => "dl",
        "ah" => "ah", "bh" => "bh", "ch" => "ch", "dh" => "dh",
        _ => return None,
    })
}

fn reg8_code(name: &str) -> u8 {
    match name { "al" => 0, "cl" => 1, "dl" => 2, "bl" => 3, "ah" => 4, "ch" => 5, "dh" => 6, "bh" => 7, _ => 0 }
}


/// 两遍：第一遍算标签地址，第二遍生成机器码。
fn emit(items: &[Item]) -> Result<Vec<u8>, String> {
    let mut labels: HashMap<String, i64> = HashMap::new();
    let mut equs: HashMap<String, i64> = HashMap::new();
    let mut _org: i64 = 0;
    // 第一遍：equ 先收集（可前向），再算地址
    for it in items {
        if let Item::Equ(n, v) = it { equs.insert(n.clone(), *v); }
    }
    let mut pc: i64 = 0;
    // 第一遍：只收集标签地址（长度用"占位长度"，跳转固定 3/5 字节）
    for it in items {
        match it {
            Item::Org(v) => { _org = *v; pc = *v; }
            Item::Bits(_) | Item::Equ(..) => {}
            Item::Label(n) => { labels.insert(n.clone(), pc); }
            Item::Data(d, _) => { for x in d { pc += data_len(x); } }
            Item::TimesExpr(..) => { /* 第一遍跳过：其长度依赖 pc，第二遍求值 */ }
            Item::Instr(m, ops, ln) => { pc += instr_len(m, ops, &labels, &equs, *ln)? as i64; }
        }
    }
    // 第二遍：生成
    let mut out: Vec<u8> = Vec::new();
    pc = 0;
    for it in items {
        match it {
            Item::Org(v) => { pc = *v; }
            Item::Bits(_) | Item::Equ(..) | Item::Label(_) => {}
            Item::Data(d, ln) => { for x in d { emit_data(x, &mut out, *ln)?; pc += data_len(x); } }
            Item::TimesExpr(expr, inner, ln) => {
                // 求值：支持 N、$、$$、A-B
                let n = eval_times(expr, pc, _org, *ln)?;
                for _ in 0..n { emit_data(inner, &mut out, *ln)?; }
                pc += n * data_len(inner);
            }
            Item::Instr(m, ops, ln) => {
                let before = out.len();
                emit_instr(m, ops, pc, &labels, &equs, *ln, &mut out)?;
                let len = (out.len() - before) as i64;
                // 回填相对跳转（指令末尾的 2 字节占位 -> 相对位移）
                if matches!(m.as_str(), "jmp" | "call" | "je" | "jz" | "jne" | "jnz" | "jb" | "jc" | "jnae" | "jae" | "jnc" | "jnb" | "jbe" | "jna" | "ja" | "jnbe" | "jl" | "jnge" | "jge" | "jnl" | "jle" | "jng" | "jg" | "jnle" | "js" | "jns" | "jo" | "jno" | "loop") {
                    if let Some(Op::Sym(name)) = ops.first() {
                        if let Some(tgt) = labels.get(name) {
                            let next = pc + len;
                            let rel = (*tgt - next) as i64;
                            let n = out.len();
                            if m == "call" {
                                // rel16（2 字节，小端）
                                let r16 = rel as u16;
                                out[n - 2] = (r16 & 0xFF) as u8;
                                out[n - 1] = ((r16 >> 8) & 0xFF) as u8;
                            } else if rel >= -128 && rel <= 127 {
                                out[n - 1] = (rel as i8) as u8;
                            }
                        }
                    }
                }
                pc += len;
            }
        }
    }
    Ok(out)
}

/// 求值 `times` 的计数表达式：`N` / `$` / `$$` / `A-B`。`$` = 当前位置（pc），`$$` = 0（ORG 基址）。
fn eval_times(expr: &str, pc: i64, org: i64, ln: usize) -> Result<i64, String> {
    let e = expr.trim();
    if let Some(i) = e.find('-') {
        let a = eval_atom(e[..i].trim(), pc, org, ln)?;
        let b = eval_atom(e[i + 1..].trim(), pc, org, ln)?;
        return Ok(a - b);
    }
    eval_atom(e, pc, org, ln)
}

fn eval_atom(s: &str, pc: i64, org: i64, ln: usize) -> Result<i64, String> {
    let s = s.trim();
    // 去括号
    if s.starts_with('(') && s.ends_with(')') { return eval_atom(&s[1..s.len()-1], pc, org, ln); }
    if s == "$" { return Ok(pc); }
    if s == "$$" { return Ok(org); }
    // 形如 $-$$
    if let Some(i) = s.find("-") {
        let a = eval_atom(&s[..i], pc, org, ln)?;
        let b = eval_atom(&s[i+1..], pc, org, ln)?;
        return Ok(a - b);
    }
    parse_int(s).map_err(|e| format!("第 {} 行：times 计数 {}", ln, e))
}
fn data_len(d: &DataItem) -> i64 {
    match d {
        DataItem::Bytes(v) => v.len() as i64,
        DataItem::Str(b) => b.len() as i64,
        DataItem::Words(v) => v.len() as i64 * 2,
        DataItem::Dwords(v) => v.len() as i64 * 4,
        DataItem::Times(n, inner) => n * data_len(inner),
    }
}

fn emit_data(d: &DataItem, out: &mut Vec<u8>, ln: usize) -> Result<(), String> {
    match d {
        DataItem::Bytes(v) => for x in v { out.push(*x as u8); },
        DataItem::Str(b) => out.extend_from_slice(b),
        DataItem::Words(v) => for x in v { out.extend_from_slice(&(*x as u16).to_le_bytes()); },
        DataItem::Dwords(v) => for x in v { out.extend_from_slice(&(*x as u32).to_le_bytes()); },
        DataItem::Times(n, inner) => for _ in 0..*n { emit_data(inner, out, ln)?; },
    }
    Ok(())
}

fn instr_len(m: &str, ops: &[Op], labels: &HashMap<String, i64>, equs: &HashMap<String, i64>, ln: usize) -> Result<usize, String> {
    let mut tmp = Vec::new();
    emit_instr(m, ops, 0, labels, equs, ln, &mut tmp)?;
    Ok(tmp.len())
}


/// 生成一条指令的机器码（16 位实模式子集）。
fn emit_instr(m: &str, ops: &[Op], _pc: i64, labels: &HashMap<String, i64>, equs: &HashMap<String, i64>, ln: usize, out: &mut Vec<u8>) -> Result<(), String> {
    let resolve = |o: &Op| -> i64 {
        match o {
            Op::Imm(v) => *v,
            _ => 0,
        }
    };
    let err = |msg: String| format!("第 {} 行：{}", ln, msg);
    match m {
        "nop" => out.push(0x90),
        "hlt" => out.push(0xF4),
        "cli" => out.push(0xFA),
        "sti" => out.push(0xFB),
        "cld" => out.push(0xFC),
        "ret" => out.push(0xC3),
        "pushf" => out.push(0x9C),
        "popf" => out.push(0x9D),
        "wbinvd" => out.extend_from_slice(&[0x0F, 0x09]),
        "int" => {
            let n = resolve(&ops[0]) as u8;
            out.extend_from_slice(&[0xCD, n]);
        }
        "out" => {
            // out dx, al / out dx, ax / out imm8, al / out imm8, ax
            match (&ops[0], &ops[1]) {
                (Op::Reg16("dx"), Op::Reg8("al")) => out.extend_from_slice(&[0xEE]),
                (Op::Reg16("dx"), Op::Reg16("ax")) => out.extend_from_slice(&[0xEF]),
                (Op::Imm(p), Op::Reg8("al")) => out.extend_from_slice(&[0xE6, *p as u8]),
                (Op::Imm(p), Op::Reg16("ax")) => out.extend_from_slice(&[0xE7, *p as u8]),
                _ => return Err(err("out 仅支持 dx/imm8 与 al/ax".into())),
            }
        }
        "in" => {
            match (&ops[0], &ops[1]) {
                (Op::Reg8("al"), Op::Reg16("dx")) => out.push(0xEC),
                (Op::Reg16("ax"), Op::Reg16("dx")) => out.push(0xED),
                _ => return Err(err("in 仅支持 al/ax, dx".into())),
            }
        }
        "mov" => emit_mov(&ops[0], &ops[1], labels, equs, out, ln)?,
        "xor" => emit_alu(0x31, 0x30, 0x35, &ops[0], &ops[1], out, ln)?,
        "add" => emit_alu(0x01, 0x00, 0x05, &ops[0], &ops[1], out, ln)?,
        "sub" => emit_alu(0x29, 0x28, 0x2D, &ops[0], &ops[1], out, ln)?,
        "and" => emit_alu(0x21, 0x20, 0x25, &ops[0], &ops[1], out, ln)?,
        "or" => emit_alu(0x09, 0x08, 0x0D, &ops[0], &ops[1], out, ln)?,
        "cmp" => emit_alu(0x39, 0x38, 0x3D, &ops[0], &ops[1], out, ln)?,
        "test" => emit_alu(0x85, 0x84, 0xA9, &ops[0], &ops[1], out, ln)?,
        "inc" => emit_incdec(&ops[0], true, out, ln)?,
        "dec" => emit_incdec(&ops[0], false, out, ln)?,
        "shl" | "sal" => emit_shift(&ops[0], &ops[1], 4, out, ln)?,
        "shr" => emit_shift(&ops[0], &ops[1], 5, out, ln)?,
        "sar" => emit_shift(&ops[0], &ops[1], 7, out, ln)?,
        "rol" => emit_shift(&ops[0], &ops[1], 0, out, ln)?,
        "ror" => emit_shift(&ops[0], &ops[1], 1, out, ln)?,
        "push" => emit_pushpop(&ops[0], true, out, ln)?,
        "pop" => emit_pushpop(&ops[0], false, out, ln)?,
        "jmp" => emit_jmp(&ops[0], labels, equs, out, ln, 0xEB, 0xE9, 0x00)?,
        "call" => emit_jmp(&ops[0], labels, equs, out, ln, 0xFF, 0xE8, 0x02)?,
        "je" | "jz" => emit_jcc(0x74, &ops[0], labels, equs, out, ln)?,
        "jne" | "jnz" => emit_jcc(0x75, &ops[0], labels, equs, out, ln)?,
        "jb" | "jc" | "jnae" => emit_jcc(0x72, &ops[0], labels, equs, out, ln)?,
        "jae" | "jnc" | "jnb" => emit_jcc(0x73, &ops[0], labels, equs, out, ln)?,
        "jbe" | "jna" => emit_jcc(0x76, &ops[0], labels, equs, out, ln)?,
        "ja" | "jnbe" => emit_jcc(0x77, &ops[0], labels, equs, out, ln)?,
        "jl" | "jnge" => emit_jcc(0x7C, &ops[0], labels, equs, out, ln)?,
        "jge" | "jnl" => emit_jcc(0x7D, &ops[0], labels, equs, out, ln)?,
        "jle" | "jng" => emit_jcc(0x7E, &ops[0], labels, equs, out, ln)?,
        "jg" | "jnle" => emit_jcc(0x7F, &ops[0], labels, equs, out, ln)?,
        "js" => emit_jcc(0x78, &ops[0], labels, equs, out, ln)?,
        "jns" => emit_jcc(0x79, &ops[0], labels, equs, out, ln)?,
        "jo" => emit_jcc(0x70, &ops[0], labels, equs, out, ln)?,
        "jno" => emit_jcc(0x71, &ops[0], labels, equs, out, ln)?,
        "loop" => emit_jcc(0xE2, &ops[0], labels, equs, out, ln)?,
        "div" => {
            // div r/m16：DX:AX / r/m16 -> AX 商, DX 余
            match &ops[0] {
                Op::Reg16(r) => { out.push(0xF7); out.push(modrm(3, 6, reg16_code(r))); }
                Op::Reg8(r) => { out.push(0xF6); out.push(modrm(3, 6, reg8_code(r))); }
                _ => return Err(err("div 操作数非法".into())),
            }
        }
        "mul" => {
            match &ops[0] {
                Op::Reg16(r) => { out.push(0xF7); out.push(modrm(3, 4, reg16_code(r))); }
                Op::Reg8(r) => { out.push(0xF6); out.push(modrm(3, 4, reg8_code(r))); }
                _ => return Err(err("mul 操作数非法".into())),
            }
        }
        "neg" => {
            match &ops[0] {
                Op::Reg16(r) => { out.push(0xF7); out.push(modrm(3, 3, reg16_code(r))); }
                Op::Reg8(r) => { out.push(0xF6); out.push(modrm(3, 3, reg8_code(r))); }
                _ => return Err(err("neg 操作数非法".into())),
            }
        }
        "not" => {
            match &ops[0] {
                Op::Reg16(r) => { out.push(0xF7); out.push(modrm(3, 2, reg16_code(r))); }
                Op::Reg8(r) => { out.push(0xF6); out.push(modrm(3, 2, reg8_code(r))); }
                _ => return Err(err("not 操作数非法".into())),
            }
        }
        "imul" => {
            match &ops[0] {
                Op::Reg16(r) => { out.push(0xF7); out.push(modrm(3, 5, reg16_code(r))); }
                _ => return Err(err("imul 操作数非法".into())),
            }
        }
        "cwd" => out.push(0x99),
        "cdq" => out.push(0x99),
        "lodsb" => out.push(0xAC),
        "stosb" => out.push(0xAA),
        "rep" => { out.push(0xF3); }
        _ => return Err(err(format!("不支持的指令 '{}'", m))),
    }
    Ok(())
}


/// ModRM 字节：mod<<6 | reg<<3 | rm
fn modrm(m: u8, reg: u8, rm: u8) -> u8 { (m << 6) | ((reg & 7) << 3) | (rm & 7) }

/// 内存操作数的 ModRM + disp（16 位寻址）。返回 (modrm, disp 字节)。
fn mem_operand(parts: &[MemPart], reg_field: u8, out: &mut Vec<u8>, ln: usize) -> Result<(), String> {
    // 支持：[disp16] / [bx|bp|si|di] / [bx+si] / [bx+di] / [bp+si] / [bp+di] / [reg+disp8/16]
    let mut base: Option<&str> = None;
    let mut index: Option<&str> = None;
    let mut disp: i64 = 0;
    for p in parts {
        match p {
            MemPart::Reg(r) => {
                if matches!(*r, "bx" | "bp") { base = Some(r); } else { index = Some(r); }
            }
            MemPart::Disp(d) => disp += d,
        }
    }
    let (modv, rm): (u8, u8) = match (base, index) {
        (Some("bx"), Some("si")) => (0, 0),
        (Some("bx"), Some("di")) => (0, 1),
        (Some("bp"), Some("si")) => (0, 2),
        (Some("bp"), Some("di")) => (0, 3),
        (None, Some("si")) => (0, 4),
        (None, Some("di")) => (0, 5),
        (Some("bp"), None) => (0, 6),
        (Some("bx"), None) => (0, 7),
        (None, None) => (0, 6), // [disp16]：mod=00, rm=110
        _ => return Err(format!("第 {} 行：不支持的寻址组合", ln)),
    };
    // [disp16] 用 mod=00 rm=110 直接跟 16 位位移
    if base.is_none() && index.is_none() {
        out.push(modrm(0, reg_field, 6));
        out.extend_from_slice(&(disp as u16).to_le_bytes());
        return Ok(());
    }
    // bp 基址 + 无位移 → mod=01 disp8=0
    let need8 = disp != 0 && (-128..=127).contains(&disp);
    let need16 = disp != 0 && !need8;
    let modbits = if need16 { 2 } else if need8 { 1 } else if base == Some("bp") && index.is_none() { 1 } else { 0 };
    out.push(modrm(modbits, reg_field, rm));
    if modbits == 1 { out.push(disp as u8); }
    else if modbits == 2 { out.extend_from_slice(&(disp as u16).to_le_bytes()); }
    let _ = modv;
    Ok(())
}

/// mov：支持 reg/imm、reg/reg、reg/[mem]、[mem]/reg、[mem]/imm、sreg。
fn emit_mov(dst: &Op, src: &Op, _labels: &HashMap<String, i64>, equs: &HashMap<String, i64>, out: &mut Vec<u8>, ln: usize) -> Result<(), String> {
    match (dst, src) {
        (Op::Reg16(r), Op::Imm(v)) => {
            let v = *v;
            out.push(0xB8 + reg16_code(r));
            out.extend_from_slice(&(v as u16).to_le_bytes());
        }
        (Op::Reg8(r), Op::Imm(v)) => {
            out.push(0xB0 + reg8_code(r));
            out.push(*v as u8);
        }
        (Op::Reg16(d), Op::Reg16(s)) => {
            out.push(0x89);
            out.push(modrm(3, reg16_code(s), reg16_code(d)));
        }
        (Op::Reg8(d), Op::Reg8(s)) => {
            out.push(0x88);
            out.push(modrm(3, reg8_code(s), reg8_code(d)));
        }
        (Op::Reg16(d), Op::Mem(parts)) => {
            out.push(0x8B);
            mem_operand(parts, reg16_code(d), out, ln)?;
        }
        (Op::Reg8(d), Op::Mem(parts)) => {
            out.push(0x8A);
            mem_operand(parts, reg8_code(d), out, ln)?;
        }
        (Op::Mem(parts), Op::Reg16(s)) => {
            out.push(0x89);
            mem_operand(parts, reg16_code(s), out, ln)?;
        }
        (Op::Mem(parts), Op::Reg8(s)) => {
            out.push(0x88);
            mem_operand(parts, reg8_code(s), out, ln)?;
        }
        (Op::Mem(parts), Op::Imm(v)) => {
            out.push(0xC7);
            mem_operand(parts, 0, out, ln)?;
            out.extend_from_slice(&(*v as u16).to_le_bytes());
        }
        _ => return Err(format!("第 {} 行：mov 不支持的操作数组合", ln)),
    }
    let _ = equs;
    Ok(())
}

/// 通用 ALU（reg/mem 形式 op_r/m 与 op_r/m,reg；imm 形式 op_acc,imm）。
fn emit_alu(op_rm_r: u8, op_r_rm: u8, op_acc_imm: u8, dst: &Op, src: &Op, out: &mut Vec<u8>, ln: usize) -> Result<(), String> {
    match (dst, src) {
        (Op::Reg16(d), Op::Reg16(s)) => { out.push(op_rm_r); out.push(modrm(3, reg16_code(s), reg16_code(d))); }
        (Op::Reg8(d), Op::Reg8(s)) => { out.push(op_r_rm); out.push(modrm(3, reg8_code(s), reg8_code(d))); }
        (Op::Reg16(d), Op::Imm(v)) => {
            if (-128..=127).contains(v) { out.push(0x83); out.push(modrm(3, alu_ext(op_rm_r), reg16_code(d))); out.push(*v as u8); }
            else { out.push(0x81); out.push(modrm(3, alu_ext(op_rm_r), reg16_code(d))); out.extend_from_slice(&(*v as u16).to_le_bytes()); }
        }
        (Op::Reg8(d), Op::Imm(v)) => { out.push(0x80); out.push(modrm(3, alu_ext(op_rm_r), reg8_code(d))); out.push(*v as u8); }
        (Op::Mem(parts), Op::Reg16(s)) => { out.push(op_rm_r); mem_operand(parts, reg16_code(s), out, ln)?; }
        (Op::Mem(parts), Op::Reg8(s)) => { out.push(op_r_rm); mem_operand(parts, reg8_code(s), out, ln)?; }
        (Op::Reg16(d), Op::Mem(parts)) => { out.push(op_r_rm + 2); mem_operand(parts, reg16_code(d), out, ln)?; }
        (Op::Reg8(d), Op::Mem(parts)) => { out.push(op_r_rm + 2); mem_operand(parts, reg8_code(d), out, ln)?; }
        (Op::Mem(parts), Op::Imm(v)) => { out.push(0x81); mem_operand(parts, alu_ext(op_rm_r), out, ln)?; out.extend_from_slice(&(*v as u16).to_le_bytes()); }
        _ => return Err(format!("第 {} 行：ALU 不支持的操作数组合", ln)),
    }
    let _ = op_acc_imm;
    Ok(())
}

/// 由 opcode 映射出 ALU 扩展字段（reg 字段 /7 等）
fn alu_ext(op_rm_r: u8) -> u8 {
    match op_rm_r {
        0x01 | 0x00 => 0, // add
        0x09 | 0x08 => 1, // or
        0x21 | 0x20 => 4, // and
        0x29 | 0x28 => 5, // sub
        0x31 | 0x30 => 6, // xor
        0x39 | 0x38 => 7, // cmp
        0x85 | 0x84 => 0, // test（用 /0）
        _ => 0,
    }
}

fn emit_incdec(op: &Op, inc: bool, out: &mut Vec<u8>, ln: usize) -> Result<(), String> {
    let (r16, r8, ext) = if inc { (0x40, 0xFE, 0u8) } else { (0x48, 0xFE, 1u8) };
    match op {
        Op::Reg16(r) => out.push(r16 + reg16_code(r)),
        Op::Reg8(r) => { out.push(r8); out.push(modrm(3, ext, reg8_code(r))); }
        Op::Mem(parts) => { out.push(r8); mem_operand(parts, ext, out, ln)?; }
        _ => return Err(format!("第 {} 行：inc/dec 操作数非法", ln)),
    }
    Ok(())
}

fn emit_shift(op: &Op, cnt: &Op, ext: u8, out: &mut Vec<u8>, ln: usize) -> Result<(), String> {
    let rm = match op {
        Op::Reg16(r) => { out.push(0xC1); modrm(3, ext, reg16_code(r)) }
        Op::Reg8(r) => { out.push(0xC0); modrm(3, ext, reg8_code(r)) }
        Op::Mem(parts) => { out.push(0xC1); let mut tmp = Vec::new(); mem_operand(parts, ext, &mut tmp, ln)?; out.extend_from_slice(&tmp); return Ok(()); }
        _ => return Err(format!("第 {} 行：移位操作数非法", ln)),
    };
    out.push(rm);
    let c = match cnt { Op::Imm(v) => *v as u8, Op::Reg8("cl") => 0xFF, _ => 1 };
    out.push(c);
    Ok(())
}

fn emit_pushpop(op: &Op, push: bool, out: &mut Vec<u8>, ln: usize) -> Result<(), String> {
    match op {
        Op::Reg16(r) => out.push(if push { 0x50 } else { 0x58 } + reg16_code(r)),
        Op::Imm(v) => { out.push(0x68); out.extend_from_slice(&(*v as u16).to_le_bytes()); }
        Op::Mem(parts) => { out.push(if push { 0xFF } else { 0x8F }); mem_operand(parts, if push { 6 } else { 0 }, out, ln)?; }
        _ => return Err(format!("第 {} 行：push/pop 操作数非法", ln)),
    }
    Ok(())
}

/// 跳转：目标标签 -> 相对位移。short=0xEB, near=0xE9, indirect=0xFF。
fn emit_jmp(op: &Op, _labels: &HashMap<String, i64>, _equs: &HashMap<String, i64>, out: &mut Vec<u8>, ln: usize, short_op: u8, near_op: u8, _ind: u8) -> Result<(), String> {
    match op {
        Op::Imm(v) => { out.push(if short_op == 0xFF { near_op } else { short_op }); out.extend_from_slice(&(*v as u16).to_le_bytes()); }
        Op::Reg16(r) => { out.push(0xFF); out.push(modrm(3, 4, reg16_code(r))); }
        Op::Sym(_name) => {
            // jmp 用短跳 0xEB+1 字节；call 用 0xE8+2 字节（rel16）
            if near_op == 0xE9 {
                out.push(0xEB);
                out.push(0);
            } else {
                out.push(near_op);
                out.push(0);
                out.push(0);
            }
        }
        _ => return Err(format!("第 {} 行：jmp/call 目标非法", ln)),
    }
    Ok(())
}

fn emit_jcc(op: u8, target: &Op, _labels: &HashMap<String, i64>, _equs: &HashMap<String, i64>, out: &mut Vec<u8>, ln: usize) -> Result<(), String> {
    match target {
        Op::Imm(v) => { out.push(op); out.extend_from_slice(&(*v as u16).to_le_bytes()); }
        Op::Sym(_name) => {
            // 短跳：op + 1 字节相对位移（占位）
            out.push(op);
            out.push(0);
        }
        _ => return Err(format!("第 {} 行：条件跳转目标非法", ln)),
    }
    Ok(())
}

fn parse(text: &str) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    let mut line_no = 0;
    for raw in text.lines() {
        line_no += 1;
        // 去注释（; 之后）
        let line = match raw.find(';') { Some(i) => &raw[..i], None => raw };
        let line = line.trim();
        if line.is_empty() { continue; }
        // ORG / BITS
        let up = line.to_uppercase();
        if up.starts_with("ORG ") {
            items.push(Item::Org(parse_int(line[4..].trim())?));
            continue;
        }
        if up.starts_with("BITS ") {
            items.push(Item::Bits(line[5..].trim().parse().map_err(|_| format!("第 {} 行：BITS 值非法", line_no))?));
            continue;
        }
        // equ
        if let Some(pos) = up.find(" EQU ") {
            let name = line[..pos].trim().to_string();
            let val = parse_int(line[pos + 5..].trim())?;
            items.push(Item::Equ(name, val));
            continue;
        }
        // 标签
        if line.ends_with(':') {
            items.push(Item::Label(line[..line.len() - 1].trim().to_string()));
            continue;
        }
        // times（支持 `N`、`$`、`$$`、`A-B` 形式的计数）
        if up.starts_with("TIMES ") {
            let rest = line[6..].trim();
            let sp = rest.find(char::is_whitespace).ok_or_else(|| format!("第 {} 行：times 语法错误", line_no))?;
            let cnt_expr = rest[..sp].trim().to_string();
            let inner = parse_data(rest[sp..].trim(), line_no)?;
            items.push(Item::TimesExpr(cnt_expr, Box::new(inner), line_no));
            continue;
        }
        // 数据指示（允许前置标签：`msg db ...`）
        let ws: Vec<&str> = line.split_whitespace().collect();
        let dpos = ws.iter().position(|w| matches!(*w, "db" | "dw" | "dd" | "DB" | "DW" | "DD"));
        if let Some(p) = dpos {
            if p > 0 {
                // 前置 token 作为标签（可能有多个）
                for w in &ws[..p] {
                    items.push(Item::Label(w.trim_end_matches(':').to_string()));
                }
            }
            let data_src = ws[p..].join(" ");
            items.push(Item::Data(vec![parse_data(&data_src, line_no)?], line_no));
            continue;
        }
        // 指令
        let mut parts = line.splitn(2, char::is_whitespace);
        let mnem = parts.next().unwrap_or("").to_lowercase();
        let rest = parts.next().unwrap_or("").trim();
        let ops = if rest.is_empty() { Vec::new() } else { parse_ops(rest, line_no)? };
        items.push(Item::Instr(mnem, ops, line_no));
    }
    Ok(items)
}

fn parse_data(s: &str, line_no: usize) -> Result<DataItem, String> {
    let toks: Vec<&str> = s.splitn(2, char::is_whitespace).collect();
    let kind = toks[0].to_lowercase();
    let body = toks.get(1).copied().unwrap_or("");
    let mut vals = Vec::new();
    for part in split_commas(body) {
        let p = part.trim();
        if p.is_empty() { continue; }
        if (p.starts_with('"') && p.ends_with('"')) || (p.starts_with('\'') && p.ends_with('\'')) {
            let bytes: Vec<u8> = p[1..p.len() - 1].bytes().collect();
            vals.extend(bytes.iter().map(|b| *b as i64));
            continue;
        }
        vals.push(parse_int(p)?);
    }
    Ok(match kind.as_str() {
        "db" => DataItem::Bytes(vals),
        "dw" => DataItem::Words(vals),
        "dd" => DataItem::Dwords(vals),
        _ => return Err(format!("第 {} 行：未知数据指示 '{}'", line_no, kind)),
    })
}

fn split_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut q = ' ';
    for c in s.chars() {
        if in_q {
            cur.push(c);
            if c == q { in_q = false; }
        } else if c == '"' || c == '\'' {
            in_q = true; q = c; cur.push(c);
        } else if c == ',' {
            out.push(cur.clone()); cur.clear();
        } else {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() { out.push(cur); }
    out
}

fn parse_int(s: &str) -> Result<i64, String> {
    let t = s.trim();
    let (neg, t) = if let Some(r) = t.strip_prefix('-') { (true, r.trim()) } else { (false, t) };
    let v = if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        i64::from_str_radix(h, 16).map_err(|_| format!("非法十六进制：{}", s))?
    } else if let Some(b) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        i64::from_str_radix(b, 2).map_err(|_| format!("非法二进制：{}", s))?
    } else {
        t.parse::<i64>().map_err(|_| format!("非法数字：{}", s))?
    };
    Ok(if neg { -v } else { v })
}

fn parse_ops(s: &str, line_no: usize) -> Result<Vec<Op>, String> {
    let mut out = Vec::new();
    for part in split_commas(s) {
        let p = part.trim();
        if p.is_empty() { continue; }
        out.push(parse_op(p, line_no)?);
    }
    Ok(out)
}

fn parse_op(s: &str, _line_no: usize) -> Result<Op, String> {
    let t = s.trim();
    if let Some(inner) = t.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
        let mut parts = Vec::new();
        let mut cur = String::new();
        for c in inner.chars() {
            if c == '+' { if !cur.trim().is_empty() { parts.push(cur.trim().to_string()); } cur.clear(); }
            else { cur.push(c); }
        }
        if !cur.trim().is_empty() { parts.push(cur.trim().to_string()); }
        let mut mp = Vec::new();
        for p in parts {
            if let Some(r) = reg16(&p.to_lowercase()) { mp.push(MemPart::Reg(r)); }
            else { mp.push(MemPart::Disp(parse_int(&p)?)); }
        }
        return Ok(Op::Mem(mp));
    }
    let lower = t.to_lowercase();
    if let Some(r) = reg16(&lower) { return Ok(Op::Reg16(r)); }
    if let Some(r) = reg8(&lower) { return Ok(Op::Reg8(r)); }
    // 特殊符号：$ = 当前偏移（此处用 0 占位，由第二遍回填）；$$ = 段基址（0）
    if lower == "$" { return Ok(Op::Imm(i64::MIN + 1)); }
    if lower == "$$" { return Ok(Op::Imm(0)); }
    if let Ok(v) = parse_int(t) { return Ok(Op::Imm(v)); }
    Ok(Op::Sym(t.to_string()))
}
