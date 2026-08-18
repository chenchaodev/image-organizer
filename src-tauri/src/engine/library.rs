//! 图库管理：library 路径的读写与校验（纯逻辑层，不依赖 Tauri）。
//!
//! 为什么独立于 db.rs：db.rs 是通用 settings 存储，不感知键语义；
//! library.rs 承载「图库」业务含义——路径 key 单一来源 + 目录存在性校验。

use rusqlite::Connection;
use std::path::Path;

/// settings 表中图库路径的 key（契约单一来源，消费方引用此常量）。
pub const LIBRARY_PATH_KEY: &str = "library_path";

/// 图库业务错误：路径非法（不存在/非目录）或底层存储失败。
#[derive(Debug)]
pub enum LibraryError {
    InvalidPath(String),
    Db(rusqlite::Error),
}

impl std::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LibraryError::InvalidPath(p) => {
                write!(f, "图库目录不存在或不是目录：{p}")
            }
            LibraryError::Db(e) => write!(f, "数据库错误：{e}"),
        }
    }
}

impl std::error::Error for LibraryError {}

impl From<rusqlite::Error> for LibraryError {
    fn from(e: rusqlite::Error) -> Self {
        LibraryError::Db(e)
    }
}

/// 读取已设置的图库路径；未设置返回 None（与「尚未选择」语义对齐）。
pub fn get_library_path(conn: &Connection) -> Result<Option<String>, LibraryError> {
    Ok(crate::engine::db::get_setting(conn, LIBRARY_PATH_KEY)?)
}

/// 设置图库路径并回写 settings。先校验目录存在且是目录——
/// 为什么必须在写入前校验：坏路径会在后续扫描/索引启动时静默失败，
/// 尽早报错（fail fast）比延迟暴露问题可诊断性更好。
pub fn set_library_path(conn: &Connection, path: &str) -> Result<String, LibraryError> {
    let dir = Path::new(path);
    if !dir.is_dir() {
        return Err(LibraryError::InvalidPath(path.to_string()));
    }
    crate::engine::db::set_setting(conn, LIBRARY_PATH_KEY, path)?;
    Ok(path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::db;

    /// set/get 往返 + 未设置态 + 非法路径拒绝，覆盖命令层全部行为路径。
    #[test]
    fn library_path_roundtrip() {
        let dir = std::env::temp_dir().join(format!("io_lib_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        {
            let conn = db::open(&dir.join("test.db")).unwrap();

            // 初始未设置
            assert_eq!(get_library_path(&conn).unwrap(), None);

            // 设置合法目录后能读回（路径带中文/空格，验证纯字符串处理无需转义）
            let photos = dir.join("我的 照片");
            std::fs::create_dir_all(&photos).unwrap();
            let saved = set_library_path(&conn, photos.to_str().unwrap()).unwrap();
            assert_eq!(saved, photos.to_str().unwrap());
            assert_eq!(
                get_library_path(&conn).unwrap(),
                Some(photos.to_str().unwrap().to_string())
            );

            // 不存在的路径被拒绝，且不污染 settings
            let err = set_library_path(&conn, "Z:/does/not/exist").unwrap_err();
            assert!(matches!(err, LibraryError::InvalidPath(_)));
            assert_eq!(
                get_library_path(&conn).unwrap(),
                Some(photos.to_str().unwrap().to_string())
            );
            // 先关闭连接再清理临时目录：WAL 模式下连接持有文件句柄，
            // 未 drop 前 remove_dir_all 会因文件占用失败。
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
