//! 临时文件管理：所有编译/解释中间产物统一放在 `%TEMP%\gtc\<标签>_<pid>\`，
//! 正常结束后自动删除整个目录，不在源码目录留下任何残留。
//!
//! 约定（与项目既有规范一致）：
//!   - 中间文件只进系统临时目录，源码目录只出现最终产物（.exe / .ll）
//!   - 成功或失败都会清理，除非显式 `keep()`（调试用）

use std::path::{Path, PathBuf};

/// 一个受管的临时目录，`Drop` 时自动递归删除
pub struct TempDir {
    path: PathBuf,
    keep: bool,
}

impl TempDir {
    /// 在 `%TEMP%\gtc\` 下创建 `<tag>_<pid>` 目录（若已存在先清空）
    pub fn new(tag: &str) -> Result<TempDir, String> {
        let mut base = std::env::temp_dir();
        base.push("gtc");
        let dir = base.join(format!("{}_{}", tag, std::process::id()));
        // 上次同 pid 的残留（pid 复用）先清掉，避免旧文件被误用
        if dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("无法创建临时目录 {}：{}", dir.display(), e))?;
        Ok(TempDir { path: dir, keep: false })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 目录内某个文件的完整路径
    pub fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    /// 保留目录（不自动删除），用于排查问题
    pub fn keep(&mut self) {
        self.keep = true;
    }

    pub fn is_kept(&self) -> bool {
        self.keep
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_under_system_temp_and_cleans_up() {
        let p;
        {
            let td = TempDir::new("gtc_tmp_test").unwrap();
            p = td.path().to_path_buf();
            assert!(p.exists());
            assert!(p.starts_with(std::env::temp_dir()));
            assert!(p.to_string_lossy().contains("gtc"));
            std::fs::write(td.file("x.c"), "int x;").unwrap();
            assert!(td.file("x.c").exists());
        }
        assert!(!p.exists(), "临时目录应在 Drop 时被删除");
    }

    #[test]
    fn keep_prevents_removal() {
        let p;
        {
            let mut td = TempDir::new("gtc_tmp_keep_test").unwrap();
            p = td.path().to_path_buf();
            td.keep();
            assert!(td.is_kept());
        }
        assert!(p.exists());
        let _ = std::fs::remove_dir_all(&p);
    }
}