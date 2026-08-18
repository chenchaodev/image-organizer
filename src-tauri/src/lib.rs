//! Tauri 入口与 IPC 命令层（薄封装，业务逻辑在 engine/）。
//!
//! 为什么命令层保持薄：引擎纯逻辑可脱离 Tauri 独立测试，
//! 命令只做三件事——持有连接状态、调用引擎、把引擎错误归一化为
//! 用户可读文案（CODE-GUIDE：错误归一化在边界做）。

mod engine;

use engine::db;
use engine::library;
use rusqlite::Connection;
use std::sync::Mutex;
use tauri::Manager;

/// 全局状态：SQLite 连接。
///
/// 为什么 Mutex<Option<Connection>> 而非 Mutex<Connection>：setup 建库
/// 失败时降级启动（命令返回可读错误），不让应用因初始化失败整体崩溃
/// （CODE-GUIDE 失败降级：能力失败 → 降级 + 警告留痕）。
struct DbState(Mutex<Option<Connection>>);

/// get_app_info 返回类型；camelCase 对齐前端命名习惯。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    name: String,
    version: String,
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
    let guard = state.0.lock().map_err(|_| "内部状态锁定失败".to_string())?;
    let conn = guard
        .as_ref()
        .ok_or_else(|| "数据库未初始化".to_string())?;
    library::set_library_path(conn, &path).map_err(|e| e.to_string())
}

/// 读取已保存的图库路径；从未设置过返回 null。
#[tauri::command]
fn get_library(state: tauri::State<'_, DbState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().map_err(|_| "内部状态锁定失败".to_string())?;
    let conn = guard
        .as_ref()
        .ok_or_else(|| "数据库未初始化".to_string())?;
    library::get_library_path(conn).map_err(|e| e.to_string())
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
                    app.manage(DbState(Mutex::new(Some(conn))));
                }
                Err(e) => {
                    // 降级启动：连接不可用但界面仍可打开，命令层返回可读错误。
                    eprintln!("[warn] 数据库初始化失败（{db_path:?}）: {e}");
                    app.manage(DbState(Mutex::new(None)));
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            select_library_dir,
            set_library,
            get_library
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
