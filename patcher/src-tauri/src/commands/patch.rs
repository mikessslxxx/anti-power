// 补丁安装/卸载模块

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use crate::embedded;

/// 侧边栏功能开关配置
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FeatureConfig {
    pub mermaid: bool,
    pub math: bool,
    #[serde(rename = "copyButton")]
    pub copy_button: bool,
    #[serde(rename = "tableColor")]
    pub table_color: bool,
    #[serde(rename = "fontSizeEnabled")]
    pub font_size_enabled: bool,
    #[serde(rename = "fontSize")]
    pub font_size: f32,
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            mermaid: true,
            math: true,
            copy_button: true,
            table_color: true,
            font_size_enabled: true,
            font_size: 20.0,
        }
    }
}

/// Manager 窗口功能开关配置
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ManagerFeatureConfig {
    pub mermaid: bool,
    pub math: bool,
    #[serde(rename = "copyButton")]
    pub copy_button: bool,
    #[serde(rename = "fontSizeEnabled")]
    pub font_size_enabled: bool,
    #[serde(rename = "fontSize")]
    pub font_size: f32,
}

impl Default for ManagerFeatureConfig {
    fn default() -> Self {
        Self {
            mermaid: false,
            math: false,
            copy_button: true,
            font_size_enabled: false,
            font_size: 16.0,
        }
    }
}

/// 安装补丁
#[tauri::command]
pub fn install_patch(
    path: String, 
    features: FeatureConfig,
    manager_features: ManagerFeatureConfig
) -> Result<(), String> {
    let antigravity_path = PathBuf::from(&path);
    
    // 侧边栏目标目录
    let extensions_dir = antigravity_path
        .join("resources")
        .join("app")
        .join("extensions")
        .join("antigravity");

    // Manager 目标目录 (不同的路径!)
    let workbench_dir = antigravity_path
        .join("resources")
        .join("app")
        .join("out")
        .join("vs")
        .join("code")
        .join("electron-browser")
        .join("workbench");

    if !extensions_dir.exists() {
        return Err("无效的 Antigravity 安装目录".to_string());
    }

    if !workbench_dir.exists() {
        return Err("Manager 窗口目录不存在".to_string());
    }

    // 备份原文件
    backup_original_files(&extensions_dir, &workbench_dir)?;

    // 写入补丁文件（从嵌入资源）
    write_embedded_patches(&extensions_dir, &workbench_dir, &features, &manager_features)?;

    Ok(())
}

/// 卸载补丁（恢复原版）
#[tauri::command]
pub fn uninstall_patch(path: String) -> Result<(), String> {
    let antigravity_path = PathBuf::from(&path);
    
    let extensions_dir = antigravity_path
        .join("resources")
        .join("app")
        .join("extensions")
        .join("antigravity");

    let workbench_dir = antigravity_path
        .join("resources")
        .join("app")
        .join("out")
        .join("vs")
        .join("code")
        .join("electron-browser")
        .join("workbench");

    if !extensions_dir.exists() {
        return Err("无效的 Antigravity 安装目录".to_string());
    }

    // 恢复备份文件
    restore_backup_files(&extensions_dir, &workbench_dir)?;

    Ok(())
}

/// 仅更新配置文件（不重新复制补丁文件）
#[tauri::command]
pub fn update_config(
    path: String, 
    features: FeatureConfig,
    manager_features: ManagerFeatureConfig
) -> Result<(), String> {
    let antigravity_path = PathBuf::from(&path);
    
    // 侧边栏配置
    let cascade_config_path = antigravity_path
        .join("resources")
        .join("app")
        .join("extensions")
        .join("antigravity")
        .join("cascade-panel")
        .join("config.json");

    if !cascade_config_path.parent().map(|p| p.exists()).unwrap_or(false) {
        return Err("补丁尚未安装，请先安装补丁".to_string());
    }

    write_config_file(&cascade_config_path, &features)?;

    // Manager 配置 (不同的路径)
    let manager_config_path = antigravity_path
        .join("resources")
        .join("app")
        .join("out")
        .join("vs")
        .join("code")
        .join("electron-browser")
        .join("workbench")
        .join("manager-panel")
        .join("config.json");

    if manager_config_path.parent().map(|p| p.exists()).unwrap_or(false) {
        write_manager_config_file(&manager_config_path, &manager_features)?;
    }

    Ok(())
}

