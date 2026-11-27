# Release Notes / 发布说明 (v5.4.3)

## 会话清理

- 识别 Usage Monitor 1.5.6 清理迁移归档后保留的 `.protected.json` 保护标记, 拒绝删除源端残留并跳过其历史残留索引, 保留迁入会话可能引用的附件

---

## Session cleanup

- Recognize `.protected.json` markers left by Usage Monitor 1.5.6 after pruning migration archives, refusing to delete source leftovers and skipping their leftover indexes so migrated sessions keep their attachments

---

## 安装 / Installation

- **Windows**: 下载 `anti-power-windows.exe` 安装运行 / Download and run `anti-power-windows.exe`
- **macOS (Universal)**: 下载 `anti-power-macos-universal.dmg` 安装 (未签名, 首次打开若提示 "已损坏", 运行 `xattr -cr /Applications/Anti-Power.app`) / Download `anti-power-macos-universal.dmg` (unsigned; if macOS reports it is damaged, run `xattr -cr /Applications/Anti-Power.app`)
- **Linux**: 下载 `anti-power-linux.AppImage` 运行 / Download and run `anti-power-linux.AppImage`
