//! 增量扫描器：递归遍历图库目录，与 images 表对比后增量入库（纯逻辑层）。
//!
//! 为什么独立成模块：扫描是引擎核心流程，必须脱离 Tauri 依赖才能被
//! 单元测试覆盖；进度经回调上报（依赖注入），由命令层（lib.rs）决定
//! 如何转发（emit 事件），本模块不感知 Tauri。
//!
//! 增量策略（为什么按 mtime+size 对比）：文件未变则跳过，避免每次扫描
//! 全量重读文件头；mtime+size 组合足以覆盖绝大多数变更场景（内容变则
//! 至少其一变化），代价是 O(1) 的 stat 而非 O(文件大小) 的解码。

use crate::engine::metadata;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// 图片扩展名白名单（大小写不敏感）。为什么不含 avif/ico 等：
/// 与产品定位（照片管理）对齐，且 image crate 未启用对应解码器。
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "tiff", "heic", "heif",
];

/// 图片存在状态（images.status 取值，契约单一来源）。
pub const STATUS_OK: &str = "ok";
pub const STATUS_MISSING: &str = "missing";

/// 扫描阶段（进度事件用）。
///
/// 为什么没有 Error 阶段：硬错误（根目录不存在/事务失败）由 scan_library
/// 以 Err 返回，命令层（lib.rs）负责转成 error 事件；引擎内单文件失败
/// 只计数不中断，不改变阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanPhase {
    Scanning,
    Done,
}

/// 进度事件负载：命令层据此转发给前端。
#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub phase: ScanPhase,
    pub scanned: u64,
    pub total: u64,
    pub message: Option<String>,
}

impl ScanProgress {
    pub fn scanning(scanned: u64, total: u64, message: Option<String>) -> Self {
        Self {
            phase: ScanPhase::Scanning,
            scanned,
            total,
            message,
        }
    }

    pub fn done(stats: &ScanStats) -> Self {
        Self {
            phase: ScanPhase::Done,
            scanned: stats.scanned,
            total: stats.total,
            message: Some(format!(
                "扫描完成：新增 {}，更新 {}，跳过 {}，标记缺失 {}，失败 {}",
                stats.inserted, stats.updated, stats.skipped, stats.missing, stats.failed
            )),
        }
    }
}

/// 一次扫描的统计结果（供 done 消息与测试断言）。
#[derive(Debug, Default, Clone, Copy)]
pub struct ScanStats {
    pub total: u64,
    pub scanned: u64,
    pub inserted: u64,
    pub updated: u64,
    pub skipped: u64,
    pub missing: u64,
    /// 尺寸/格式探测失败但已入库的文件数（降级：尺寸留空）。
    pub probe_failed: u64,
    /// 硬失败数（stat/DB 写入失败），不中断扫描。
    pub failed: u64,
}

/// 磁盘上发现的图片文件（第一遍遍历时取好元数据，避免第二遍重复 stat）。
struct DiskFile {
    path: PathBuf,
    size: i64,
    mtime: i64,
}

/// 单文件处理结果。
struct ProcessedFile {
    outcome: FileOutcome,
    probe_failed: bool,
}

enum FileOutcome {
    Inserted,
    Updated,
    Skipped,
}

