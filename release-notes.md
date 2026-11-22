# Release Notes / 发布说明 (v5.2.1)

## Anti-Power Usage 用量插件

修复 Antigravity App 启用顶栏挂载后不显示用量指标的问题: 窗口右上角显示当前会话的输出速度, 首字延迟和上下文占比, 随会话切换更新, 收起侧栏后仍可见.

CLI 状态栏分别保留当前会话最近一次实测的速度和首字延迟, 暂无测速时显示活动状态, 并在新会话建库完成前跳过读取以避免争锁.

在 macOS 上安装新版 Anti-Power 后, 打开 **用量插件** 安装或点击 **更新到 v5.2.1**, 退出 Antigravity App 后再点击 **启动 Antigravity 并启用挂载** (首次安装需在 Antigravity 的 **Customizations → Installed** 中启用插件).

## Anti-Power Usage

Fix the missing title-bar usage strip in the Antigravity App: each window's upper-right corner shows the current conversation's output speed, time to first token, and context usage, following conversation switches and staying visible with the side panel closed.

The CLI status line keeps the current conversation's latest measured speed and first-token latency, shows the activity state until speed is available, and skips uninitialized SQLite files to avoid lock contention.

On macOS, install the new Anti-Power, open the usage plugin dialog to install or choose **Update to v5.2.1**, then quit the Antigravity App and click **Launch Antigravity with debugging** (on first install, enable the plugin under **Customizations → Installed** in Antigravity).

---

## 安装 / Installation

- **Windows**: 下载 `anti-power-windows.exe` 安装运行 / Download and run `anti-power-windows.exe`
- **macOS (Universal)**: 下载 `anti-power-macos-universal.dmg` 安装 (未签名, 首次打开若提示 "已损坏", 运行 `xattr -cr /Applications/Anti-Power.app`) / Download `anti-power-macos-universal.dmg` (unsigned; if macOS reports it is damaged, run `xattr -cr /Applications/Anti-Power.app`)
- **Linux**: 下载 `anti-power-linux.AppImage` 运行 / Download and run `anti-power-linux.AppImage`
