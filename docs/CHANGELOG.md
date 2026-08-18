# CHANGELOG

> 变更历史:随发版更新——平时提交不写本文件(流水查 git log),「迭代完成」打版本号时从 git log 汇总本版本条目(见全局配置目录 AGENTS.md「提交时」节)。版本号三统一细则见全局配置目录 `PUBLISH-GUIDE.md`(单一事实源);条目号与 tag 同号,如 tag v1.0.0 → [1.0.0];历史遗留「迭代序列与发布号解耦」为例外,须显式声明对应关系。写入的命令/事实须实际验证过。

## [0.1.2] - 2026-08-18
- 修复:release body 脚本在 Windows runner 解析失败(默认 pwsh 不兼容 bash 语法),显式指定 shell: bash;0.1.1 发布失败作废
- 验证:前端 build、cargo check/test;发布构建由 CI release job 验证
- 待实测:发布资产三平台可运行

## [0.1.1] - 2026-08-18
- 修复:tag 触发发布时 release job 被连带跳过(needs: check + check 限定 branch 触发),去除依赖后三平台构建恢复;0.1.0 发布失败作废
- 验证:前端 build、cargo check/test;发布构建由 CI release job 验证
- 待实测:发布资产三平台可运行

## [0.1.0] - 2026-08-18
- 初始化工程骨架:Tauri 2 + React 19 + Vite 7 + TS 脚手架、docs 知识库体系、国内镜像配置(npm/cargo/rustup)
- M0 骨架:图库目录选择 + SQLite 初始化(ADR-02 六表,WAL)+ 4 个 IPC 命令 + 前端版本回显与路径持久化
- CI:GitHub Actions 工作流——push 构建验证(check)+ tag v* 三平台自动发布(release,Release 说明自动汇总提交)
- 验证:前端 build、cargo check、cargo test 4/4、人工实测 4/4 通过
- 待实测:无