/// 增量扫描图库目录。
///
/// 流程：① 校验根目录存在；② 遍历收集图片文件（含元数据）；③ 与
/// images 表对比，未变更跳过、新增插入、变更更新；④ 磁盘上已不存在的
/// 记录置 status='missing'（不删除，保留索引历史）。
///
/// 进度经 `on_progress` 回调上报：扫描中每处理一个文件回调一次，
/// 结束回调一次 Done。单文件失败只计数不中断（记录计数继续）。
pub fn scan_library(
    conn: &mut Connection,
    root: &Path,
    mut on_progress: impl FnMut(ScanProgress),
) -> Result<ScanStats, String> {
    if !root.is_dir() {
        return Err(format!("图库目录不存在或不是目录：{}", root.display()));
    }

    let mut stats = ScanStats::default();
    let files = collect_image_files(root, &mut stats)?;
    stats.total = files.len() as u64;
    on_progress(ScanProgress::scanning(0, stats.total, None));

    // 为什么加载全部行（含 missing）：文件被标记 missing 后若重新出现在
    // 磁盘，应走 UPDATE 恢复为 ok；若只加载非 missing 行，会撞 path 唯一约束。
    let existing = load_existing(conn)?;
    let mut seen: HashSet<String> = HashSet::with_capacity(files.len());

    {
        // 为什么整个写阶段一个事务：要么全部生效要么全部回滚，
        // 避免扫描中途崩溃留下「部分文件已入库」的半态。
        let tx = conn
            .transaction()
            .map_err(|e| format!("开启事务失败：{e}"))?;
        for (idx, file) in files.iter().enumerate() {
            let norm = normalize_path(&file.path);
            seen.insert(norm.clone());
            match process_file(&tx, file, &norm, &existing) {
                Ok(processed) => {
                    match processed.outcome {
                        FileOutcome::Inserted => stats.inserted += 1,
                        FileOutcome::Updated => stats.updated += 1,
                        FileOutcome::Skipped => stats.skipped += 1,
                    }
                    if processed.probe_failed {
                        stats.probe_failed += 1;
                    }
                }
                Err(e) => {
                    stats.failed += 1;
                    eprintln!("[warn] 处理文件失败（{}）: {e}", file.path.display());
                }
            }
            stats.scanned = idx as u64 + 1;
            on_progress(ScanProgress::scanning(stats.scanned, stats.total, None));
        }
        stats.missing = mark_missing(&tx, &seen)?;
        tx.commit().map_err(|e| format!("提交事务失败：{e}"))?;
    }

    on_progress(ScanProgress::done(&stats));
    Ok(stats)
}

/// 第一遍遍历：收集图片文件并提取元数据。
///
/// 为什么用 walkdir 而非手写递归：walkdir 自带目录遍历错误处理
/// （单目录不可读不中断整体）、符号链接控制与排序选项，避免重复造轮子。
fn collect_image_files(root: &Path, stats: &mut ScanStats) -> Result<Vec<DiskFile>, String> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                // 目录不可读/权限问题：跳过并继续，不中断整个扫描。
                stats.failed += 1;
                eprintln!("[warn] 扫描跳过条目：{e}");
                continue;
            }
        };
        if !entry.file_type().is_file() || !is_image_path(entry.path()) {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                stats.failed += 1;
                eprintln!("[warn] 读取文件信息失败（{}）: {e}", entry.path().display());
                continue;
            }
        };
        let mtime = match mtime_secs(&meta) {
            Ok(t) => t,
            Err(e) => {
                stats.failed += 1;
                eprintln!("[warn] {e}");
                continue;
            }
        };
        files.push(DiskFile {
            path: entry.path().to_path_buf(),
            size: meta.len() as i64,
            mtime,
        });
    }
    Ok(files)
}

/// 判断路径是否为白名单内的图片文件（扩展名大小写不敏感）。
pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| IMAGE_EXTENSIONS.iter().any(|x| ext.eq_ignore_ascii_case(x)))
        .unwrap_or(false)
}

