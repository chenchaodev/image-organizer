# 开发者手册

> 上手最快路径:环境/命令/代码地图/架构/验证基线。新会话或换人先看这里;环境/命令/代码结构/验证基线变化时更新本文件。

## 环境
- Node.js 24 + npm 11(镜像:npm 走项目 .npmrc npmmirror)
- Rust 1.97.1(rustup 管理;已加入用户 PATH `%USERPROFILE%\.cargo\bin`;镜像:cargo 中科大 sparse、rustup 华科,详见 AGENTS.md)
- Windows:MSVC Build Tools(VS 2022,VC Tools 工作负载)编译 Rust 必需

## 命令
| 命令 | 用途 |
| ---- | ---- |
| `npm install` | 安装前端依赖(npmrc 镜像) |
| `npm run build` | 类型检查(tsc)+ 前端构建(vite) |
| `npm run tauri dev` | 开发运行(需 cargo 在 PATH 或临时加) |
| `cargo check` | Rust 静态检查(在 src-tauri/ 下) |
| `cargo test` | Rust 单元测试 |
| `npm run tauri build` | 打包发布 |

## 架构(设计决策,勿随意偏离)
- 分层:React 界面(UI)→ Tauri IPC(commands/events)→ Rust 引擎 → SQLite;单向依赖
- 引擎内模块:扫描器/索引库/缩略图服务/去重引擎/批处理管道,模块间解耦
- 通用原则:核心逻辑与界面/入口分离(便于测试与复用),界面层只做编排
- 设计决策详情见 `docs/ADR.md`

## 代码地图
- `src/`:React 前端(网格浏览/去重视图/批处理向导,后续迭代展开)
- `src-tauri/src/`:Rust 后端(main.rs 入口 + lib.rs 引擎与 IPC)
- `docs/`:项目知识库(见 README.md 登记表)

## 测试体系(按内容主题零注册,新增=新建主题文件)
- 目录分层:`test/<层>/`(unit/ 单元层 + integration/ 集成层),按内容主题命名,零注册自动发现
- 静态样例入 `test/fixtures/`(可版本化);产物按用途分目录(如 output/artifacts + output/smoke),可清理重建、自清理
- 断言写可验证事实(解包/产物字符串/读回),不写无断言日志
- 验收样例生成器(可选):fixtures 由测试段导出 + 漂移校验(幂等,exit 0/1),GUI 人工实测直接拖入
- 新增能力须补对应测试段;缺口清单见 ROADMAP

## 验证基线(STATUS.md 只留指针)
- 已跑通:`npm run build`(tsc + vite,脚手架默认页)、`npm install`(npmmirror)、`cargo check`(Rust 1.97.1 + MSVC 14.44 + WinSDK 10.0.26100,自动探测 MSVC 实例,1m33s)
- 历史:初始化期 installer 未写完注册表时曾用显式 linker + 手动 LIB 绕行,2026-08-18 注册表就绪后已移除(记录于 git 历史)
- 验收方式:ACCEPTANCE.md 待实测清单(GUI 人工项);自动断言测试段(后续迭代建立)
- 打包/发布:打包命令 `npm run tauri build`;已知限制:Windows 需 MSVC(自动探测已就绪)
- 构建/打包/工具链类改动必须实际构建并重跑本基线
- 人工验收要点:docs/ACCEPTANCE.md 待实测清单
