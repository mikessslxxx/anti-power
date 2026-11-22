<p align="center">
  <img src="docs/assets/images/LOGO.png" alt="Anti-Power" width="120">
</p>

<h1 align="center">Anti-Power</h1>

<p align="center">
  <a href="https://github.com/mikessslxxx/anti-power/releases"><img src="https://img.shields.io/badge/版本-v5.2.1-blue.svg" alt="版本"></a>
  <img src="https://img.shields.io/badge/Antigravity_App-2.34.0-green.svg" alt="Antigravity App">
  <img src="https://img.shields.io/badge/Antigravity_CLI-1.5.14-green.svg" alt="Antigravity CLI">
</p>

<p align="center">
  中文 | <a href="README_EN.md">English</a>
</p>

> 查看并清理 **Antigravity App** 和 **Antigravity CLI** (`agy`) 的本地会话, 并附带一个记录每次请求用量的 Antigravity 插件.

## 功能

- 列出 Antigravity App (`~/.gemini/antigravity`) 和 Antigravity CLI (`~/.gemini/antigravity-cli`) 的会话, 也识别 CC Switch 的 `~/.cc-switch/agy/<profile>/antigravity-cli`
- 按标题, 项目或会话 ID 搜索, 按来源 (显示检测到的客户端版本) 筛选并查看消息
- 单条或批量删除, 逐条显示结果
- 标出只剩索引的残留记录 (其他工具删掉会话文件后留下的), 可照常删除

已在 macOS 上用 Antigravity App 2.34.0 和 Antigravity CLI 1.5.14 核对. Windows 和 Linux 安装包尚未实测. 不支持 Antigravity IDE (`~/.gemini/antigravity-ide`).

## 清理范围

按来源和会话 ID 精确清理, 删除后 CC Switch 等工具刷新也不会再列出:

| 位置 | 内容 |
|------|------|
| `conversations/<id>.db` 及 `-wal`, `-shm`, `-journal`, `<id>.pb` | 会话正文 |
| `brain/<id>/` | 日志和产物 |
| `annotations/<id>.pbtxt`, `presence/<id>.lock` | 注解和占用锁 |
| `conversation_summaries.db` | 摘要, 全文索引, 父子关系 |
| `agyhub_summaries_proto.pb`, `jetbox_summaries_proto.pb` | 摘要索引 |
| `cache/last_conversations.json`, `cache/conversation_metadata.json` | 最近会话和摘要缓存 |
| `history.jsonl` | Antigravity CLI 输入历史中属于该会话的条目 |
| `sidecar_data/<插件>/<面板>/events/*.json` | 插件 sidecar 事件 |
| `sidecar_data/<插件>/<面板>/` 下的 `data/trajectory_usage_cache.json`, `logs/sidecar.log` | Usage Monitor 缓存条目和相关日志行 |
| Antigravity App 数据目录的 `app_storage.json` | 窗口布局和侧栏状态 |
| `sidecar_data/anti-power-usage/` 下的 `ledger.json` | 用量插件中该会话的合计, 并入匿名的 "已删除会话" |

- 子代理会话随所属会话一起删除, `/fork` 出的会话独立保留.
- 同一 ID 可能同时出现在 Antigravity App 和 Antigravity CLI 中, 两边分别显示; 若要一并清除, 需分别勾选.
- Antigravity App 数据目录在 macOS 上是 `~/Library/Application Support/Antigravity`, Windows 上是 `%APPDATA%\Antigravity`, Linux 上是 `~/.config/Antigravity`.
- 不清理运行日志 (`log/`), 也不清理写到 `/tmp` 或项目目录里的文件.
- 清理后会压缩 `conversation_summaries.db`, 库内不留已删记录的片段; 若此时有其他工具正在读取该库, 压缩会被跳过, 片段会留到下次清理. 被删除的文件本身不做磁盘级擦除.

界面上方有两个默认关闭的可选项, 勾选状态会被记住:

- **CC Switch 同步记录**: 从 `~/.cc-switch/` 下的 `cc-switch.db` 及 `backups/*.db` 中删除所选会话及存储中已无痕迹会话的同步进度 (`session_log_sync`), 并压缩数据库. CC Switch 运行中也可清理 (此时跳过主库压缩, 退出 CC Switch 后刷新一次即可完成复查). 用量和费用历史 (`proxy_request_logs`, `session_usage_dedup`, `usage_daily_rollups`) 会保留; 它们只含会话 ID, 模型和 token 用量, 不含对话内容.
- **Antigravity App 浏览器缓存**: 清空 Antigravity App 数据目录下的 `Cache/` 和 `Code Cache/`. 这两个目录无法按会话过滤, 可能留有会话 ID; Antigravity App 会自动重建, 下次启动稍慢.

