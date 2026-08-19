//! 缩略图生成与缓存（纯逻辑层，不依赖 Tauri）。
//!
//! 为什么独立成模块：缩略图是 CPU/IO 密集操作（HEIC 全尺寸解码 100-300ms），
//! 必须脱离 Tauri 依赖才能被单元测试覆盖；cache_dir 由调用方（命令层）传入，
//! 本模块不感知 Tauri 的路径 API（硬约束：引擎层不得依赖 Tauri）。
//!
//! 缓存策略：首次访问生成 {image_id}.webp 并写入 thumbnails 表；后续访问
//! 命中表记录且文件在磁盘即直接返回，不重复解码。解码失败（损坏文件等）
//! 返回 None 且不写库，前端显示占位图。

use crate::engine::{images, metadata, scanner};
use image::{DynamicImage, RgbaImage};
use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};

/// 缩略图最长边（像素）。为什么 256：网格浏览的缩略图尺寸，
/// 256px 在 2x 高分屏下显示 128dp 网格足够清晰，且体积/解码开销可控。
pub const THUMB_SIZE: u32 = 256;

/// 查询源图路径与缓存命中（调用方持锁，短暂）。
///
/// 返回 (source_path, cached_path)：source 为 None 表示索引行不存在或
/// status='missing'；cached 为 Some 表示缓存命中（表记录 + 文件在盘）。
///
/// 为什么拆出查询：解码是 CPU 密集（HEIC 全尺寸 100-300ms），若在持锁
/// 期间执行，多个并发请求会串行排队（实测大库缩略图长时间不显示）；
/// 拆出后解码在 blocking 池并行，锁只覆盖毫秒级查询/写入。
pub fn thumbnail_lookup(
    conn: &Connection,
    image_id: i64,
) -> Result<(Option<String>, Option<PathBuf>), String> {
    // ① 索引行不存在 → source None
    let Some(row) = images::query_image(conn, image_id)? else {
        return Ok((None, None));
    };
    // ② status='missing'（磁盘文件已删除）→ source None
    // 为什么单独查 status：images::ImageRow 契约不含 status 字段（images.rs 不可改），
    // 状态语义由 scanner 常量承载（契约单一来源）。
    let status: String = conn
        .query_row(
            "SELECT status FROM images WHERE id = ?1",
            rusqlite::params![image_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("查询图片状态失败：{e}"))?;
    if status == scanner::STATUS_MISSING {
        return Ok((None, None));
    }

    // ③ 缓存命中：表记录存在且文件在磁盘 → cached Some
    // 为什么同时校验文件存在：表记录可能因缓存目录被清理/移动而失效，
    // 只查表不查盘会返回一个不存在的路径。
    let cached: Option<String> = conn
        .query_row(
            "SELECT path FROM thumbnails WHERE image_id = ?1",
            rusqlite::params![image_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("查询缩略图缓存失败：{e}"))?;
    if let Some(p) = cached {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok((Some(row.path), Some(p)));
        }
    }
    Ok((Some(row.path), None))
}

/// 解码 + 缩略 + 保存（不持锁，可并行）。
///
/// 解码失败返回 Ok(None)（前端显示占位图，不写缓存库避免反复生成失败记录）；
/// 保存失败返回 Err（真正的系统故障）。
pub fn generate_thumbnail(
    cache_dir: &Path,
    source_path: &str,
    image_id: i64,
) -> Result<Option<(PathBuf, u32)>, String> {
    let img = match decode_image(Path::new(source_path)) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("[warn] 缩略图解码失败（id={image_id}）: {e}");
            return Ok(None);
        }
    };
    let thumb = img.thumbnail(THUMB_SIZE, THUMB_SIZE);
    let cache_path = cache_dir.join(format!("{image_id}.webp"));
    save_webp(&thumb, &cache_path)?;
    // size 存实际最长边：小图缩略后可能小于 THUMB_SIZE，存实际值供分级缓存判断。
    Ok(Some((cache_path, thumb.width().max(thumb.height()))))
}

/// 写缓存索引（调用方持锁，短暂）。
///
/// 为什么 INSERT OR IGNORE：并发请求同一 id 时，后到者插入被主键冲突
/// 忽略，不覆盖先到者的记录（幂等）。
pub fn record_thumbnail(
    conn: &Connection,
    image_id: i64,
    cache_path: &Path,
    size: u32,
) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO thumbnails (image_id, size, path, generated_at)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![image_id, size as i64, cache_path.to_string_lossy(), now_iso()],
    )
    .map_err(|e| format!("写入缩略图索引失败：{e}"))?;
    Ok(())
}

