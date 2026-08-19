//! Tauri 入口与 IPC 命令层（薄封装，业务逻辑在 engine/）。
//!
//! 为什么命令层保持薄：引擎纯逻辑可脱离 Tauri 独立测试，
//! 命令只做三件事——持有连接状态、调用引擎、把引擎错误归一化为
//! 用户可读文案（CODE-GUIDE：错误归一化在边界做）。

mod engine;

use engine::{db, images, library, scanner, thumbnail};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

/// 全局状态：SQLite 连接 + 数据库路径 + 扫描运行标记。
///
/// 为什么 Mutex<Option<Connection>> 而非 Mutex<Connection>：setup 建库
/// 失败时降级启动（命令返回可读错误），不让应用因初始化失败整体崩溃
/// （CODE-GUIDE 失败降级：能力失败 → 降级 + 警告留痕）。
///
/// 为什么 conn 再包一层 Arc：get_thumbnail_path 等异步命令需要把连接
/// 移入 spawn_blocking 闭包（'static 要求），Arc 克隆后闭包内重新 lock；
/// 同步命令经自动解引用照常使用，无需改动。
///
/// 为什么额外存 db_path：扫描线程需要独立连接（见 scan_library 注释），
/// 而 rusqlite 连接无法反查自身文件路径，故在 setup 时一并保存。
struct DbState {
    conn: Arc<Mutex<Option<Connection>>>,
    db_path: Option<PathBuf>,
    /// 扫描运行标记：防止用户重复触发并发扫描（写库互踩）。
    scanning: AtomicBool,
}

/// get_app_info 返回类型；camelCase 对齐前端命名习惯。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    name: String,
    version: String,
}

/// 扫描进度事件名（前端监听）。
const SCAN_PROGRESS_EVENT: &str = "scan-progress";

/// 扫描进度事件负载。
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanProgressPayload {
    phase: String,
    scanned: u32,
    total: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// 图片列表项。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageItem {
    id: i64,
    path: String,
    width: Option<i64>,
    height: Option<i64>,
    format: Option<String>,
    file_size: Option<i64>,
    mtime: Option<i64>,
}

impl From<images::ImageRow> for ImageItem {
    fn from(r: images::ImageRow) -> Self {
        Self {
            id: r.id,
            path: r.path,
            width: r.width,
            height: r.height,
            format: r.format,
            file_size: r.file_size,
            mtime: r.mtime,
        }
    }
}

/// get_images 返回体。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageList {
    items: Vec<ImageItem>,
    total: u32,
}

/// 图片详情：列表项 + EXIF 键值。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageDetail {
    id: i64,
    path: String,
    width: Option<i64>,
    height: Option<i64>,
    format: Option<String>,
    file_size: Option<i64>,
    mtime: Option<i64>,
    exif: HashMap<String, String>,
}

/// 应用名与版本（M0 用于前端展示，证明 IPC 链路通）。
#[tauri::command]
fn get_app_info(app: tauri::AppHandle) -> AppInfo {
    let info = app.package_info();
    AppInfo {
        name: info.name.to_string(),
        version: info.version.to_string(),
    }
}

/// 弹出目录选择对话框，返回选中路径；用户取消返回 None。
///
/// 为什么用回调 + channel 桥接而非阻塞等待：插件回调是异步的（本版本
/// pick_folder 仅提供回调形式），把结果经 tokio mpsc 转成可 await 的
/// future 交给命令层继续，避免阻塞 Tauri 事件循环。
#[tauri::command]
async fn select_library_dir(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, mut rx) =
        tauri::async_runtime::channel::<Option<tauri_plugin_dialog::FilePath>>(1);
    app.dialog().file().pick_folder(move |picked| {
        // 回调侧只发送、不等待；对话框结果只出现一次，容量 1 足够。
        let _ = tx.send(picked);
    });
    let picked = rx
        .recv()
        .await
        .ok_or_else(|| "目录选择失败：对话框通道已关闭".to_string())?;
    Ok(picked.map(|p| p.to_string()))
}

/// 保存图库路径（校验目录存在后写入 settings）。
#[tauri::command]
fn set_library(state: tauri::State<'_, DbState>, path: String) -> Result<String, String> {
    let guard = state.conn.lock().map_err(|_| "内部状态锁定失败".to_string())?;
    let conn = guard
        .as_ref()
        .ok_or_else(|| "数据库未初始化".to_string())?;
    library::set_library_path(conn, &path).map_err(|e| e.to_string())
}

/// 读取已保存的图库路径；从未设置过返回 null。
#[tauri::command]
fn get_library(state: tauri::State<'_, DbState>) -> Result<Option<String>, String> {
    let guard = state.conn.lock().map_err(|_| "内部状态锁定失败".to_string())?;
    let conn = guard
        .as_ref()
        .ok_or_else(|| "数据库未初始化".to_string())?;
    library::get_library_path(conn).map_err(|e| e.to_string())
}