/// 检测补丁是否已安装
#[tauri::command]
pub fn check_patch_status(path: String) -> Result<bool, String> {
    let antigravity_path = PathBuf::from(&path);
    
    let config_path = antigravity_path
        .join("resources")
        .join("app")
        .join("extensions")
        .join("antigravity")
        .join("cascade-panel")
        .join("config.json");

    // 如果 config.json 存在，则认为补丁已安装
    Ok(config_path.exists())
}

/// 读取已安装的补丁配置
#[tauri::command]
pub fn read_patch_config(path: String) -> Result<Option<FeatureConfig>, String> {
    let antigravity_path = PathBuf::from(&path);
    
    let config_path = antigravity_path
        .join("resources")
        .join("app")
        .join("extensions")
        .join("antigravity")
        .join("cascade-panel")
        .join("config.json");

    if !config_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("读取配置失败: {}", e))?;
    
    let config: FeatureConfig = serde_json::from_str(&content)
        .map_err(|e| format!("解析配置失败: {}", e))?;
    
    Ok(Some(config))
}

/// 备份原始文件
fn backup_original_files(extensions_dir: &PathBuf, workbench_dir: &PathBuf) -> Result<(), String> {
    // 备份 cascade-panel.html (在 extensions_dir)
    let cascade_panel = extensions_dir.join("cascade-panel.html");
    let cascade_backup = extensions_dir.join("cascade-panel.html.bak");
    if cascade_panel.exists() && !cascade_backup.exists() {
        fs::copy(&cascade_panel, &cascade_backup)
            .map_err(|e| format!("备份 cascade-panel.html 失败: {}", e))?;
    }

    // 备份 workbench-jetski-agent.html (在 workbench_dir)
    let jetski_agent = workbench_dir.join("workbench-jetski-agent.html");
    let jetski_backup = workbench_dir.join("workbench-jetski-agent.html.bak");
    if jetski_agent.exists() && !jetski_backup.exists() {
        fs::copy(&jetski_agent, &jetski_backup)
            .map_err(|e| format!("备份 workbench-jetski-agent.html 失败: {}", e))?;
    }

    Ok(())
}

/// 写入嵌入的补丁文件
fn write_embedded_patches(
    extensions_dir: &PathBuf,
    workbench_dir: &PathBuf,
    features: &FeatureConfig,
    manager_features: &ManagerFeatureConfig
) -> Result<(), String> {
    // 侧边栏补丁目录 (extensions_dir)
    let cascade_panel_dir = extensions_dir.join("cascade-panel");
    
    // Manager 补丁目录 (workbench_dir)
    let manager_panel_dir = workbench_dir.join("manager-panel");
    
    // 先删除旧目录，确保文件结构干净
    if cascade_panel_dir.exists() {
        fs::remove_dir_all(&cascade_panel_dir)
            .map_err(|e| format!("删除旧 cascade-panel 目录失败: {}", e))?;
    }
    if manager_panel_dir.exists() {
        fs::remove_dir_all(&manager_panel_dir)
            .map_err(|e| format!("删除旧 manager-panel 目录失败: {}", e))?;
    }
    
    // 创建目录
    fs::create_dir_all(&cascade_panel_dir)
        .map_err(|e| format!("创建 cascade-panel 目录失败: {}", e))?;
    fs::create_dir_all(&manager_panel_dir)
        .map_err(|e| format!("创建 manager-panel 目录失败: {}", e))?;
    
    // 写入所有补丁文件（开发模式优先使用磁盘文件）
    let patch_files = embedded::get_all_files_runtime()?;
    for (relative_path, content) in patch_files {
        // 根据文件路径决定写入哪个目录
        let full_path = if relative_path.starts_with("manager-panel") || relative_path == "workbench-jetski-agent.html" {
            workbench_dir.join(&relative_path)
        } else {
            extensions_dir.join(&relative_path)
        };
        
        // 确保父目录存在
        if let Some(parent) = full_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("创建目录失败: {}", e))?;
            }
        }
        
        fs::write(&full_path, content)
            .map_err(|e| format!("写入文件失败 {:?}: {}", full_path, e))?;
    }
    
    // 生成侧边栏配置文件
    let cascade_config_path = cascade_panel_dir.join("config.json");
    write_config_file(&cascade_config_path, features)?;

    // 生成 Manager 配置文件
    let manager_config_path = manager_panel_dir.join("config.json");
    write_manager_config_file(&manager_config_path, manager_features)?;

    Ok(())
}

