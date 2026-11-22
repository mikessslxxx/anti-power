# Release Notes / 发布说明 (v5.2.0)

## Anti-Power Usage 用量插件
- 修复 Antigravity App 侧栏打开显示空白的问题; 窄侧栏中会话和请求明细按字段分行显示, 图表随宽度自适应
- 当前会话指标栏显示最近一次模型请求的速度, 首字延迟和上下文占比, 展开可查看已用 token, 窗口上限和分类构成 (会话压缩后按保留内容和摘要计算)
- Antigravity CLI 状态栏居中显示这三项指标, 两侧保留快捷键提示与模式, 模型, 推理档位; macOS 和 Linux 安装时自动为已有 CLI 配置 (已有其他配置或已关闭时需勾选替换, 卸载时自动还原)

## 客户端版本与启动
- 会话来源选择器显示本机检测到的 Antigravity App 和 Antigravity CLI 版本
- macOS 用量插件弹窗新增 **启动 Antigravity 并启用挂载**: 退出 App 后以本机随机调试端口启动, 供桌面顶栏挂载使用 (侧栏和 CLI 状态栏无需此项)

---

## Anti-Power Usage
- Fix a blank side panel in the Antigravity App; narrow panels show session and request details as labeled fields, and charts resize with the panel
- Show the latest request's speed, time to first token, and context utilization in the session strip; expand for used tokens, window limit, and category breakdown (accounting for retained content and summary after compaction)
- Center the same three metrics in the Antigravity CLI footer between the shortcut hint and the active mode, model, and effort; on macOS and Linux, installing configures an existing CLI profile unless another or disabled status line is set (opt in to replace, restored on uninstall)

## Client versions and launch options
- Show detected Antigravity App and Antigravity CLI versions in the session source selector
- Add **Launch Antigravity with debugging** on macOS: quit the app and launch with a random loopback debugging port for desktop overlays (the side panel and CLI status line work without it)

---

## 安装 / Installation

- **Windows**: 下载 `anti-power-windows.exe` 安装运行 / Download and run `anti-power-windows.exe`
- **macOS (Universal)**: 下载 `anti-power-macos-universal.dmg` 安装 (未签名, 首次打开若提示 "已损坏", 运行 `xattr -cr /Applications/Anti-Power.app`) / Download `anti-power-macos-universal.dmg` (unsigned; if macOS reports it is damaged, run `xattr -cr /Applications/Anti-Power.app`)
- **Linux**: 下载 `anti-power-linux.AppImage` 运行 / Download and run `anti-power-linux.AppImage`

从 v5.1.0 升级时, 安装新版 Anti-Power 后打开 **用量插件**, 点击 **更新到 v5.2.0**. 若 Antigravity App 正在运行, 重启后打开侧栏即可使用新版插件.

If you have the v5.1.0 plugin installed, open the usage plugin dialog in the new Anti-Power and choose **Update to v5.2.0**. If the Antigravity App is running, restart it before opening the updated panel.