/// 后台补齐缺失缩略图：为所有非 missing 且无缩略图的图片生成缩略图。
///
/// 为什么独立于按需生成：扫描只更新索引表不生成缩略图，历史扫描过的
/// 图片可能一直缺缩略图（实测：重扫不更新）；扫描完成后调用本函数
/// 一次性补齐，浏览时即缓存命中。
///
/// 为什么 std::thread::scope：解码是 CPU 密集且不持连接锁，用 scoped
/// 线程并行解码（concurrency 个），结果收集后统一写库（连接只短暂持锁）。
/// 返回成功生成数量。
pub fn backfill_missing(
    conn: &Connection,
    cache_dir: &Path,
    concurrency: usize,
) -> Result<u32, String> {
    // 查询缺缩略图的非 missing 图片
    let mut stmt = conn
        .prepare(
            "SELECT i.id, i.path FROM images i
             LEFT JOIN thumbnails t ON t.image_id = i.id
             WHERE i.status != ?1 AND t.image_id IS NULL",
        )
        .map_err(|e| format!("准备查询失败：{e}"))?;
    let rows = stmt
        .query_map(rusqlite::params![scanner::STATUS_MISSING], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| format!("查询缺缩略图图片失败：{e}"))?;
    let jobs: Vec<(i64, String)> = rows
        .map(|r| r.map_err(|e| format!("读取图片行失败：{e}")))
        .collect::<Result<_, _>>()?;
    let total = jobs.len();
    if total == 0 {
        return Ok(0);
    }

    // 并行解码（不持连接锁），结果收集后统一写库
    let queue = std::sync::Mutex::new(jobs);
    let results = std::sync::Mutex::new(Vec::with_capacity(total));
    std::thread::scope(|s| {
        for _ in 0..concurrency {
            let queue = &queue;
            let results = &results;
            s.spawn(move || loop {
                let job = queue.lock().unwrap().pop();
                let Some((id, path)) = job else { break };
                match generate_thumbnail(cache_dir, &path, id) {
                    Ok(Some((p, size))) => results.lock().unwrap().push((id, p, size)),
                    Ok(None) => { /* 解码失败跳过（前端占位） */ }
                    Err(e) => eprintln!("[warn] 缩略图生成失败（id={id}）: {e}"),
                }
            });
        }
    });

    // 统一写缓存索引（短暂持锁）
    let results = results.lock().unwrap();
    for (id, p, size) in results.iter() {
        record_thumbnail(conn, *id, p, *size)?;
    }
    Ok(results.len() as u32)
}

/// 解码图片为 DynamicImage；heic/heif 走 libheif，其余走 image crate。
fn decode_image(path: &Path) -> Result<DynamicImage, String> {
    if metadata::is_heic_path(path) {
        decode_heic(path)
    } else {
        image::open(path).map_err(|e| format!("解码图片失败：{e}"))
    }
}

/// 用 libheif 解码 HEIC/HEIF。
///
/// 为什么优先内嵌缩略图：HEIF 容器常带内嵌缩略图（iPhone 拍摄即带），
/// 解码毫秒级；全尺寸解码 100-300ms 且 12MP 全解码约 48MB 内存（ADR-03）。
fn decode_heic(path: &Path) -> Result<DynamicImage, String> {
    let path_str = path
        .to_str()
        .ok_or_else(|| format!("路径不是有效 UTF-8：{}", path.display()))?;
    let ctx = HeifContext::read_from_file(path_str)
        .map_err(|e| format!("读取 HEIC 文件失败：{e}"))?;
    let primary = ctx
        .primary_image_handle()
        .map_err(|e| format!("获取 HEIC 主图句柄失败：{e}"))?;

    // 内嵌缩略图优先；无内嵌才全尺寸解码。
    let handle = if primary.number_of_thumbnails() > 0 {
        let mut ids = [0u32; 1];
        primary.thumbnail_ids(&mut ids);
        if ids[0] != 0 {
            primary
                .thumbnail(ids[0])
                .map_err(|e| format!("获取 HEIC 内嵌缩略图失败：{e}"))?
        } else {
            primary
        }
    } else {
        primary
    };

    let lib = LibHeif::new();
    let img = lib
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
        .map_err(|e| format!("HEIC 解码失败：{e}"))?;
    heif_image_to_dynamic(&img)
}

