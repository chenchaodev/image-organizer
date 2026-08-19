# 状态速查

> 每会话入口:新会话从这里开始,看「上次做到哪、接下来做什么」。保持 ≤2400 字符(超限先压缩/删除过期内容再新增),每次收尾更新顶部记录。docs 更新规则见各文档头部与全局配置目录 AGENTS.md。

## 记录(倒序,最新在上 = 当前状态)

- 2026-08-19:**M1 实测修复一轮**——启用 asset protocol(convertFileSrc 必需)+ get_image_detail 改异步(EXIF 读取不再阻塞主线程);cargo check/test 通过,待复测
- 2026-08-19:**迭代 2「M1 扫描浏览」开发完成**——缩略图生成/缓存(WebP 无损)+HEIC 解码(libheif-rs+vcpkg)+前端网格浏览/详情面板/扫描进度+vitest 测试段;Rust 19 测试+前端 13 测试通过,待人工实测(ACCEPTANCE 5 项)
- 2026-08-19:**迭代 2「M1 扫描浏览」规划完成**——调研落地(HEIC:libheif-rs+vcpkg 方案 ADR-03;虚拟滚动:TanStack Virtual),ACCEPTANCE 清单 5 项已建,待开发
- 2026-08-19:**v0.1.8 收尾**——CI(2m33s)与 release(7m27s)均通过,apt 动态查找修复验证完成;同步 M0 完成状态,进入迭代 2「M1 扫描浏览」规划
- 2026-08-19:发布 v0.1.8(2a71ba7;apt 动态查找修复后验证发布,CI 构建中;对比 v0.1.7:macOS 5m46s/Windows 7m42s/Ubuntu 12s 快速失败)

## 验证基线
- 已跑通命令/验收方式/打包发布见 `DEV-GUIDE.md`「验证基线」节,此处只留指针

## 打开事项
- [x] 确认 GitHub Actions check job 通过(2026-08-19:CI success 2m33s + release success 7m27s)
- [ ] M1 扫描浏览(迭代 2):开发完成待人工实测(ACCEPTANCE 5 项);遗留:HEIC DLL 打包分发、有损缩略图编码(记 ROADMAP 已知限制)
