# 更新日志

[English](CHANGELOG_EN.md) | 中文

---

## v5.4.0 (2026-11-24)

### 界面
- 顶部工具栏加双栏卡片布局, 深浅主题配色改为深蓝, 砖红, 铜棕
- 会话来源改为分段切换并显示本机 App 和 CLI 版本; 会话列表在标题下方显示来源, 时间和项目; 支持隐藏仅索引会话 (按完整 ID 搜索时仍显示)
- 会话详情支持在文件管理器中显示项目目录和会话文件

### 会话内容
- 助手回复按 Markdown 渲染 (标题, 列表, 任务列表, 表格, 代码块); 原始 HTML 显示为文本, 不加载远程图片
- 本地文件链接显示为标签, 点击可在文件管理器中定位; 网页链接用系统浏览器打开
- 用户消息改为右侧气泡, 连续回复合并显示; 长消息默认折叠, 离开底部时可跳到最新消息

### 会话清理
- 删除完整会话后一并清理其他来源中同 ID 的仅索引记录, 并在确认后清理各本地来源均无会话文件, 附件和迁移归档的历史残留索引
- Usage Monitor 完整迁移会话后拒绝删除源端残留, 以保留迁入会话可能引用的附件; 迁移失败的归档不影响删除, 自定义 `--data-dir` 不在检查范围内

---

## v5.3.0 (2026-11-22)