/// libheif 解码结果 → DynamicImage（8 位 RGBA）。
///
/// 为什么请求 Rgba 而非 Rgb：统一输出 RGBA8，后续缩略/编码无需分支；
/// 无 alpha 的图 libheif 会填 255（不透明）。
fn heif_image_to_dynamic(img: &libheif_rs::Image) -> Result<DynamicImage, String> {
    let width = img.width();
    let height = img.height();
    if width == 0 || height == 0 {
        return Err("HEIC 解码结果尺寸为 0".to_string());
    }
    let planes = img.planes();
    let plane = planes
        .interleaved
        .ok_or_else(|| "HEIC 解码结果不是交错格式".to_string())?;
    // 请求 Rgb(Rgba) 时 libheif 输出 32bpp 交错 RGBA；HDR 源也会被转换到 8 位。
    if plane.storage_bits_per_pixel != 32 {
        return Err(format!(
            "HEIC 位深不支持：{} bpp（仅支持 8 位 RGBA）",
            plane.storage_bits_per_pixel
        ));
    }
    let row_size = width as usize * 4;
    if row_size > plane.stride {
        return Err("HEIC 行宽大于 stride".to_string());
    }
    // 逐行拷贝：stride 可能大于 row_size（行对齐填充），不能整块 memcpy。
    let mut buf = vec![0u8; (height as usize) * row_size];
    for y in 0..height as usize {
        let src = &plane.data[y * plane.stride..y * plane.stride + row_size];
        buf[y * row_size..(y + 1) * row_size].copy_from_slice(src);
    }
    let rgba = RgbaImage::from_raw(width, height, buf)
        .ok_or_else(|| "HEIC 像素缓冲构造失败".to_string())?;
    Ok(DynamicImage::ImageRgba8(rgba))
}

/// 以无损 WebP 保存缩略图。
///
/// 为什么无损：image 0.25.10 的 WebP 编码器仅支持无损（new_lossless，
/// 源码 encoder.rs 注明 only lossless encoding is supported），无质量参数；
/// 256px 缩略图无损 WebP 约 10-30KB，体积可接受。后续如需有损压缩
/// 可换 libwebp 或升级 image（记 ROADMAP 已知限制）。
fn save_webp(img: &DynamicImage, path: &Path) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("创建缩略图文件失败：{e}"))?;
    let encoder = image::codecs::webp::WebPEncoder::new_lossless(file);
    encoder
        .encode(img.as_bytes(), img.width(), img.height(), img.color().into())
        .map_err(|e| format!("编码缩略图失败：{e}"))
}

