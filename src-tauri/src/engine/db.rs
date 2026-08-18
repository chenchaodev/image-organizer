//! SQLite 连接、初始化、建表与 settings 读写（纯逻辑层，不依赖 Tauri）。
//!
//! 为什么独立成模块：引擎核心逻辑必须脱离 Tauri 依赖才能被单元测试覆盖；
//! 命令层（lib.rs）只做状态持有与错误归一化。

use rusqlite::Connection;
use std::path::Path;

/// 建表 SQL（ADR-02 数据模型）。为什么一次 execute_batch 执行全部：
/// 保持 schema 与 ADR 逐字对应，后续演进用 ALTER 增量补充，不留半初始化态。
const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS folders (
    id INTEGER PRIMARY KEY,
    path TEXT UNIQUE NOT NULL,
    last_scanned_at TEXT
);
CREATE TABLE IF NOT EXISTS images (
    id INTEGER PRIMARY KEY,
    path TEXT UNIQUE NOT NULL,
    file_size INTEGER,
    mtime INTEGER,
    width INTEGER,
    height INTEGER,
    format TEXT,
    phash TEXT,
    status TEXT,
    scanned_at TEXT
);
CREATE TABLE IF NOT EXISTS thumbnails (
    image_id INTEGER PRIMARY KEY,
    size INTEGER,
    path TEXT,
    generated_at TEXT
);
CREATE TABLE IF NOT EXISTS dedup_groups (
    id INTEGER PRIMARY KEY,
    created_at TEXT
);
CREATE TABLE IF NOT EXISTS dedup_members (
    group_id INTEGER,
    image_id INTEGER,
    similarity REAL
);
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT
);
";

/// 打开（必要时创建）数据库文件并完成初始化。
pub fn open(db_path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path)?;
    init(&conn)?;
    Ok(conn)
}

/// 初始化：PRAGMA + 建表。
///
/// 为什么 WAL：后续扫描写入与 UI 查询并发时读写互不阻塞，契合
/// 「索引写 + 界面读」并行的产品形态；为什么 foreign_keys=ON：
/// dedup_members 引用 dedup_groups/images，为后续级联清理预留约束。
fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(())
}

/// 写入一条设置（upsert：已存在则覆盖，settings.key 为主键）。
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

/// 读取一条设置，键不存在时返回 None（与「未设置」语义对齐，不视为错误）。
pub fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query(rusqlite::params![key])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ADR-02 的 6 张业务表全部建齐（排除 SQLite 内部 sqlite_* 表）。
    #[test]
    fn schema_creates_all_expected_tables() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap();
        let mut names: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        names.sort();

        let mut expected = vec![
            "dedup_groups",
            "dedup_members",
            "folders",
            "images",
            "settings",
            "thumbnails",
        ];
        expected.sort();
        assert_eq!(names, expected);
    }

    /// settings 写入后可原样读回；未设置返回 None；重复写覆盖旧值。
    #[test]
    fn settings_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();

        set_setting(&conn, "library_path", "C:/photos").unwrap();
        assert_eq!(
            get_setting(&conn, "library_path").unwrap(),
            Some("C:/photos".to_string())
        );
        assert_eq!(get_setting(&conn, "missing").unwrap(), None);

        // 覆盖写：upsert 语义
        set_setting(&conn, "library_path", "D:/pics").unwrap();
        assert_eq!(
            get_setting(&conn, "library_path").unwrap(),
            Some("D:/pics".to_string())
        );
    }

    /// WAL 只对文件库生效（内存库不支持，pragma 会回落到 memory），
    /// 因此这里用临时文件库验证 journal_mode 确为 wal。
    #[test]
    fn journal_mode_is_wal_on_file_db() {
        let dir = std::env::temp_dir().join(format!("io_db_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test.db");
        {
            let conn = open(&db_path).unwrap();
            let mode: String = conn
                .pragma_query_value(None, "journal_mode", |row| row.get(0))
                .unwrap();
            assert_eq!(mode, "wal");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
