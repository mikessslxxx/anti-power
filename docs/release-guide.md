# Anti-Power Patcher 发布指南

## 版本号同步

发布前确保以下文件版本号一致：

| 文件 | 路径 |
|------|------|
| package.json | `patcher/package.json` → `version` |
| Tauri 配置 | `patcher/src-tauri/tauri.conf.json` → `version` |
| Cargo 配置 | `patcher/src-tauri/Cargo.toml` → `version` |
| 前端显示 | `patcher/src/App.vue` → `APP_VERSION` |
| README 版本徽章 | `README.md` → 顶部版本号徽章 |
| README 版本表格 | `README.md` → "📋 版本信息" 表格 |

---

## 编译

```powershell
cd patcher
npm run tauri:build
```

产物位置：`patcher/src-tauri/target/release/patcher.exe`

### 编译选项

修改 `tauri.conf.json` 中的 `bundle.targets`：

| 值 | 说明 |
|----|------|
| `[]` | 仅生成单体 exe |
| `["nsis"]` | 生成 Windows 安装包 |
| `"all"` | 生成所有格式 |

---

## 发布流程

```powershell
# 1. 提交代码
git add -A
git commit -m "release: vX.Y.Z"

# 2. 创建标签
git tag vX.Y.Z
git push origin master
git push origin vX.Y.Z
```

3. 访问 [GitHub Releases](https://github.com/daoif/anti-power/releases/new)
4. 选择标签，填写发布说明
5. 上传 `patcher.exe`
6. 发布

---

## 版本号规范

- **Major**: 不兼容的重大变更
- **Minor**: 新增功能
- **Patch**: Bug 修复
