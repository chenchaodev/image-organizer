# 架构决策

> ADR 风格:「为什么这么设计」。做决策/评审时记录;被推翻的旧条目首行标「已被 YYYY-MM-DD HH:mm:ss 主题(ADR-00N)取代」,保留不删;编号 ADR-00N 按条目新增顺序递增(01、02…),被取代标注引用原编号;时间戳一律带时分秒;同一主题就地更新旧条目并置顶,不新增;写入的命令/事实须实际验证过。条目格式见「格式示例」节,新条目置顶插入「条目」节。

## 格式示例(固定,勿删勿改)

### YYYY-MM-DD HH:mm:ss 主题(ADR-00N)
- 决策:……
- 理由:……
- 备选: <否决方案 + 否决原因>(无则省略)
- 验证: <构建/测试验证方式>
- 来源: 子代理 / 主会话
- 关联: 相关文档 / 文件 / 原文存档(docs/archive/,如有原文存档则必填)

## 条目

### 2026-08-18 21:45:00 技术栈选型(ADR-01)
- 决策:跨平台桌面应用采用 Tauri 2(Rust 1.97 后端 + React 19/TypeScript/Vite 7 前端),SQLite 存储;图片处理 image/img_hash/exif,数据库 rusqlite
- 理由:1万~10万张图片的扫描/哈希/缩略图/转换为 CPU+IO 密集任务,Rust 性能与并发优势明显;Tauri 2 体积小(~10MB)、内存占用低,跨 Win/Mac/Linux 一套代码;React 19 + TanStack Virtual 提供成熟虚拟滚动方案;SQLite 单文件 + WAL 满足本地索引与 FTS5 全文搜索需求
- 备选: Electron(内存占用高、体积 ~100MB+,10万张场景性能压力大);Flutter Desktop(原生能力需插件,文件系统/进程生态不如 Tauri 直接);PySide/Qt(打包分发繁琐,性能弱于 Rust);DuckDB(分析型,本地单机索引 SQLite 足够且生态更熟)
- 验证: Tauri 2 + React 19 + Vite 7 脚手架 `npm run build` 通过;Rust 1.97.1 工具链安装成功(cargo/rustc 可用)
- 来源: 主会话
- 关联: docs/archive/20260818-214500-技术栈选型.md

### 2026-08-18 21:45:00 数据模型(ADR-02)
- 决策:核心表为 folders/images/thumbnails/dedup_groups/dedup_members/settings;images.path 唯一,含 file_size/mtime/width/height/format/phash;去重按组-成员两表(相似度可扩展)
- 理由:非破坏式产品形态下,索引库必须与磁盘解耦——path 唯一键承载文件身份,file_size+mtime 支撑增量扫描跳过;缩略图独立成表便于分级缓存与清理;去重组-成员结构支持多图成组(>2 张重复)与相似度排序;settings 表为后续设置项留扩展
- 备选: 单表冗余存缩略图路径(更新/清理不灵活);去重只存成对关系(多图组查询复杂)
- 验证: 待 M0 迭代 SQLite 建表落地后验证
- 来源: 主会话
- 关联: docs/ROADMAP.md
