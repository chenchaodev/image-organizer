//! 引擎层：与 Tauri 无关的纯逻辑，可脱离 UI 单元测试。
//!
//! 依赖方向单向：commands（lib.rs）→ engine；engine 内部
//! db 为通用存储原语，library 承载「图库」业务语义并依赖 db。

pub mod db;
pub mod library;
