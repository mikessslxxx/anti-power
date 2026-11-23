<p align="center">
  <img src="docs/assets/images/LOGO.png" alt="Anti-Power" width="120">
</p>

<h1 align="center">Anti-Power</h1>

<p align="center">
  <a href="https://github.com/mikessslxxx/anti-power/releases"><img src="https://img.shields.io/badge/版本-v5.3.0-blue.svg" alt="版本"></a>
  <img src="https://img.shields.io/badge/Antigravity_App-2.34.0-green.svg" alt="Antigravity App">
  <img src="https://img.shields.io/badge/Antigravity_CLI-1.5.14-green.svg" alt="Antigravity CLI">
</p>

<p align="center">
  中文 | <a href="README_EN.md">English</a>
</p>

> 查看和清理 **Antigravity App** 和 **Antigravity CLI** (`agy`) 的本地会话, 支持安装 Usage Monitor.

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
| `sidecar_data/<插件>/<面板>/logs/sidecar.log` | 已删会话的日志行 |
| Antigravity App 数据目录的 `app_storage.json` | 窗口布局和侧栏状态 |
| `sidecar_data/anti-power-usage/` 下的 `ledger.json` | 旧版 Anti-Power Usage 中该会话的合计, 并入匿名的 "已删除会话" |

- 子代理会话随所属会话一起删除, `/fork` 出的会话独立保留.
- 同一 ID 可能同时出现在 Antigravity App 和 Antigravity CLI 中, 两边分别显示; 若要一并清除, 需分别勾选.
- Antigravity App 数据目录在 macOS 上是 `~/Library/Application Support/Antigravity`, Windows 上是 `%APPDATA%\Antigravity`, Linux 上是 `~/.config/Antigravity`.
- 不清理运行日志 (`log/`), 也不清理写到 `/tmp` 或项目目录里的文件.
- 清理后会压缩 `conversation_summaries.db` 以清除已删记录碎片 (若正被其他工具读取则延至下次清理); 已删文件不做磁盘级擦除.

界面上方有三个默认关闭的可选项, 勾选状态会被记住:

- **Antigravity App 浏览器缓存**: 清空 Antigravity App 数据目录下的 `Cache/` 和 `Code Cache/`. 这两个目录无法按会话过滤, 可能留有会话 ID; Antigravity App 会自动重建, 下次启动稍慢.
- **CC Switch 同步记录**: 从 `~/.cc-switch/` 的 `cc-switch.db` 及 `backups/*.db` 中删除所选会话及在对应来源中已无痕迹会话的同步进度 (`session_log_sync`), 并压缩数据库 (CC Switch 运行时跳过主库压缩, 退出后刷新即可复查). 不含对话正文的用量与费用历史会保留.
- **Usage Monitor 会话记录**: 清除已无本地副本的 App 和 CLI 会话缓存的标题与提问, 保留用量和费用历史; 已安装的插件具备匹配的独立用量记录时移除原对话条目, 否则保留脱敏占位行. 独立运行的 Usage Monitor 可保持打开 (本地会话照常删除, 退出 Usage Monitor 后刷新即可完成缓存清理).

应用数据目录, CC Switch 和 Usage Monitor 的清理在会话删除提交后执行; 若失败, 会话不会回滚, 结果中会列出未清理项.

删除前须退出 Antigravity App 和 Antigravity CLI; Antigravity IDE 运行时同样会拒绝清理. 删除无法撤销.

如果清理中途退出, 关闭上述客户端后重新打开 Anti-Power 或刷新列表: 已提交的清理会继续收尾, 尚未提交的清理会还原; 无法恢复时列表上方会给出提示.

## 用量插件

通过标题栏的 **Usage Monitor** 按钮可将 [Usage Monitor 最新正式版](https://github.com/yuhuan-dev/antigravity-usage-monitor/releases/latest) 安装到 `~/.gemini/config/plugins/usage-monitor` 或卸载. 在 Antigravity 的 **Customizations → Installed** 中启用后即可查看缓存命中率和账号额度. 价格和 CLI 状态栏由插件自身管理 (若启用了 CLI 状态栏, 卸载前请先在插件内关闭).

安装时会为官方插件添加两个 Anti-Power 按钮: 面板顶栏打开当前本地会话, 设置页打开 **Usage Monitor** 弹窗. 插件通过内置 ZIP 更新时会重新应用按钮补丁.

macOS 用户可先退出 Antigravity, 再在 **Usage Monitor** 弹窗点击 **启动 Antigravity 并启用挂载**, 使用顶栏或输入框中的用量胶囊.

如果之前用 v5.1–5.2 安装过 Anti-Power Usage, 可先在旧版中卸载它并还原 CLI 状态栏. 原账本保留在本机, Usage Monitor 不会导入.

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
