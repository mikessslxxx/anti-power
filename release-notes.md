# Release Notes / 发布说明 (v5.4.0)

## 界面

- 顶部工具栏加双栏卡片布局, 深浅主题配色改为深蓝, 砖红, 铜棕
- 会话来源改为分段切换并显示本机 App 和 CLI 版本; 会话列表在标题下方显示来源, 时间和项目; 支持隐藏仅索引会话 (按完整 ID 搜索时仍显示)
- 会话详情支持在文件管理器中显示项目目录和会话文件

## 会话内容

- 助手回复按 Markdown 渲染 (标题, 列表, 任务列表, 表格, 代码块); 原始 HTML 显示为文本, 不加载远程图片
- 本地文件链接显示为标签, 点击可在文件管理器中定位; 网页链接用系统浏览器打开
- 用户消息改为右侧气泡, 连续回复合并显示; 长消息默认折叠, 离开底部时可跳到最新消息

## 会话清理

- 删除完整会话后一并清理其他来源中同 ID 的仅索引记录, 并在确认后清理各本地来源均无会话文件, 附件和迁移归档的历史残留索引
- Usage Monitor 完整迁移会话后拒绝删除源端残留, 以保留迁入会话可能引用的附件; 迁移失败的归档不影响删除, 自定义 `--data-dir` 不在检查范围内

---

## Interface

- Redesign the window with a toolbar above two cards and a navy, rust, and bronze palette across dark and light themes
- Switch session sources with a segmented control showing installed App and CLI versions, and list each session's source, time, and project under its title; add a toggle to hide index-only sessions (full-ID search still shows matches)
- Reveal the project directory and session file in the file manager from session details

## Conversation view

- Render assistant replies as Markdown (headings, lists, task lists, tables, code blocks); show raw HTML as text and never load remote images
- Render local file links as chips that reveal in the file manager, and open web links in the system browser
- Show user messages as right-aligned bubbles and group consecutive replies; collapse long messages by default and offer a jump to the latest message when scrolled away

## Session cleanup

- Clear matching index-only records in other sources after deleting a complete session, and clear confirmed leftover indexes that have no session files, attachments, or migration archive in any local source
- Refuse to delete source leftovers after a full Usage Monitor migration so migrated sessions keep their attachments; archives from failed moves do not block deletion, and custom `--data-dir` paths are not checked

---

## 安装 / Installation

- **Windows**: 下载 `anti-power-windows.exe` 安装运行 / Download and run `anti-power-windows.exe`
- **macOS (Universal)**: 下载 `anti-power-macos-universal.dmg` 安装 (未签名, 首次打开若提示 "已损坏", 运行 `xattr -cr /Applications/Anti-Power.app`) / Download `anti-power-macos-universal.dmg` (unsigned; if macOS reports it is damaged, run `xattr -cr /Applications/Anti-Power.app`)
- **Linux**: 下载 `anti-power-linux.AppImage` 运行 / Download and run `anti-power-linux.AppImage`