### 用量插件
- 标题栏弹窗改为从 GitHub 下载安装或卸载上游 [Usage Monitor](https://github.com/yuhuan-dev/antigravity-usage-monitor) 最新正式版; 在 Antigravity 的 **Customizations → Installed** 中启用后可查看缓存命中率及账号的 5 小时和每周额度
- 安装时为官方插件添加两个 Anti-Power 按钮: 面板顶栏打开当前本地会话, 设置页打开 **Usage Monitor** 弹窗; 插件通过内置 ZIP 更新时会重新应用按钮补丁
- 卸载保留 Antigravity 数据目录中的设置和用量历史; 若安装目录含 Git 仓库或插件本地数据, 会备份整个目录并显示保存位置
- 从 v5.1–5.2 切换时, 可先用旧版卸载 Anti-Power Usage 以还原 CLI 状态栏; 原账本保留在本机, Usage Monitor 不会导入

### 会话清理
- 新增默认关闭的 **Usage Monitor 会话记录** 选项: 清除已无本地副本会话的缓存标题与提问并保留用量和费用; 已安装的插件具备匹配的独立用量记录时移除原对话条目

---

## v5.2.1 (2026-11-22)

### Anti-Power Usage 用量插件
- 修复 Antigravity App 启用顶栏挂载后不显示用量指标的问题: 窗口右上角显示当前会话的输出速度, 首字延迟和上下文占比, 随会话切换更新, 收起侧栏后仍可见
- CLI 状态栏分别保留当前会话最近一次实测的速度和首字延迟, 暂无测速时显示活动状态; 新会话建库完成前跳过读取以避免争锁

---

## v5.2.0 (2026-11-22)

### Anti-Power Usage 用量插件
- 修复 Antigravity App 侧栏打开显示空白的问题; 窄侧栏中会话和请求明细按字段分行显示, 图表随宽度自适应
- 当前会话指标栏显示最近一次模型请求的速度, 首字延迟和上下文占比, 展开可查看已用 token, 窗口上限和分类构成 (会话压缩后按保留内容和摘要计算)
- Antigravity CLI 状态栏居中显示这三项指标, 两侧保留快捷键提示与模式, 模型, 推理档位; macOS 和 Linux 安装时自动为已有 CLI 配置 (已有其他配置或已关闭时需勾选替换, 卸载时自动还原)

### 客户端版本与启动
- 会话来源选择器显示本机检测到的 Antigravity App 和 Antigravity CLI 版本
- macOS 用量插件弹窗新增 **启动 Antigravity 并启用挂载**: 退出 App 后以本机随机调试端口启动, 供桌面顶栏挂载使用 (侧栏和 CLI 状态栏无需此项)

---

## v5.1.0 (2026-11-22)

### 用量插件
- 随附 **Anti-Power Usage**: 从标题栏弹窗安装, 升级, 卸载或更新价格表, 在 Antigravity App 的 **Customizations → Installed** 中启用; 卸载保留已记录用量
- 后台自动采集本地请求, 可在侧边栏按日期, 模型, 来源或会话查看 token 用量, 缓存命中率和估算费用, 并展开查看每轮输入, 回复, 思考, 工具调用, 首字延迟和输出速度 (对话正文按需从本机库读取)
- 同一会话的多个本地副本合并计量; 删除最后一份副本后, 已记录用量和费用并入匿名的 "已删除会话" (外部移走文件在确认缺失满 60 秒后归并)

### 会话清理
- 勾选 **CC Switch 同步记录** 时一并清理本地存储中已消失会话在 CC Switch 主库和备份中的残留同步进度 (保留用量和费用历史); 支持在 CC Switch 运行时清理, 退出后刷新即可完成复查
- 删除会话后若窗口状态, 用量汇总, 缓存或同步记录未清理完, 重新打开 Anti-Power 或刷新列表会继续处理

### 许可证
- 自 v5.1.0 起采用 GPL-3.0-only; v5.0.1 及更早版本的 MIT 声明保留在 `LICENSE-MIT`, 插件上游来源见 `plugin/NOTICE.md`

---

## v5.0.1 (2026-11-20)

### 新功能
- 删除会话时一并清理插件 `sidecar_data/` 下的事件, Usage Monitor 缓存条目和相关日志行, 与会话文件在同一可恢复清理中提交
- 同步清理 Antigravity App `app_storage.json` 中的窗口布局和侧栏状态
- 新增两个默认关闭的可选项, 勾选状态会被记住:
  - **CC Switch 同步记录**: 删除 `~/.cc-switch/` 下的 `cc-switch.db` 及 `backups/*.db` 中的会话文件同步进度, 保留用量和费用历史 (需先退出 CC Switch)
  - **Antigravity App 浏览器缓存**: 清空 `Cache/` 和 `Code Cache/`, 其中可能留有会话 ID, 会自动重建
- 应用数据目录和 CC Switch 的清理若失败, 会话不会回滚, 结果中会列出未清理项

---

## v5.0.0 (2026-11-19)

### 新功能
- 项目转为 Antigravity App 和 Antigravity CLI (`agy`) 的本地会话管理工具, 支持查看, 搜索, 单条和批量删除
- 按来源和会话 ID 清理会话正文, 日志, 注解, 占用锁, 摘要数据库, 全文索引 (删除后重建 FTS5, 压缩数据库并执行 WAL 检查点), protobuf 摘要索引, 最近会话, 摘要缓存和 Antigravity CLI 输入历史, 清理后 CC Switch 不再列出
- 支持清理中断后的自动恢复: 重新打开应用或刷新列表时, 已提交的清理继续收尾, 未提交的清理自动还原
- 列出并可清理只剩索引的残留记录, 包括只存在于 protobuf 摘要索引里的记录
- 批量删除时每个存储目录只处理一遍, 单条失败不影响同批其他会话
- 识别 CC Switch 的 Antigravity CLI 配置目录

### 移除
- 侧边栏和 Manager 补丁 (Markdown 复制, LaTeX, Mermaid, 表格配色), 补丁安装器和脚本. Google Antigravity 已内置相关能力, 需要补丁请使用 v4.0.0
- Claude Code, Codex, Gemini CLI, OpenCode, OpenClaw 的对话浏览和缓存清理

### 依赖
- 升级到 Tauri 2.12, Vite 8, TypeScript 6, vue-i18n 11, rusqlite 0.43, sysinfo 0.40
- 移除 Tailwind CSS, 改用轻量基础样式重置
- 需要 Node.js 22.12+ 和 Rust 1.95+

### 文档
- README / Changelog / Release Notes 同步至 v5.0.0, 已核对版本更新至 Antigravity App 2.33.0 与 Antigravity CLI 1.5.12 (macOS)

---

## v4.0.0 (2026-05-23)

### 新功能
- 识别新版 `Antigravity IDE` 的安装路径和数据目录命名，覆盖 Windows、macOS、Linux 及安装和清理脚本
- 支持版本更新至 `Antigravity IDE v2.0.6`

### 优化
- 优化 Sidebar/Manager 的消息内容识别, 兼容新版 DOM 结构并减少对输入框, 按钮等控制区的误绑定
- 复制包含 KaTeX/MathJax 公式的选区时自动还原 LaTeX 源码, 旧版侧边栏, 新版侧边栏和 Manager 一致生效
- 清理工具新增对 `.gemini/antigravity-ide` 对话缓存目录的支持

### 修复
- 修复新版 IDE 缺少旧版 `extensions/antigravity` 目录时安装校验过严的问题

### 文档
- README/Changelog/Release Notes 同步至 v4.0.0
- README 支持的 Antigravity IDE 版本更新至 v2.0.6

---

## v3.3.2 (2026-03-13)

### 修复
- 修复最新 Antigravity + 最新插件激活后 Manager 显示异常 (图标文字化, 布局异常), 同步 `workbench-jetski-agent.html` 入口模板的 CSP/Trusted Types 与上游保持一致
- 同步新版侧边栏 `workbench.html` 入口模板 CSP 白名单, 避免同类字体/图标资源策略变更导致的异常

### 文档
- README/Changelog/Release Notes 同步至 v3.3.2

---

## v3.3.1 (2026-03-10)

### 修复
- 修复侧边栏, Manager 与旧版侧边栏中代码块字号的回归问题; 当面板自定义字号小于编辑器字号时, 代码块现在会优先跟随面板字号, 不再被更大的编辑器字号锁定

### 文档
- README/Changelog/Release Notes 同步至 v3.3.1
- README 支持的 Antigravity 版本更新至 v1.20.5

---

## v3.3.0 (2026-03-09)

### 新功能
- 安装器新增对话浏览器, 支持查看与删除 Claude Code, Codex, Gemini CLI, OpenCode, OpenClaw 的本地对话记录
- 支持选择多条对话并批量删除

### 优化
- 清理工具新增对 OpenCode 和 OpenClaw 的支持
- 侧边栏与 Manager 的代码块字号现在会跟随面板字号与编辑器字号, 提升不同字号配置下的代码可读性
- 补充高亮代码节点样式覆盖, 统一旧版侧边栏, 新版侧边栏与 Manager 的代码显示效果

### 文档
- README/Changelog/Release Notes 同步至 v3.3.0
- README 支持的 Antigravity 版本更新至 v1.20.4

---

## v3.2.3 (2026-03-02)

### 优化
- Windows 清理工具新增 `sqlite3` 可执行文件自动解析, 除 PATH 外支持 Chocolatey/Git for Windows/Scoop/WinGet 常见安装路径
- 启动时优先对已保存的 Antigravity 路径做规范化与状态刷新, 减少路径已配置但状态未回填的问题

### 修复
- 自动检测路径失败时, 保留用户已保存路径, 避免启动后路径输入被清空

### 文档
- README/Changelog/Release Notes 同步至 v3.2.3

---

## v3.2.2 (2026-02-27)

### 优化
- 复制提取新增内联 Markdown 语义保留, 支持加粗/斜体/删除线/链接/行内代码
- 新增段落/块引用/水平分割线/显式换行转换规则, 提升复杂内容复制可读性
- 当内容包含内联格式时禁用原始文本缓存直出, 避免格式丢失
- 空行策略改为最多合并为一个空行, 保留段落与标题结构

### 文档
- README/Changelog/Release Notes 同步至 v3.2.2
- README 支持的 Antigravity 版本更新至 v1.19.6

---

## v3.2.1 (2026-02-26)

### 优化
- LaTeX 渲染支持流式增量更新重渲染, 避免内容变化后无法刷新
- 修复 `$...$` 公式内 `_`/`__` 被 Markdown 强调标签拆分导致的下划线丢失
- Mermaid/KaTeX 资源加载失败后不再缓存失败状态, 后续扫描可自动重试
- 复制反馈定时器去重, 减少连续点击时按钮状态闪烁
- 移除 Sidebar/Manager Mermaid 调试日志, 降低控制台噪音

### 修复
- 修复 Sidebar Mermaid Trusted Types policy 名称, 与 `sidebarPanel` CSP 白名单对齐
- 修复 Unix 临时脚本/临时目录命名冲突风险, 提高并发与重复安装稳定性
- 卸载恢复后自动清理 `.bak` 备份文件, 避免残留
- 配置路径获取新增 `~/.config` 回退, 提升 Linux 环境兼容性

### 文档
- README/Changelog/Release Notes 同步至 v3.2.1

---

## v3.2.0 (2026-02-24)

### 新功能
- 适配 Antigravity 新版侧边栏入口, 根据 `product.json` 中 `ideVersion` 自动切换旧版/新版补丁路径

### 优化
- 安装器界面新增 Antigravity 版本号显示
- 确认安装弹窗根据侧边栏入口模式动态展示变更文件
- 新版侧边栏样式与 Manager 样式对齐, 修复复制按钮显示细节
- 修复补丁状态回填逻辑, 避免仅安装单模块时误判另一模块为启用

### 文档
- README/Changelog/Release Notes 同步至 v3.2.0
- README 支持的 Antigravity 版本更新至 v1.18.4

---

## v3.1.0 (2026-02-06)

### 新功能
- 清理工具新增 Windows 平台支持, 可清理 Antigravity, Gemini CLI, Codex 与 Claude Code 的对话缓存

### 优化
- 后端命令错误处理支持多语言, 根据当前界面语言返回错误信息
- 前端向后端命令透传 locale, 统一跨平台错误提示体验

### 文档
- README/Changelog/Release Notes 同步至 v3.1.0

---

## v3.0.1 (2026-02-05)

### 新功能
- 清理工具新增支持 Gemini CLI, Codex 和 Claude Code, 默认仅选 Antigravity
- 清理工具新增启用开关, 防止误触
- 新增英文版脚本 `anti-power.en.sh`/`anti-clean.en.sh`

### 修复
- 修复英文模式下脚本输出为中文的问题

### 文档
- 手动安装说明补充英文脚本
- README/Changelog/Release Notes 同步至 v3.0.1

---

## v3.0.0 (2026-02-04)

### 新功能
- 中英文界面切换
- 深浅色主题切换
- 侧边栏与 Manager 功能卡片支持折叠/展开

### 优化
- 安装器布局重构为双栏响应式，清理工具根据屏幕自动调整位置
- 全面升级视觉样式（字体、色板、按钮、卡片、模态与动画）

### 修复
- 修复清理工具（macOS/Linux）运行逻辑，确保执行前经过二次确认

### 文档
- README/Changelog/Release Notes 同步至 v3.0.0
- README 更新支持的 Antigravity 版本至 v1.16.5，并统一 macOS 应用名为 `Anti-Power.app`

---

## v2.4.0 (2026-02-02)

### 新功能
- 支持 macOS 通用构建 (Universal Build)
- 新增 `icon-gen` 图标生成工具脚本
- 新增 `npm run clean` 清理命令
- 依赖库升级

### 文档
- 同步跨平台安装说明
- 修正文档遗留问题

---

## v2.3.4 (2026-02-01)

> 社区贡献: 感谢 [@mikessslxxx](https://github.com/mikessslxxx) 的 PR #18

### 新功能
- macOS/Linux 跨平台支持
- 跨平台路径规范化与检测
- Unix 系统提权安装流程 (sudo/pkexec)
- GitHub Actions 自动化构建与发布

### 优化
- 路径检测新增 Linux 支持
- 前端适配 macOS Resources 路径
- 调整窗口装饰以适配 macOS 系统标题栏

---

## v2.3.3 (2026-01-31)

### 修复
- 修复数学公式渲染失效的严重 BUG
- 临时回滚公式渲染机制至 v2.0.1 稳定版本逻辑
- 新增公式渲染模式切换功能 (经典模式/延迟模式)

---

## v2.3.2 (2026-01-31)

> 社区贡献: 感谢 [@mikessslxxx](https://github.com/mikessslxxx)

### 优化
- 数学公式渲染优化
- 复制按钮样式优化

### 新功能
- 复制按钮提供更多自定义选项 (样式, 位置等)

### 修复
- 修复 Manager 补丁导致 "安装似乎损坏" 提示的问题

---

## v2.3.1 (2026-01-31)

> 社区贡献: 感谢 [@Sophomoresty](https://github.com/Sophomoresty) 的 PR

### 新功能
- 新增右下角常驻复制按钮
- 开发者文档 (中英文双语)

### 优化
- KaTeX CSS 和 JS 并行加载, 加快首次公式渲染速度
- 右上角悬停按钮上移, 避免遮挡文字内容
- 代码块使用等宽字体栈 (Cascadia Code, Fira Code 等)
- 清理复制内容中的多余空行

### 修复
- 嵌套列表中的代码块及语言标识符正确识别
- 内联代码 (`pre.inline`) 正确提取为反引号格式
- 过滤 SVG 标签, 避免复制内容包含图标代码

### 变更
- 移除 good/bad 反馈区域的复制按钮 (减少界面干扰)
- Manager 功能 (mermaid, math) 默认启用

---

## v2.3.0 (2026-01-30)

### 新功能
- 复制功能支持 Markdown 格式保留 (标题/列表/加粗/斜体/链接等)

---

## v2.2.0 (2026-01-21)

> 社区贡献: 感谢 [@mikessslxxx](https://github.com/mikessslxxx)

### 新功能
- Manager 窗口 Mermaid 渲染
- Manager 窗口数学公式渲染
- Manager 窗口对话宽度调节
- Manager 窗口字号调节

---

## v2.1.0 (2026-01-19)

### 新功能
- 侧边栏字体大小调节
- Manager 窗口一键复制

### 优化
- Mermaid 渲染报错提示优化

---

## v2.0.1 (2026-01-14)

### 优化
- 性能优化

---

## v2.0.0 (2026-01-14)

### 新功能
- 全新 Tauri 安装工具
- 支持功能单独开关

---

## v1.2.1 (2026-01-13)

### 修复
- Bug 修复

---

## v1.2.0 (2026-01-13)

### 新功能
- Mermaid 流程图渲染

---

## v1.1.0 (2026-01-13)

> 社区贡献: 感谢 [@mikessslxxx](https://github.com/mikessslxxx)

### 新功能
- 数学公式渲染 (KaTeX)

---

## v1.0.0 (2026-01-13)

### 初始版本
- 一键复制按钮
- 表格颜色修复 (深色主题)
