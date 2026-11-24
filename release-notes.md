# Release Notes / 发布说明 (v5.4.1)

## 会话清理

- 新增 **清理 Magpie 模型探测会话**, 可预览并批量删除 Magpie 独立 agy 存储中的空探测会话及其索引, 保留正常会话
- 优先使用 `AGY_BRIDGE_GEMINI_DIR` 指定的独立存储, 未设置时使用 Magpie 默认路径

## 界面

- Magpie 清理按钮与刷新按钮右对齐; 窄窗口中清理选项自动换行, 最小窗口尺寸调整为 640 × 460

---

## Session cleanup

- Add **Clean Magpie model probes** to preview and batch-delete empty probe sessions and their indexes from Magpie's separate agy store, keeping regular sessions
- Use the separate store configured in `AGY_BRIDGE_GEMINI_DIR` when set, or Magpie's default location otherwise

## Interface

- Align the Magpie cleanup and Refresh buttons on the right; wrap cleanup options in narrow windows, with a minimum window size of 640 × 460

---

## 安装 / Installation

- **Windows**: 下载 `anti-power-windows.exe` 安装运行 / Download and run `anti-power-windows.exe`
- **macOS (Universal)**: 下载 `anti-power-macos-universal.dmg` 安装 (未签名, 首次打开若提示 "已损坏", 运行 `xattr -cr /Applications/Anti-Power.app`) / Download `anti-power-macos-universal.dmg` (unsigned; if macOS reports it is damaged, run `xattr -cr /Applications/Anti-Power.app`)
- **Linux**: 下载 `anti-power-linux.AppImage` 运行 / Download and run `anti-power-linux.AppImage`