/// 写入侧边栏配置文件
fn write_config_file(config_path: &PathBuf, features: &FeatureConfig) -> Result<(), String> {
    let config_content = serde_json::json!({
        "mermaid": features.mermaid,
        "math": features.math,
        "copyButton": features.copy_button,
        "tableColor": features.table_color,
        "fontSizeEnabled": features.font_size_enabled,
        "fontSize": features.font_size
    });
    
    fs::write(config_path, serde_json::to_string_pretty(&config_content).unwrap())
        .map_err(|e| format!("写入配置文件失败: {}", e))?;
    
    Ok(())
}

/// 写入 Manager 配置文件
fn write_manager_config_file(config_path: &PathBuf, features: &ManagerFeatureConfig) -> Result<(), String> {
    let config_content = serde_json::json!({
        "mermaid": features.mermaid,
        "math": features.math,
        "copyButton": features.copy_button,
        "fontSizeEnabled": features.font_size_enabled,
        "fontSize": features.font_size
    });
    
    fs::write(config_path, serde_json::to_string_pretty(&config_content).unwrap())
        .map_err(|e| format!("写入 Manager 配置文件失败: {}", e))?;
    
    Ok(())
}

/// 恢复备份文件
fn restore_backup_files(extensions_dir: &PathBuf, workbench_dir: &PathBuf) -> Result<(), String> {
    // 恢复 cascade-panel.html (在 extensions_dir)
    let cascade_panel = extensions_dir.join("cascade-panel.html");
    let cascade_backup = extensions_dir.join("cascade-panel.html.bak");
    if cascade_backup.exists() {
        fs::copy(&cascade_backup, &cascade_panel)
            .map_err(|e| format!("恢复 cascade-panel.html 失败: {}", e))?;
    }

    // 恢复 workbench-jetski-agent.html (在 workbench_dir)
    let jetski_agent = workbench_dir.join("workbench-jetski-agent.html");
    let jetski_backup = workbench_dir.join("workbench-jetski-agent.html.bak");
    if jetski_backup.exists() {
        fs::copy(&jetski_backup, &jetski_agent)
            .map_err(|e| format!("恢复 workbench-jetski-agent.html 失败: {}", e))?;
    }

    // 删除侧边栏补丁目录 (在 extensions_dir)
    let cascade_dir = extensions_dir.join("cascade-panel");
    if cascade_dir.exists() {
        fs::remove_dir_all(&cascade_dir)
            .map_err(|e| format!("删除 cascade-panel 目录失败: {}", e))?;
    }

    // 删除 Manager 补丁目录 (在 workbench_dir)
    let manager_dir = workbench_dir.join("manager-panel");
    if manager_dir.exists() {
        fs::remove_dir_all(&manager_dir)
            .map_err(|e| format!("删除 manager-panel 目录失败: {}", e))?;
    }

    Ok(())
}