/// 当前 UTC 时间，格式 `YYYY-MM-DD HH:MM:SS`。
///
/// 为什么手写日历换算：仅为格式化时间引入 chrono 依赖不划算
/// （CODE-GUIDE：新依赖先评估必要性），civil_from_days 是标准算法
/// （与 scanner.rs 同思路，本地实现避免跨模块私有函数耦合）。
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
        let dir = std::env::temp_dir().join(format!("io_thumb_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 插入图片索引行，返回自增 id。
    fn insert(conn: &Connection, path: &str, status: &str) -> i64 {
        conn.execute(
            "INSERT INTO images (path, file_size, mtime, width, height, format, status, scanned_at)
             VALUES (?1, 100, 1000, 4, 4, 'png', ?2, '2026-01-01 00:00:00')",
            rusqlite::params![path, status],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    /// 模拟命令层完整流程：查询 → 生成 → 记录（与 lib.rs 的 get_thumbnail_path 同构）。
    fn get_thumb(conn: &Connection, cache: &Path, id: i64) -> Result<Option<PathBuf>, String> {
        let (source, cached) = thumbnail_lookup(conn, id)?;
        if let Some(p) = cached {
            return Ok(Some(p));
        }
        let Some(source) = source else { return Ok(None); };
        let Some((path, size)) = generate_thumbnail(cache, &source, id)? else {
            return Ok(None);
        };
        record_thumbnail(conn, id, &path, size)?;
        Ok(Some(path))
    }

    /// PNG 首次生成 + 二次调用缓存命中（文件存在、不重新生成）。
    #[test]
    fn png_generate_then_cache_hit() {
        let dir = temp_dir("png");
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let src = dir.join("a.png");
        image::RgbaImage::from_pixel(640, 480, image::Rgba([10u8, 200, 30, 255]))
            .save(&src)
            .unwrap();

        {
            let conn = db::open(&dir.join("test.db")).unwrap();
            let id = insert(&conn, src.to_str().unwrap(), scanner::STATUS_OK);

            // 首次生成：文件存在、命名 {id}.webp、长边 ≤ THUMB_SIZE 且保持宽高比
            let first = get_thumb(&conn, &cache, id).unwrap().unwrap();
            assert!(first.is_file());
            assert_eq!(first.file_name().unwrap(), format!("{id}.webp").as_str());
            let img = image::open(&first).unwrap();
            assert_eq!((img.width(), img.height()), (256, 192));

            // 二次调用：缓存命中（返回同一路径，表记录数不变，不重新生成）
            let second = get_thumb(&conn, &cache, id).unwrap().unwrap();
            assert_eq!(second, first);
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM thumbnails", [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 1);
        }
        // 先关闭连接再清理临时目录：WAL 模式下连接持有文件句柄，
        // 未 drop 前 remove_dir_all 会因文件占用失败（与 library.rs 测试同因）。
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 不存在的 image_id → None。
    #[test]
    fn unknown_id_returns_none() {
        let dir = temp_dir("unknown");
        {
            let conn = db::open(&dir.join("test.db")).unwrap();
            assert!(
                get_thumb(&conn, &dir.join("cache"), 999)
                    .unwrap()
                    .is_none()
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// status='missing'（磁盘文件已删除）→ None，不生成。
    #[test]
    fn missing_image_returns_none() {
        let dir = temp_dir("missing");
        let src = dir.join("a.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([0u8, 0, 0, 255]))
            .save(&src)
            .unwrap();
        {
            let conn = db::open(&dir.join("test.db")).unwrap();
            let id = insert(&conn, src.to_str().unwrap(), scanner::STATUS_MISSING);
            assert!(
                get_thumb(&conn, &dir.join("cache"), id)
                    .unwrap()
                    .is_none()
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 损坏文件（假 .png 内容）→ None，且不写缓存库。
    #[test]
    fn corrupt_file_returns_none_without_db_write() {
        let dir = temp_dir("corrupt");
        let src = dir.join("broken.png");
        std::fs::write(&src, b"this is not a png").unwrap();
        {
            let conn = db::open(&dir.join("test.db")).unwrap();
            let id = insert(&conn, src.to_str().unwrap(), scanner::STATUS_OK);
            assert!(
                get_thumb(&conn, &dir.join("cache"), id)
                    .unwrap()
                    .is_none()
            );
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM thumbnails", [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// HEIC 解码冒烟：样例在 test/fixtures/sample.heic（下载失败时跳过）。
    /// 覆盖解码 + 完整生成管道两条路径。
    #[test]
    fn heic_decode_and_generate() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("test/fixtures/sample.heic");
        if !fixture.is_file() {
            eprintln!("[skip] 缺少 HEIC 样例文件 {fixture:?}，跳过 HEIC 测试");
            return;
        }

        // 解码路径：断言尺寸 > 0
        let img = decode_image(&fixture).unwrap();
        assert!(img.width() > 0 && img.height() > 0);

        // 完整管道：索引行 + 生成缩略图
        let dir = temp_dir("heic");
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        {
            let conn = db::open(&dir.join("test.db")).unwrap();
            let id = insert(&conn, fixture.to_str().unwrap(), scanner::STATUS_OK);
            let thumb = get_thumb(&conn, &cache, id).unwrap().unwrap();
            assert!(thumb.is_file());
            let t = image::open(&thumb).unwrap();
            assert!(t.width() <= THUMB_SIZE && t.height() <= THUMB_SIZE);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 补齐缺失缩略图：无缩略图的生成，已有缩略图的跳过。
    #[test]
    fn backfill_generates_missing_only() {
        let dir = temp_dir("backfill");
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let src1 = dir.join("a.png");
        let src2 = dir.join("b.png");
        image::RgbaImage::from_pixel(64, 64, image::Rgba([1u8, 2, 3, 255]))
            .save(&src1)
            .unwrap();
        image::RgbaImage::from_pixel(32, 32, image::Rgba([4u8, 5, 6, 255]))
            .save(&src2)
            .unwrap();
        {
            let conn = db::open(&dir.join("test.db")).unwrap();
            let id1 = insert(&conn, src1.to_str().unwrap(), scanner::STATUS_OK);
            let id2 = insert(&conn, src2.to_str().unwrap(), scanner::STATUS_OK);
            // 先给 id1 生成缩略图（模拟已有缓存）
            get_thumb(&conn, &cache, id1).unwrap().unwrap();
            // 补齐：只生成 id2
            let n = backfill_missing(&conn, &cache, 2).unwrap();
            assert_eq!(n, 1);
            // 两个都有缩略图了
            assert!(get_thumb(&conn, &cache, id1).unwrap().is_some());
            assert!(get_thumb(&conn, &cache, id2).unwrap().is_some());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}