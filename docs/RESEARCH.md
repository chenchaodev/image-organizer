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

### 2026-08-19 22:40:00 dev 构建解码慢 40 倍 + 缩略图并发需限流(实测)
- 结论:① `npm run tauri dev` 是 debug 构建,image/libheif 解码慢 ~40 倍(image-rs issue #1424),12MP JPEG 解码 10s+,缩略图/详情体验不可用——`[profile.dev.package."*"] opt-level = 2` 只优化依赖(解码热点在依赖内),自身代码保持 debug 快速迭代;② 缩略图解码无界并发会内存暴涨(12MP ≈48MB RGBA/张,40 并发 ≈2GB)且打满 CPU 饿死详情等 blocking 任务——需限流(4 并发 ≈200MB);③ Rust 1.97.1 无 std::sync::Semaphore(E0433),用 Mutex+Condvar 自实现 ~30 行;④ libheif-sys 的 vcpkg crate 默认找 x64-windows-static-md triplet,本机装的是 x64-windows——VCPKGRS_TRIPLET/VCPKGRS_DYNAMIC 固化到 `src-tauri/.cargo/config.toml` [env],依赖重编不再失败
- 理由:debug 未优化时解码热点在依赖内,opt-level 只开依赖即可;限流同时保护内存与调度公平
- 验证: 2026-08-19 实测——未开优化时大库缩略图 10s+/张、详情排队卡加载;修复后 cargo test 20 过,待 GUI 复测
- 来源: 主会话
- 关联: src-tauri/Cargo.toml、src-tauri/.cargo/config.toml、src-tauri/src/lib.rs

### 2026-08-19 22:10:00 WebView2 不支持 HEIC + 缩略图解码持锁串行(实测)
- 结论:① WebView2(Chromium)无法解码 HEIC,详情面板直接显示 HEIC 原图会失败,需改用缩略图(WebP);② 缩略图生成若在 DB 连接锁内解码(100-500ms/张),并发请求串行排队,大库首屏缩略图长时间不显示、详情查询排队卡死——解码必须移出锁,锁只覆盖毫秒级查询/写入;③ 扫描器跳过路径(mtime+size 未变)不恢复 missing→ok,换目录扫描后再扫原目录,列表(排除 missing)会一直为空
- 理由:WebView2 解码能力取决于 Chromium 内置 codec(无 HEIC);rusqlite 默认 busy_timeout=5000ms,扫描事务持写锁期间缩略图写入会等待后失败
- 验证: 2026-08-19 实测——大库 989 图仅 46 缩略图(串行生成);修复后 cargo test 20 过(含 missing 恢复回归测试),待 GUI 复测
- 来源: 主会话
- 关联: src-tauri/src/engine/thumbnail.rs、scanner.rs、src/components/DetailPanel.tsx

### 2026-08-19 21:10:00 Tauri 2 convertFileSrc 需启用 asset protocol(实测)
- 结论:前端用 convertFileSrc(path) 显示本地文件(缩略图/原图)时,tauri.conf.json 必须配置 `security.assetProtocol.enable=true` 且 scope 覆盖目标路径(本地桌面应用可先 `["**"]`),同时 Cargo.toml 的 tauri 依赖需加 `protocol-asset` feature;缺任一则 img 加载失败(现象:网格有格子无图)。另:同步命令在主线程执行文件 I/O(EXIF 读取)会阻塞 UI(现象:点击详情卡死),应改 async + spawn_blocking
- 理由:asset protocol 默认关闭,convertFileSrc 生成的 asset:// URL 无法加载;tauri-build 会校验 feature 与配置一致性(报错提示 add the protocol-asset feature)
- 验证: 2026-08-19 实测——未配置时缩略图已生成(文件在 app data)但界面无图;配置 enable+scope 并加 feature 后 cargo check/test 通过,待 GUI 复测
- 来源: 主会话
- 关联: src-tauri/tauri.conf.json、src-tauri/Cargo.toml、src-tauri/src/lib.rs

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