应用数据目录和 CC Switch 的清理在会话删除提交后执行; 若失败, 会话不会回滚, 结果中会列出未清理项.

删除前须退出 Antigravity App 和 Antigravity CLI; Antigravity IDE 运行时同样会拒绝清理. 删除无法撤销.

如果清理中途退出, 关闭上述客户端后重新打开 Anti-Power 或刷新列表: 已提交的清理会继续收尾, 尚未提交的清理会还原; 无法恢复时列表上方会给出提示.

## 用量插件

Anti-Power 附带 Antigravity 插件 **Anti-Power Usage**, 记录每次模型请求:

- 用户消息, 模型回复, 思考过程和工具调用
- 新输入, 缓存读取, 输出和思考 token, 缓存命中率
- 首字延迟, 请求耗时和输出速度
- 按模型价格估算的费用; 没有价格的模型标为 "待定", 不计入费用合计
- 按今天, 7 天, 30 天或自定义区间汇总, 按日期, 模型, 来源和会话拆分

点击标题栏的用量插件按钮即可安装, 更新或卸载, 然后在 Antigravity App 的 **Customizations → Installed** 中开启, 从侧边栏打开. 插件进程启动时及之后每 10 秒采集一次, 关闭面板期间也会采集. 插件只读本地会话库, 不调用 Language Server, 也不上传数据; 对话内容在打开会话时现读, 不另存.

侧栏顶部显示当前会话的请求速度, 首字延迟和上下文占比. 展开可查看已用 token, 窗口上限和用量构成. Antigravity CLI 状态栏也可独立显示这三项指标, 例如 `48.8 t/s | 4.4s | ◎ 19%`.

macOS 和 Linux 安装插件时会为已有 CLI 配置状态栏 (已有其他配置或已关闭时可勾选替换, 卸载时还原). macOS 如需在窗口右上角挂载这三项指标 (收起侧栏后仍会更新), 退出 App 后在插件弹窗点击 **启动 Antigravity 并启用挂载**.

会话的所有本地副本删除后, 已记录用量会并入匿名的 "已删除会话" (外部移走文件约 60 秒后确认归并). 价格表来自 [antigravity-usage-monitor](https://github.com/yuhuan-dev/antigravity-usage-monitor), 可在 Anti-Power 的插件弹窗中更新; 需要配额监控可改用该插件.

## 安装和运行

在 [Releases](https://github.com/mikessslxxx/anti-power/releases) 下载对应平台的安装包:

- **Windows**: `anti-power-windows.exe`
- **macOS (Universal)**: `anti-power-macos-universal.dmg` (未签名, 首次打开若提示 "已损坏", 运行 `xattr -cr /Applications/Anti-Power.app`)
- **Linux**: `anti-power-linux.AppImage`

从源码运行需要 Node.js 22.12+ 和 Rust 1.95+:

```bash
cd patcher
npm install
npm run tauri:dev
```

## 旧版补丁

Google Antigravity 已内置 LaTeX 和 Mermaid 渲染, 补丁功能已移除. [v4.0.0](https://github.com/mikessslxxx/anti-power/releases/tag/v4.0.0) 是最后一个包含补丁的版本, 支持到 Antigravity IDE v2.0.6.

## 文档

- 更新日志: [CHANGELOG.md](CHANGELOG.md)
- 发布指南: [release-guide.md](docs/guides/release-guide.md)

## 贡献

欢迎提交 Issue 和 Pull Request.

## 致谢

- [@daoif](https://github.com/daoif) (原作者)
- [@syanle](https://github.com/syanle)
- [@Sophomoresty](https://github.com/Sophomoresty)
- [@Wusir7355608](https://github.com/Wusir7355608)
- [@aiaiads](https://github.com/aiaiads)

## 许可证

[GPL-3.0](LICENSE). v5.0.1 及更早版本以 MIT 许可发布, 相应声明保留在 [LICENSE-MIT](LICENSE-MIT).

`plugin/` 中的部分代码改编自 [antigravity-usage-monitor](https://github.com/yuhuan-dev/antigravity-usage-monitor) (GPL-3.0), 详见 [plugin/NOTICE.md](plugin/NOTICE.md).
