# Image Organizer

非破坏式本地图片管理器：扫描你的现有目录建立索引，图片原地不动；提供高速浏览、重复图片检测与清理，后续扩展批处理工具。面向个人 1万~10万张图片。

## 功能特性

- **本地图片库整理归类**：扫描任意目录建立索引，网格浏览、排序与筛选，重启后秒级增量更新
- **重复图片检测**：感知哈希识别重复与相似图片，分组展示，一键清理到回收站
- **批处理工具（规划中）**：批量重命名（模板规则）、格式转换、压缩

## 安装与使用

环境要求：Node.js ≥ 20、Rust ≥ 1.77（Windows 需 MSVC Build Tools）

```bash
npm install            # 安装依赖
npm run tauri dev      # 开发运行
npm run tauri build    # 打包发布
```

快速上手：启动应用 → 选择图片库目录 → 等待索引完成后浏览图库。

## 技术栈

- Tauri 2（桌面框架：Rust 后端 + Web 前端）
- React 19 + TypeScript + Vite 7（前端界面）
- SQLite（本地图片索引存储）

## 开发

```bash
npm run build        # 类型检查 + 前端构建
cargo check          # Rust 静态检查
npm run tauri dev    # 开发运行
```

测试体系：test/ 按内容主题零注册分层（细则见 docs/DEV-GUIDE.md）。

## 文档

- [用户手册](docs/USER-GUIDE.md)：安装、操作、设置项、FAQ
- [开发者手册](docs/DEV-GUIDE.md)：环境、命令、代码地图、验证基线
- [变更日志](docs/CHANGELOG.md)：版本演进历史
- [路线图](docs/ROADMAP.md)：需求范围、规划、里程碑
- [验收记录](docs/ACCEPTANCE.md)：验收清单与实测结果
- [状态速查](docs/STATUS.md)：当前状态与打开事项
- [研究结论](docs/RESEARCH.md) / [架构决策](docs/ADR.md)：技术事实与决策记录

## 许可证

MIT：自由使用与修改。
