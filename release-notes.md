# Release Notes / 发布说明 (v5.3.0)

## 用量插件

- 标题栏弹窗改为从 GitHub 下载安装或卸载上游 [Usage Monitor](https://github.com/yuhuan-dev/antigravity-usage-monitor) 最新正式版; 在 Antigravity 的 **Customizations → Installed** 中启用后可查看缓存命中率及账号的 5 小时和每周额度
- 安装时为官方插件添加两个 Anti-Power 按钮: 面板顶栏打开当前本地会话, 设置页打开 **Usage Monitor** 弹窗; 插件通过内置 ZIP 更新时会重新应用按钮补丁
- 卸载保留 Antigravity 数据目录中的设置和用量历史; 若安装目录含 Git 仓库或插件本地数据, 会备份整个目录并显示保存位置
- 从 v5.1–5.2 切换时, 可先用旧版卸载 Anti-Power Usage 以还原 CLI 状态栏; 原账本保留在本机, Usage Monitor 不会导入

## 会话清理

- 新增默认关闭的 **Usage Monitor 会话记录** 选项: 清除已无本地副本会话的缓存标题与提问并保留用量和费用; 已安装的插件具备匹配的独立用量记录时移除原对话条目

---

## Usage Monitor

- Install or uninstall the latest official [Usage Monitor](https://github.com/yuhuan-dev/antigravity-usage-monitor) release from GitHub via the title bar dialog; enable it under **Customizations → Installed** in Antigravity to view cache hit rates and 5-hour/weekly quotas
- Add two Anti-Power buttons to the official plugin on install: the panel header opens the current local session, and Settings opens the **Usage Monitor** dialog; the built-in ZIP updater reapplies them
- Keep settings and usage history in Antigravity's data directory on uninstall; back up the plugin folder and report its path when it contains a Git checkout or local plugin data
- When switching from v5.1–5.2, uninstall Anti-Power Usage with the older Anti-Power first to restore its CLI status line; the existing ledger stays on disk and is not imported

## Session cleanup

- Add an optional **Usage Monitor session history** cleanup that clears cached titles and prompts for sessions with no local copy left while preserving token counts and costs, removing the entry when the installed plugin has a matching independent usage record

---

## 安装 / Installation

- **Windows**: 下载 `anti-power-windows.exe` 安装运行 / Download and run `anti-power-windows.exe`
- **macOS (Universal)**: 下载 `anti-power-macos-universal.dmg` 安装 (未签名, 首次打开若提示 "已损坏", 运行 `xattr -cr /Applications/Anti-Power.app`) / Download `anti-power-macos-universal.dmg` (unsigned; if macOS reports it is damaged, run `xattr -cr /Applications/Anti-Power.app`)
- **Linux**: 下载 `anti-power-linux.AppImage` 运行 / Download and run `anti-power-linux.AppImage`
