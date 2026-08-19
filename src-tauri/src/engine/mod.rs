//! 引擎层：与 Tauri 无关的纯逻辑，可脱离 UI 单元测试。
//!
//! 依赖方向单向：commands（lib.rs）→ engine；engine 内部
//! db 为通用存储原语，library 承载「图库」业务语义并依赖 db，
//! scanner（增量扫描）/metadata（元数据探测）/images（图片查询）
//! 依赖 db 与 library，互不反向依赖命令层。

pub mod db;
pub mod images;
pub mod library;
pub mod metadata;
pub mod scanner;