/// 加载 images 表全部行：path → (file_size, mtime, status)。
fn load_existing(
    conn: &Connection,
) -> Result<HashMap<String, (Option<i64>, Option<i64>, String)>, String> {
    let mut stmt = conn
        .prepare("SELECT path, file_size, mtime, status FROM images")
        .map_err(|e| format!("准备查询失败：{e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| format!("查询索引失败：{e}"))?;
    let mut map = HashMap::new();
    for row in rows {
        let (path, size, mtime, status) = row.map_err(|e| format!("读取索引行失败：{e}"))?;
        map.insert(normalize_str(&path), (size, mtime, status));
    }
    Ok(map)
}

/// 处理单个文件：对比后插入/更新/跳过。
fn process_file(
    conn: &Connection,
    file: &DiskFile,
    norm: &str,
    existing: &HashMap<String, (Option<i64>, Option<i64>, String)>,
) -> Result<ProcessedFile, String> {
    // 未变更且状态为 ok → 跳过，不重读文件头。
    // 为什么要求 status==ok：文件可能因「扫描过其他目录」被标 missing，
    // 重扫原目录时 mtime+size 未变但状态是 missing，必须走 UPDATE 恢复为 ok，
    // 否则列表（排除 missing）会一直为空（实测 bug：重扫后图片列表不显示）。
    if let Some((old_size, old_mtime, old_status)) = existing.get(norm) {
        if old_status == STATUS_OK
            && *old_size == Some(file.size)
            && *old_mtime == Some(file.mtime)
        {
            return Ok(ProcessedFile {
                outcome: FileOutcome::Skipped,
                probe_failed: false,
            });
        }
    }

    let probe = metadata::probe_image(&file.path);
    let probe_failed = probe.is_err();
    let (width, height, format) = match probe {
        Ok(p) => (Some(p.width as i64), Some(p.height as i64), Some(p.format)),
        Err(e) => {
            // 探测失败仍入库（尺寸/格式留空）：该文件按扩展名确属图片，
            // 且入库后下次扫描可跳过，避免反复探测同一坏文件。
            eprintln!("[warn] 图片探测失败（{}）: {e}", file.path.display());
            (None, None, None)
        }
    };

    let scanned_at = now_iso();
    let outcome = if existing.contains_key(norm) {
        conn.execute(
            "UPDATE images SET file_size=?1, mtime=?2, width=?3, height=?4, format=?5,
             status=?6, scanned_at=?7 WHERE path=?8",
            rusqlite::params![
                file.size,
                file.mtime,
                width,
                height,
                format,
                STATUS_OK,
                scanned_at,
                norm
            ],
        )
        .map_err(|e| format!("更新索引失败：{e}"))?;
        FileOutcome::Updated
    } else {
        conn.execute(
            "INSERT INTO images (path, file_size, mtime, width, height, format, status, scanned_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                norm,
                file.size,
                file.mtime,
                width,
                height,
                format,
                STATUS_OK,
                scanned_at
            ],
        )
        .map_err(|e| format!("插入索引失败：{e}"))?;
        FileOutcome::Inserted
    };
    Ok(ProcessedFile {
        outcome,
        probe_failed,
    })
}

/// 磁盘上已不存在的记录置 status='missing'（不删除，保留索引历史）。
fn mark_missing(conn: &Connection, seen: &HashSet<String>) -> Result<u64, String> {
    let mut stmt = conn
        .prepare("SELECT id, path FROM images WHERE status != ?1")
        .map_err(|e| format!("准备查询失败：{e}"))?;
    let rows = stmt
        .query_map(rusqlite::params![STATUS_MISSING], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| format!("查询索引失败：{e}"))?;
    let mut count = 0u64;
    for row in rows {
        let (id, path) = row.map_err(|e| format!("读取索引行失败：{e}"))?;
        // 为什么比较前再归一化：历史数据可能存过反斜杠路径，
        // 归一化后与本次扫描的磁盘路径集合对齐。
        if !seen.contains(&normalize_str(&path)) {
            conn.execute(
                "UPDATE images SET status=?1 WHERE id=?2",
                rusqlite::params![STATUS_MISSING, id],
            )
            .map_err(|e| format!("标记缺失失败：{e}"))?;
            count += 1;
        }
    }
    Ok(count)
}

/// 路径归一化为正斜杠（Windows 反斜杠 → 正斜杠）。
///
/// 为什么统一正斜杠：同一文件在不同 API（walkdir/前端/用户输入）下
/// 可能呈现反斜杠或正斜杠两种形态，不归一化会导致 path 唯一约束
/// 下同一文件重复入库。
fn normalize_path(p: &Path) -> String {
    normalize_str(&p.to_string_lossy())
}

fn normalize_str(s: &str) -> String {
    s.replace('\\', "/")
}

/// 文件修改时间 → Unix 秒。
fn mtime_secs(meta: &std::fs::Metadata) -> Result<i64, String> {
    meta.modified()
        .map_err(|e| format!("读取修改时间失败：{e}"))?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .map_err(|e| format!("修改时间早于 Unix 纪元：{e}"))
}

/// 当前 UTC 时间，格式 `YYYY-MM-DD HH:MM:SS`。
///
/// 为什么手写日历换算：仅为格式化时间引入 chrono 依赖不划算
/// （CODE-GUIDE：新依赖先评估必要性），civil_from_days 是标准算法。
fn now_iso() -> String {
    let now = std::time::SystemTime::now();
    let dur = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Unix 天数（自 1970-01-01）→ 公历 (年, 月, 日)。Howard Hinnant 算法。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::db;

    /// 临时目录：唯一子目录避免并行测试互相干扰，测试后清理。
    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("io_scan_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_png(path: &Path, w: u32, h: u32) {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([255u8, 0, 0, 255]));
        img.save(path).unwrap();
    }

    fn make_jpeg(path: &Path) {
        let img = image::RgbImage::from_pixel(8, 6, image::Rgb([0u8, 255, 0]));
        img.save(path).unwrap();
    }

    /// 执行一次扫描并收集全部进度事件。
    fn run_scan(conn: &mut Connection, root: &Path) -> (ScanStats, Vec<ScanProgress>) {
        let mut events = Vec::new();
        let stats = scan_library(conn, root, |p| events.push(p)).unwrap();
        (stats, events)
    }

    /// 首扫全部插入；二次扫描（mtime/size 未变）全部跳过；尺寸/格式入库正确。
    #[test]
    fn first_scan_inserts_second_scan_skips() {
        let dir = temp_dir("insert_skip");
        let root = dir.join("lib");
        std::fs::create_dir_all(&root).unwrap();
        make_png(&root.join("a.png"), 4, 4);
        make_jpeg(&root.join("b.jpg"));

        let db_path = dir.join("test.db");
        {
            let mut conn = db::open(&db_path).unwrap();

            let (stats, events) = run_scan(&mut conn, &root);
            assert_eq!(stats.total, 2);
            assert_eq!(stats.inserted, 2);
            assert_eq!(stats.skipped, 0);
            // 进度事件：先 scanning 后 done，done 的 scanned 等于 total。
            assert_eq!(events.last().unwrap().phase, ScanPhase::Done);
            assert_eq!(events.last().unwrap().scanned, 2);

            let (stats2, _) = run_scan(&mut conn, &root);
            assert_eq!(stats2.inserted, 0);
            assert_eq!(stats2.updated, 0);
            assert_eq!(stats2.skipped, 2);

            // 尺寸/格式已入库（png 4x4、jpeg 8x6）
            let (w, h, fmt): (i64, i64, String) = conn
                .query_row(
                    "SELECT width, height, format FROM images WHERE path LIKE '%a.png'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
            assert_eq!((w, h), (4, 4));
            assert_eq!(fmt, "png");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 文件内容变更（尺寸变化）→ 重扫更新而非跳过。
    #[test]
    fn changed_file_is_updated() {
        let dir = temp_dir("update");
        let root = dir.join("lib");
        std::fs::create_dir_all(&root).unwrap();
        let png = root.join("a.png");
        make_png(&png, 4, 4);

        let db_path = dir.join("test.db");
        {
            let mut conn = db::open(&db_path).unwrap();
            run_scan(&mut conn, &root);

            make_png(&png, 10, 10);
            let (stats, _) = run_scan(&mut conn, &root);
            assert_eq!(stats.updated, 1);
            assert_eq!(stats.skipped, 0);

            let (w, h): (i64, i64) = conn
                .query_row(
                    "SELECT width, height FROM images WHERE path LIKE '%a.png'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!((w, h), (10, 10));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 磁盘文件删除 → 重扫后 status='missing'（记录保留）。
    #[test]
    fn deleted_file_marked_missing() {
        let dir = temp_dir("missing");
        let root = dir.join("lib");
        std::fs::create_dir_all(&root).unwrap();
        let png = root.join("a.png");
        make_png(&png, 4, 4);

        let db_path = dir.join("test.db");
        {
            let mut conn = db::open(&db_path).unwrap();
            run_scan(&mut conn, &root);

            std::fs::remove_file(&png).unwrap();
            let (stats, _) = run_scan(&mut conn, &root);
            assert_eq!(stats.missing, 1);

            let status: String = conn
                .query_row(
                    "SELECT status FROM images WHERE path LIKE '%a.png'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(status, STATUS_MISSING);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 文件被标 missing 后重扫原目录 → 恢复为 ok（回归：跳过路径不恢复状态）。
    ///
    /// 场景：先扫目录 A，再扫目录 B 时 A 的文件被标 missing；重扫 A 时
    /// 文件未变（mtime+size 一致），若跳过路径不恢复状态，列表会一直为空。
    #[test]
    fn missing_file_restored_on_rescan() {
        let dir = temp_dir("restore");
        let root = dir.join("lib");
        std::fs::create_dir_all(&root).unwrap();
        let png = root.join("a.png");
        make_png(&png, 4, 4);

        let db_path = dir.join("test.db");
        {
            let mut conn = db::open(&db_path).unwrap();
            run_scan(&mut conn, &root);

            // 模拟「扫描过其他目录」：直接把状态置为 missing
            conn.execute(
                "UPDATE images SET status=?1",
                rusqlite::params![STATUS_MISSING],
            )
            .unwrap();

            // 重扫原目录：文件未变，应走 UPDATE 恢复为 ok（而非跳过）
            let (stats, _) = run_scan(&mut conn, &root);
            assert_eq!(stats.updated, 1);
            assert_eq!(stats.skipped, 0);

            let status: String = conn
                .query_row(
                    "SELECT status FROM images WHERE path LIKE '%a.png'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(status, STATUS_OK);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 非图片文件（txt/bin）被忽略，只索引图片。
    #[test]
    fn non_image_files_ignored() {
        let dir = temp_dir("nonimage");
        let root = dir.join("lib");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("notes.txt"), "hello").unwrap();
        std::fs::write(root.join("data.bin"), [0u8, 1, 2]).unwrap();
        make_png(&root.join("real.png"), 4, 4);

        let db_path = dir.join("test.db");
        {
            let mut conn = db::open(&db_path).unwrap();
            let (stats, _) = run_scan(&mut conn, &root);
            assert_eq!(stats.total, 1);
            assert_eq!(stats.inserted, 1);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 扩展名大小写不敏感（.PNG 与 .png 同样识别）。
    #[test]
    fn extension_matching_is_case_insensitive() {
        assert!(is_image_path(Path::new("a.PNG")));
        assert!(is_image_path(Path::new("b.JpEg")));
        assert!(is_image_path(Path::new("c.heic")));
        assert!(!is_image_path(Path::new("d.txt")));
        assert!(!is_image_path(Path::new("noext")));
    }

    /// 根目录不存在 → 返回可读错误而非静默成功。
    #[test]
    fn nonexistent_root_returns_error() {
        let dir = temp_dir("badroot");
        let db_path = dir.join("test.db");
        {
            let mut conn = db::open(&db_path).unwrap();
            let err = scan_library(&mut conn, &dir.join("nope"), |_| {}).unwrap_err();
            assert!(err.contains("不存在"));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}