//! 图片查询：列表分页/计数/详情（纯逻辑层，不依赖 Tauri）。
//!
//! 为什么独立于 db.rs：db.rs 是通用存储原语（连接/建表/settings），
//! 不感知「图片」业务语义；本模块承载图片查询契约（status 过滤、
//! 排序、分页），供命令层（lib.rs）直接调用。

use crate::engine::metadata;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;

/// 图片行数据（不含 EXIF；EXIF 属文件内容而非索引，按需读取）。
#[derive(Debug, Clone)]
pub struct ImageRow {
    pub id: i64,
    pub path: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub format: Option<String>,
    pub file_size: Option<i64>,
    pub mtime: Option<i64>,
}

/// 分页查询图片列表（按 id 升序，排除 missing）。
///
/// 为什么按 id 排序：id 自增即入库顺序，稳定且无需额外索引；
/// 后续如需按拍摄时间/名称排序再扩展。
pub fn list_images(conn: &Connection, offset: u32, limit: u32) -> Result<Vec<ImageRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, path, width, height, format, file_size, mtime
             FROM images WHERE status != 'missing'
             ORDER BY id LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| format!("准备查询失败：{e}"))?;
    let rows = stmt
        .query_map(rusqlite::params![limit, offset], row_from)
        .map_err(|e| format!("查询图片列表失败：{e}"))?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|e| format!("读取图片行失败：{e}"))?);
    }
    Ok(items)
}

/// 非 missing 图片总数（分页 total 用）。
pub fn count_images(conn: &Connection) -> Result<u32, String> {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM images WHERE status != 'missing'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("统计图片数量失败：{e}"))?;
    Ok(n as u32)
}

/// 按 id 查询单张图片；不存在返回 None。
pub fn query_image(conn: &Connection, id: i64) -> Result<Option<ImageRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, path, width, height, format, file_size, mtime
             FROM images WHERE id = ?1",
        )
        .map_err(|e| format!("准备查询失败：{e}"))?;
    let mut rows = stmt
        .query_map(rusqlite::params![id], row_from)
        .map_err(|e| format!("查询图片详情失败：{e}"))?;
    match rows.next() {
        Some(Ok(row)) => Ok(Some(row)),
        Some(Err(e)) => Err(format!("查询图片详情失败：{e}")),
        None => Ok(None),
    }
}

/// 图片详情 = 索引行 + 文件 EXIF（无 EXIF 时为空 map，失败降级）。
pub fn get_image_detail(conn: &Connection, id: i64) -> Result<(ImageRow, HashMap<String, String>), String> {
    let row = query_image(conn, id)?.ok_or_else(|| format!("图片不存在：id={id}"))?;
    let exif = metadata::read_exif(Path::new(&row.path));
    Ok((row, exif))
}

fn row_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<ImageRow> {
    Ok(ImageRow {
        id: row.get(0)?,
        path: row.get(1)?,
        width: row.get(2)?,
        height: row.get(3)?,
        format: row.get(4)?,
        file_size: row.get(5)?,
        mtime: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::db;

    /// 临时目录：唯一子目录避免并行测试互相干扰，测试后清理。
    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("io_img_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn insert(conn: &Connection, path: &str, status: &str) {
        conn.execute(
            "INSERT INTO images (path, file_size, mtime, width, height, format, status, scanned_at)
             VALUES (?1, 100, 1000, 4, 4, 'png', ?2, '2026-01-01 00:00:00')",
            rusqlite::params![path, status],
        )
        .unwrap();
    }

    /// 列表排除 missing、按 id 升序、分页生效；计数与详情正确。
    #[test]
    fn list_count_and_detail() {
        let dir = temp_dir("query");
        let db_path = dir.join("test.db");
        {
            let conn = db::open(&db_path).unwrap();
            insert(&conn, "C:/photos/a.png", "ok");
            insert(&conn, "C:/photos/b.jpg", "missing");
            insert(&conn, "C:/photos/c.png", "ok");

            assert_eq!(count_images(&conn).unwrap(), 2);

            let items = list_images(&conn, 0, 10).unwrap();
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].path, "C:/photos/a.png");
            assert_eq!(items[1].path, "C:/photos/c.png");
            assert_eq!(items[0].width, Some(4));
            assert_eq!(items[0].format.as_deref(), Some("png"));

            // 分页：offset=1 只返回第二条
            let page = list_images(&conn, 1, 1).unwrap();
            assert_eq!(page.len(), 1);
            assert_eq!(page[0].path, "C:/photos/c.png");

            // 详情：存在返回行，不存在返回 None
            let (row, exif) = get_image_detail(&conn, items[0].id).unwrap();
            assert_eq!(row.path, "C:/photos/a.png");
            assert!(exif.is_empty()); // 测试路径不存在 → EXIF 降级为空 map
            assert!(query_image(&conn, 999).unwrap().is_none());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}