# Release Notes / 发布说明 (v5.1.0)

## 用量插件

- 随附 **Anti-Power Usage**: 从标题栏弹窗安装, 升级, 卸载或更新价格表, 在 Antigravity App 的 **Customizations → Installed** 中启用; 卸载保留已记录用量
- 后台自动采集本地请求, 可在侧边栏按日期, 模型, 来源或会话查看 token 用量, 缓存命中率和估算费用, 并展开查看每轮输入, 回复, 思考, 工具调用, 首字延迟和输出速度 (对话正文按需从本机库读取)
- 同一会话的多个本地副本合并计量; 删除最后一份副本后, 已记录用量和费用并入匿名的 "已删除会话" (外部移走文件在确认缺失满 60 秒后归并)

## 会话清理

- 勾选 **CC Switch 同步记录** 时一并清理本地存储中已消失会话在 CC Switch 主库和备份中的残留同步进度 (保留用量和费用历史); 支持在 CC Switch 运行时清理, 退出后刷新即可完成复查
- 删除会话后若窗口状态, 用量汇总, 缓存或同步记录未清理完, 重新打开 Anti-Power 或刷新列表会继续处理

## 许可证

- 自 v5.1.0 起采用 GPL-3.0-only; v5.0.1 及更早版本的 MIT 声明保留在 `LICENSE-MIT`, 插件上游来源见 `plugin/NOTICE.md`

---

## Usage Plugin

- Bundle **Anti-Power Usage**: install, upgrade, uninstall, or update prices from the title bar dialog, then enable under **Customizations → Installed** in the Antigravity App; uninstalling preserves recorded usage
- Collect local requests in the background and view token usage, cache hit rate, and estimated cost by date, model, source, or session; open a session to inspect each turn's prompt, reply, thinking, tool calls, time to first token, and output speed (content is read on demand from local stores)
- Count multiple local copies of a session once, and fold recorded usage and cost into anonymous "Deleted sessions" totals after the last copy is deleted (or confirmed missing for 60 seconds)

## Session Cleanup

- **CC Switch sync records** also clears leftover sync cursors for sessions already gone from local stores across the main database and backups (preserving usage and cost history), and works while CC Switch is open (refresh after it exits to finish verification)
- Reopening Anti-Power or refreshing the session list retries any unfinished window state, usage ledger, cache, or sync cleanup

## License

- Switch to GPL-3.0-only starting with v5.1.0; the MIT notice for v5.0.1 and earlier stays in `LICENSE-MIT`, and upstream plugin credits are in `plugin/NOTICE.md`

---

## 安装 / Installation

- **Windows**: 下载 `anti-power-windows.exe` 安装运行 / Download and run `anti-power-windows.exe`
- **macOS (Universal)**: 下载 `anti-power-macos-universal.dmg` 安装 (未签名, 首次打开若提示 "已损坏", 运行 `xattr -cr /Applications/Anti-Power.app`) / Download `anti-power-macos-universal.dmg` (unsigned; if macOS reports it is damaged, run `xattr -cr /Applications/Anti-Power.app`)
- **Linux**: 下载 `anti-power-linux.AppImage` 运行 / Download and run `anti-power-linux.AppImage`