/// 启动后台扫描线程；返回 started=false 表示已有扫描在运行。
///
/// 为什么扫描用独立连接而非主连接：扫描可能持续数分钟，若占用主连接
/// 的 Mutex，期间 get_images 等查询会被阻塞；WAL 模式下多连接并发
/// 读写互不阻塞，扫描线程自开连接即可。
#[tauri::command]
fn scan_library(state: tauri::State<'_, DbState>, app: tauri::AppHandle) -> Result<bool, String> {
    if state.scanning.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    let (db_path, library_path) = {
        let guard = state.conn.lock().map_err(|_| "内部状态锁定失败".to_string())?;
        let conn = guard
            .as_ref()
            .ok_or_else(|| "数据库未初始化".to_string())?;
        let path = library::get_library_path(conn)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "尚未设置图库路径".to_string())?;
        let db_path = state
            .db_path
            .clone()
            .ok_or_else(|| "数据库路径不可用".to_string())?;
        (db_path, path)
    };

    std::thread::spawn(move || {
        let result = run_scan(&app, &db_path, &library_path);
        // 无论成败都复位运行标记，允许下一次扫描。
        app.state::<DbState>().scanning.store(false, Ordering::SeqCst);
        if let Err(e) = result {
            let _ = app.emit(
                SCAN_PROGRESS_EVENT,
                ScanProgressPayload {
                    phase: "error".into(),
                    scanned: 0,
                    total: 0,
                    message: Some(e),
                },
            );
        }
    });

    Ok(true)
}

/// 在后台线程中执行扫描：打开独立连接 → 调用引擎扫描器 → 转发进度事件。
fn run_scan(app: &tauri::AppHandle, db_path: &Path, library_path: &str) -> Result<(), String> {
    let mut conn = db::open(db_path).map_err(|e| format!("打开数据库失败：{e}"))?;
    let root = Path::new(library_path);
    scanner::scan_library(&mut conn, root, |p| {
        let phase = match p.phase {
            scanner::ScanPhase::Scanning => "scanning",
            scanner::ScanPhase::Done => "done",
        };
        let _ = app.emit(
            SCAN_PROGRESS_EVENT,
            ScanProgressPayload {
                phase: phase.to_string(),
                scanned: p.scanned as u32,
                total: p.total as u32,
                message: p.message,
            },
        );
    })
    .map(|_| ())
}

/// 分页查询图片列表（按 id 升序，排除 missing）。
#[tauri::command]
fn get_images(
    state: tauri::State<'_, DbState>,
    offset: u32,
    limit: u32,
) -> Result<ImageList, String> {
    let guard = state.conn.lock().map_err(|_| "内部状态锁定失败".to_string())?;
    let conn = guard
        .as_ref()
        .ok_or_else(|| "数据库未初始化".to_string())?;
    let items = images::list_images(conn, offset, limit)?
        .into_iter()
        .map(ImageItem::from)
        .collect();
    let total = images::count_images(conn)?;
    Ok(ImageList { items, total })
}

/// 查询单张图片详情（索引行 + EXIF 键值）。
#[tauri::command]
fn get_image_detail(
    state: tauri::State<'_, DbState>,
    id: i64,
) -> Result<ImageDetail, String> {
    let guard = state.conn.lock().map_err(|_| "内部状态锁定失败".to_string())?;
    let conn = guard
        .as_ref()
        .ok_or_else(|| "数据库未初始化".to_string())?;
    let (row, exif) = images::get_image_detail(conn, id)?;
    Ok(ImageDetail {
        id: row.id,
        path: row.path,
        width: row.width,
        height: row.height,
        format: row.format,
        file_size: row.file_size,
        mtime: row.mtime,
        exif,
    })
}

/// 获取缩略图路径；无缓存时生成并落库，无索引/解码失败返回 None。
///
/// 为什么 spawn_blocking：HEIC 全尺寸解码 100-300ms，若在 async 命令里
/// 直接执行会阻塞 Tauri 异步运行时的事件循环（ADR-03）。
/// 为什么 cache_dir 在命令层解析：引擎层不感知 Tauri 路径 API（硬约束），
/// 应用数据目录经 app.path() 获取后拼 thumbnails 子目录。
#[tauri::command]
async fn get_thumbnail_path(
    state: tauri::State<'_, DbState>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<Option<String>, String> {
    let cache_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败：{e}"))?
        .join("thumbnails");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("创建缩略图目录失败：{e}"))?;
    // 为什么克隆 Arc 而非直接引用 state：spawn_blocking 要求闭包 'static，
    // 不能借用命令作用域内的 state；连接在闭包内重新 lock 获取。
    let conn = state.conn.clone();
    let path = tauri::async_runtime::spawn_blocking(move || {
        let guard = conn.lock().map_err(|_| "内部状态锁定失败".to_string())?;
        let conn = guard
            .as_ref()
            .ok_or_else(|| "数据库未初始化".to_string())?;
        thumbnail::get_or_create_thumbnail(conn, &cache_dir, id)
    })
    .await
    .map_err(|e| format!("缩略图生成线程异常：{e}"))??;
    Ok(path.map(|p| p.to_string_lossy().into_owned()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 数据库放 app_data_dir；为什么不放用户图库目录：非破坏式原则，
            // 索引库与用户图片目录物理隔离，绝不往图库写任何文件。
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("library.db");
            match db::open(&db_path) {
                Ok(conn) => {
                    app.manage(DbState {
                        conn: Arc::new(Mutex::new(Some(conn))),
                        db_path: Some(db_path),
                        scanning: AtomicBool::new(false),
                    });
                }
                Err(e) => {
                    // 降级启动：连接不可用但界面仍可打开，命令层返回可读错误。
                    eprintln!("[warn] 数据库初始化失败（{db_path:?}）: {e}");
                    app.manage(DbState {
                        conn: Arc::new(Mutex::new(None)),
                        db_path: Some(db_path),
                        scanning: AtomicBool::new(false),
                    });
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            select_library_dir,
            set_library,
            get_library,
            scan_library,
            get_images,
            get_image_detail,
            get_thumbnail_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}