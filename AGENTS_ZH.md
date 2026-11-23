# AGENTS_ZH.md

[English](AGENTS.md) | 中文

## 项目定位

Anti-Power 是一个桌面工具 (Tauri + Vue), 用来查看并清理 Antigravity App 和 Antigravity CLI (`agy`) 的本地会话, 不处理其他工具. 早期的 IDE 补丁 (Markdown 复制, LaTeX, Mermaid) 在 v4.0.0 之后已移除.

Anti-Power 可安装或卸载上游 Usage Monitor 正式版.

## 目录

- `patcher/src-tauri/src/commands/`: Rust 后端
  - `antigravity_store.rs`: 存储根目录, ID 校验, 不跟随符号链接的路径解析
  - `antigravity_scan.rs`: 会话列表和消息读取
  - `antigravity_clean.rs`: 会话清理
  - `cleanup_extras.rs`: Antigravity App 窗口状态和缓存, CC Switch 同步记录, 用量账本清理
  - `usage_monitor.rs`: 可选清理 Usage Monitor 中已删除的本地会话明细
  - `usage_ledger.rs`: 将已删除会话折入 Anti-Power Usage 的匿名合计
  - `client_versions.rs`: 读取本机 App 和 CLI 版本, 供会话来源选择器显示
  - `plugin_install.rs`: 下载, 安装和卸载 Usage Monitor
  - `plugin_links.rs`: 为 Usage Monitor 注入 Anti-Power 跳转按钮
  - `sessions.rs`: 会话相关的 Tauri 命令 (`scan_sessions`, `load_session_messages`, `delete_sessions`)
- `patcher/src/`: Vue 前端, 主界面是 `components/SessionViewer.vue`
- `patcher/src/locales/`: `zh-CN` 和 `en-US` 文案
- `.github/workflows/release.yml`: Windows, macOS, Linux 安装包构建

## 会话存储

| 来源 | 根目录 | 摘要索引 |
|------|--------|----------|
| Antigravity App (`antigravity`) | `~/.gemini/antigravity` | `agyhub_summaries_proto.pb` |
| Antigravity CLI (`agy`) | `~/.gemini/antigravity-cli`, `~/.cc-switch/agy/<profile>/antigravity-cli` | `jetbox_summaries_proto.pb` |

会话由来源和 UUID 共同确定. 在每个根目录下, 一个会话可能出现在 `conversations/`, `brain/`, `annotations/`, `presence/`, `conversation_summaries.db` (摘要, 全文索引, 父子关系), 摘要索引, `cache/last_conversations.json`, `cache/conversation_metadata.json`, `history.jsonl` (Antigravity CLI 的输入历史) 和插件的 `sidecar_data/`. 存储目录之外, Antigravity App 在 `app_storage.json` 中保存窗口状态, CC Switch 在 `~/.cc-switch/cc-switch.db` 及其备份中保存同步进度. 清理和保留的范围见 README.

最近在 macOS 上核对的版本是 Antigravity App 2.34.0 和 Antigravity CLI 1.5.14. Antigravity IDE (`~/.gemini/antigravity-ide`) 不在范围内.

## 清理规则

- Antigravity App, Antigravity CLI 或 Antigravity IDE 运行时拒绝清理. CC Switch 和 Usage Monitor 可保持运行 (Usage Monitor 运行时暂留待清理记录, 退出后下次扫描再清理缓存).
- 不跟随存储目录内的符号链接.
- 先把待删文件移入 `.anti-power-clean-*`, 并在索引事务中记录提交. 清理中断后, 还原尚未提交的文件, 完成已提交的清理. 扫描时恢复失败只给出警告, 仍然列出会话.
- 批量清理时每个存储目录只处理一遍: 一次事务, 一次全文索引重建, 一次 `VACUUM`. 整批失败时逐条重试, 但属于整个存储目录的错误不再重试.
- 摘要索引格式无法识别时直接失败, 不做任何修改.
- 存储目录外的数据在存储提交后通过持久化收据 (`.anti-power-extras-*`, `.anti-power-sync-pending`) 清理, 未完成项在后续扫描时重试, 失败作为警告返回.
- 子代理会话 (`nesting_depth > 0`) 随所属会话删除, fork 保留.
- 其他工具能列出的会话这里也要能列出, 包括只剩索引的记录.

## 构建和测试

在 `patcher/` 下执行:

```bash
npm install
npm run build          # 类型检查和前端构建
npm run tauri:dev      # 运行应用
cd src-tauri && cargo test && cargo clippy --all-targets
```

测试在临时目录里构造存储. 测试中不要对真实存储执行清理.

## 约定

- 代码和文档保持精简.
- 代码注释用英文, 注释符号后一个空格, 末尾不加句号.
- 中英文文档统一使用英文标点.
- 已发布的发布说明和更新日志条目保持原样.
- `patcher/package.json`, `patcher/src-tauri/tauri.conf.json` 和 `patcher/src-tauri/Cargo.toml` 共用一个版本号.
