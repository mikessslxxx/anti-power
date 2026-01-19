// 补丁安装/卸载模块

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use crate::embedded;

/// 功能开关配置
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

/// 安装补丁
#[tauri::command]
pub fn install_patch(path: String, features: FeatureConfig) -> Result<(), String> {
    let antigravity_path = PathBuf::from(&path);
    
    // 目标目录
    let extensions_dir = antigravity_path
        .join("resources")
        .join("app")
        .join("extensions")
        .join("antigravity");

    if !extensions_dir.exists() {
        return Err("无效的 Antigravity 安装目录".to_string());
    }

    // 备份原文件
    backup_original_files(&extensions_dir)?;

    // 写入补丁文件（从嵌入资源）
    write_embedded_patches(&extensions_dir, &features)?;

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

    if !extensions_dir.exists() {
        return Err("无效的 Antigravity 安装目录".to_string());
    }

    // 恢复备份文件
    restore_backup_files(&extensions_dir)?;

    Ok(())
}

/// 仅更新配置文件（不重新复制补丁文件）
#[tauri::command]
pub fn update_config(path: String, features: FeatureConfig) -> Result<(), String> {
    let antigravity_path = PathBuf::from(&path);
    
    let config_path = antigravity_path
        .join("resources")
        .join("app")
        .join("extensions")
        .join("antigravity")
        .join("cascade-panel")
        .join("config.json");

    if !config_path.parent().map(|p| p.exists()).unwrap_or(false) {
        return Err("补丁尚未安装，请先安装补丁".to_string());
    }

    write_config_file(&config_path, &features)?;

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
fn backup_original_files(extensions_dir: &PathBuf) -> Result<(), String> {
    let cascade_panel = extensions_dir.join("cascade-panel.html");
    let backup_path = extensions_dir.join("cascade-panel.html.bak");

    // 如果备份不存在，则创建备份
    if cascade_panel.exists() && !backup_path.exists() {
        fs::copy(&cascade_panel, &backup_path)
            .map_err(|e| format!("备份失败: {}", e))?;
    }

    Ok(())
}

/// 写入嵌入的补丁文件
fn write_embedded_patches(extensions_dir: &PathBuf, features: &FeatureConfig) -> Result<(), String> {
    let cascade_panel_dir = extensions_dir.join("cascade-panel");
    
    // 先删除旧目录，确保文件结构干净
    if cascade_panel_dir.exists() {
        fs::remove_dir_all(&cascade_panel_dir)
            .map_err(|e| format!("删除旧补丁目录失败: {}", e))?;
    }
    
    // 创建目录
    fs::create_dir_all(&cascade_panel_dir)
        .map_err(|e| format!("创建补丁目录失败: {}", e))?;
    
    // 写入所有嵌入文件
    for (relative_path, content) in embedded::get_all_files() {
        let full_path = extensions_dir.join(relative_path);
        
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
    
    // 生成配置文件
    let config_path = cascade_panel_dir.join("config.json");
    write_config_file(&config_path, features)?;

    Ok(())
}

/// 写入配置文件
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

/// 恢复备份文件
fn restore_backup_files(extensions_dir: &PathBuf) -> Result<(), String> {
    let cascade_panel = extensions_dir.join("cascade-panel.html");
    let backup_path = extensions_dir.join("cascade-panel.html.bak");

    if backup_path.exists() {
        fs::copy(&backup_path, &cascade_panel)
            .map_err(|e| format!("恢复失败: {}", e))?;
    } else {
        return Err("未找到备份文件".to_string());
    }

    // 删除补丁目录
    let patch_dir = extensions_dir.join("cascade-panel");
    if patch_dir.exists() {
        fs::remove_dir_all(&patch_dir)
            .map_err(|e| format!("删除补丁目录失败: {}", e))?;
    }

    Ok(())
}
