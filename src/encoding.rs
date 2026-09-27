//! 源文件编码适配。
//!
//! 解码优先级：
//!   1) UTF-8（含 BOM，BOM 静默剥离）
//!   2) 非法 UTF-8 → 回退系统 ANSI 代码页
//!      简体 GBK/GB2312(936)、繁体 Big5(950)、日文 Shift-JIS(932)、韩文 EUC-KR(949) 等
//!
//! 这样用旧编辑器存成 GBK/Shift-JIS 的 `.gt` 源码也能直接编译。

use std::path::{Path, PathBuf};

#[link(name = "kernel32")]
extern "system" {
    fn GetACP() -> u32;
    fn MultiByteToWideChar(
        code_page: u32,
        flags: u32,
        src: *const u8,
        src_len: i32,
        dst: *mut u16,
        dst_len: i32,
    ) -> i32;
}

/// 读取并解码源文件，返回 (源码文本, 编码提示)。
/// 编码提示仅在「非 UTF-8、靠系统代码页猜出来」时有值，便于用户核对。
pub fn read_source(path: &Path) -> Result<(String, Option<String>), String> {
    let bytes = std::fs::read(path).map_err(|e| {
        format!(
            "无法读取源文件：{}\n  解析为绝对路径：{}\n  原因：{}\n  提示：相对路径按当前工作目录解析。",
            path.display(),
            abs_of(path).display(),
            e
        )
    })?;
    decode(&bytes)
}

pub fn decode(bytes: &[u8]) -> Result<(String, Option<String>), String> {
    // 1) UTF-8 BOM：静默剥离（Windows 编辑器常见）
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return match std::str::from_utf8(rest) {
            Ok(s) => Ok((s.to_string(), None)),
            Err(_) => Err(
                "源文件带 UTF-8 BOM，但正文不是合法 UTF-8（可能实际是 GBK 等编码并存了 BOM）"
                    .into(),
            ),
        };
    }

    // 2) 纯 UTF-8（含 ASCII）
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok((s.to_string(), None));
    }

    // 3) 回退系统 ANSI 代码页
    unsafe {
        let cp = GetACP();
        let n = MultiByteToWideChar(
            cp,
            0,
            bytes.as_ptr(),
            bytes.len() as i32,
            std::ptr::null_mut(),
            0,
        );
        if n <= 0 {
            return Err(format!(
                "源文件不是合法 UTF-8，且无法按系统代码页 CP{} 解码；请另存为 UTF-8 后重试",
                cp
            ));
        }
        let mut wide = vec![0u16; n as usize];
        let m = MultiByteToWideChar(
            cp,
            0,
            bytes.as_ptr(),
            bytes.len() as i32,
            wide.as_mut_ptr(),
            n,
        );
        if m <= 0 {
            return Err(format!("按系统代码页 CP{} 解码源文件失败", cp));
        }
        Ok((
            String::from_utf16_lossy(&wide[..m as usize]),
            Some(format!(
                "{}（系统代码页 CP{}，已自动转为 UTF-8）",
                cp_name(cp),
                cp
            )),
        ))
    }
}

fn cp_name(cp: u32) -> &'static str {
    match cp {
        932 => "Shift-JIS 日文",
        936 => "GBK/GB2312 简体中文",
        949 => "EUC-KR 韩文",
        950 => "Big5 繁体中文",
        874 => "Windows-874 泰文",
        1251 => "Windows-1251 西里尔",
        1252 => "Windows-1252 西欧",
        65001 => "UTF-8",
        _ => "ANSI",
    }
}

fn abs_of(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}