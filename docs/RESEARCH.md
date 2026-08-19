# 研究结论

> 坑/库事实(含勿回退项细节)的单一事实源;勿回退标注位置在项目级 AGENTS.md。只记「换会话仍会用上、且别处查不到」的结论;调研/决策结论收到即落盘(见全局配置目录 AGENTS.md「落盘格式」节),分析原文存档于 `docs/archive/`。写入的命令/事实须实际验证过;时间戳一律带时分秒;同一主题就地更新旧条目并置顶,不新增;使用中读到已失效结论 → 删除该条目。条目格式见「格式示例」节,新条目置顶插入「条目」节。

## 格式示例(固定,勿删勿改)

### YYYY-MM-DD HH:mm:ss 主题
- 结论:……
- 理由:……
- 验证: <验证命令/复现方式>
- 来源: 子代理 / 主会话
- 关联: 相关文件 / commit / 版本号 / 原文存档文件名(docs/archive/,如有原文存档则必填)

## 条目

### 2026-08-19 19:43:20 HEIC 解码与虚拟滚动选型
- 结论:HEIC 用 libheif-rs 2.7.0 + libheif-sys 5.3.0(内嵌 libheif 1.23.0);Windows 必须 vcpkg 预装 libheif(不能纯 cargo 构建),`vcpkg install libheif[core,libde265]` 规避 GPL;macOS `brew install libheif`、Ubuntu `apt install libheif-dev`;HEIF 内嵌缩略图可直接解码(毫秒级);虚拟滚动用 @tanstack/react-virtual 3.14.9(React 19 官方适配);缩略图缓存 WebP q80,image thumbnail() 快速但锯齿,质量优先 resize(Triangle);并发 rayon + spawn_blocking,libheif 内部默认 4 线程注意超订
- 理由:libheif-rs 是 Rust 生态唯一成熟 HEIC 方案;image crate 无 HEIC(专利+项目政策);react-window 维护停滞、react-virtuoso 网格限等尺寸
- 验证: 待 M1 开发实际构建验证(Windows vcpkg 路径)
- 来源: 子代理(librarian)
- 关联: docs/archive/20260819-194320-HEIC与虚拟滚动调研.md

### 2026-08-18 21:46:00 国内安装 Rust 工具链镜像(实测)
- 结论:rustup-init.exe 走华科镜像 `https://mirrors.hust.edu.cn/rustup/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe` 可直下(8.6MB,PE 文件);安装时设 `RUSTUP_DIST_SERVER=https://mirrors.hust.edu.cn/rustup`、`RUSTUP_UPDATE_ROOT=https://mirrors.hust.edu.cn/rustup/rustup`
- 理由:中科大 `mirrors.ustc.edu.cn/rust-static` 对 exe 返回「Verifying your browser」JS 验证页(Invoke-WebRequest 拿不到真文件);rsproxy.cn/rustup-init.exe 404;阿里云镜像站未提供 rustup-init.exe 直链
- 验证: 2026-08-18 实测安装成功,rustc 1.97.1 + cargo 1.97.1 可用
- 来源: 主会话
- 关联: 全局配置目录 NETWORK-GUIDE.md(未收录项,建议补录)

### 2026-08-18 21:46:00 cargo 依赖源镜像
- 结论:`~/.cargo/config.toml` 配 `[source.crates-io] replace-with='ustc'` + `sparse+https://mirrors.ustc.edu.cn/crates.io-index/`,并设 `[net] git-fetch-with-cli = true`
- 理由:sparse 协议免 git 索引克隆,快且稳;git-fetch-with-cli 让 cargo 走系统 git(可复用 SSH 配置)
- 验证: 配置写入成功,待首次 cargo 构建实际拉包验证
- 来源: 主会话
- 关联: AGENTS.md「镜像/网络」硬约束
