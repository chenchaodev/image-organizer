# Image Organizer 项目约束

> 项目级约束:只写**硬约束(勿回退)**与**规则**,≤2500 字符;细节只留指针到 docs/;不写瞬态信息(git 哈希/产物体积/完成历史,查 git log);有变化时随对应提交更新并递增版本号,超限先瘦身再新增。全局工作流见全局配置目录 AGENTS.md。

## 硬约束(勿回退)
- 技术栈:Tauri 2 桌面应用(Rust 1.97 后端 + React 19/TypeScript/Vite 7 前端 + SQLite 存储);选型结论见 docs/ADR.md
- 镜像/网络:npm 走 npmmirror(项目 .npmrc)、cargo 走中科大 sparse 镜像(~/.cargo/config.toml)、rustup 走华科镜像;install 失败先怀疑网络(细则见全局配置目录 NETWORK-GUIDE.md)
- 核心依赖选型:tauri@2、tauri-plugin-opener@2、react@19、vite@7;图片处理 image/img_hash/exif、数据库 rusqlite(SQLite);实际验证事实见 docs/RESEARCH.md
- 架构方向:核心逻辑(Rust 引擎)与界面(React)分离;UI → IPC → 引擎 → SQLite 单向依赖;引擎内扫描/索引/缩略图/去重模块解耦
- 产品形态:非破坏式图片库(扫描现有目录建索引,图片原地不动);整理动作经软件执行并写回磁盘

## 规则
- 提交策略:一次提交 = 一个可独立回退的逻辑单元;message 用 prefix 风格(feat/fix/docs/chore/refactor)
- 提交前:过 `npm run build`(tsc + vite)与 `cargo check`;git status 只含本逻辑单元文件
- 版本号三统一:package.json(版本源)/git tag/CHANGELOG 同号同源,细则见全局配置目录 PUBLISH-GUIDE.md
- 测试体系:test/ 按内容主题零注册分层;新增能力须补对应测试段(细则见 docs/DEV-GUIDE.md)
- 本仓库特有规则:Rust 工具链未加系统 PATH,用 `%USERPROFILE%\.cargo\bin\` 全路径调用或临时加 PATH;pwsh 环境;图片库路径含中文/空格时注意引号
