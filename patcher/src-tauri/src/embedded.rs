// 嵌入的补丁资源
// 使用 include_str! 将文件内容在编译时嵌入到二进制中
use std::fs;
use std::path::PathBuf;

/// cascade-panel.html 入口文件
pub const CASCADE_PANEL_HTML: &str = include_str!("../../patches/cascade-panel.html");

/// workbench-jetski-agent.html 入口文件 (Manager 窗口)
pub const WORKBENCH_JETSKI_AGENT_HTML: &str = include_str!("../../patches/workbench-jetski-agent.html");

/// cascade-panel/ 目录下的文件
pub mod cascade_panel {
    pub const CASCADE_PANEL_CSS: &str = include_str!("../../patches/cascade-panel/cascade-panel.css");
    pub const CASCADE_PANEL_JS: &str = include_str!("../../patches/cascade-panel/cascade-panel.js");
    pub const CONSTANTS_JS: &str = include_str!("../../patches/cascade-panel/constants.js");
    pub const COPY_JS: &str = include_str!("../../patches/cascade-panel/copy.js");
    pub const EXTRACT_JS: &str = include_str!("../../patches/cascade-panel/extract.js");
    pub const ICONS_JS: &str = include_str!("../../patches/cascade-panel/icons.js");
    pub const MATH_JS: &str = include_str!("../../patches/cascade-panel/math.js");
    pub const MERMAID_JS: &str = include_str!("../../patches/cascade-panel/mermaid.js");
    pub const SCAN_JS: &str = include_str!("../../patches/cascade-panel/scan.js");
    pub const TABLE_FIX_CSS: &str = include_str!("../../patches/cascade-panel/table-fix.css");
    pub const UTILS_JS: &str = include_str!("../../patches/cascade-panel/utils.js");
    // config.json 不内嵌，因为需要动态生成
}

/// manager-panel/ 目录下的文件
pub mod manager_panel {
    pub const MANAGER_PANEL_CSS: &str = include_str!("../../patches/manager-panel/manager-panel.css");
    pub const MANAGER_PANEL_JS: &str = include_str!("../../patches/manager-panel/manager-panel.js");
    pub const CONSTANTS_JS: &str = include_str!("../../patches/manager-panel/constants.js");
    pub const COPY_JS: &str = include_str!("../../patches/manager-panel/copy.js");
    pub const MATH_JS: &str = include_str!("../../patches/manager-panel/math.js");
    pub const MERMAID_JS: &str = include_str!("../../patches/manager-panel/mermaid.js");
    pub const SCAN_JS: &str = include_str!("../../patches/manager-panel/scan.js");
    pub const UTILS_JS: &str = include_str!("../../patches/manager-panel/utils.js");
    // config.json 不内嵌，因为需要动态生成
}

/// 所有需要写入的文件列表
pub fn get_all_files() -> Vec<(&'static str, &'static str)> {
    vec![
        // cascade-panel (侧边栏)
        ("cascade-panel.html", CASCADE_PANEL_HTML),
        ("cascade-panel/cascade-panel.css", cascade_panel::CASCADE_PANEL_CSS),
        ("cascade-panel/cascade-panel.js", cascade_panel::CASCADE_PANEL_JS),
        ("cascade-panel/constants.js", cascade_panel::CONSTANTS_JS),
        ("cascade-panel/copy.js", cascade_panel::COPY_JS),
        ("cascade-panel/extract.js", cascade_panel::EXTRACT_JS),
        ("cascade-panel/icons.js", cascade_panel::ICONS_JS),
        ("cascade-panel/math.js", cascade_panel::MATH_JS),
        ("cascade-panel/mermaid.js", cascade_panel::MERMAID_JS),
        ("cascade-panel/scan.js", cascade_panel::SCAN_JS),
        ("cascade-panel/table-fix.css", cascade_panel::TABLE_FIX_CSS),
        ("cascade-panel/utils.js", cascade_panel::UTILS_JS),
        // manager-panel (Manager 窗口)
        ("workbench-jetski-agent.html", WORKBENCH_JETSKI_AGENT_HTML),
        ("manager-panel/manager-panel.css", manager_panel::MANAGER_PANEL_CSS),
        ("manager-panel/manager-panel.js", manager_panel::MANAGER_PANEL_JS),
        ("manager-panel/constants.js", manager_panel::CONSTANTS_JS),
        ("manager-panel/copy.js", manager_panel::COPY_JS),
        ("manager-panel/math.js", manager_panel::MATH_JS),
        ("manager-panel/mermaid.js", manager_panel::MERMAID_JS),
        ("manager-panel/scan.js", manager_panel::SCAN_JS),
        ("manager-panel/utils.js", manager_panel::UTILS_JS),
    ]
}

fn find_patches_dir() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;

    for _ in 0..6 {
        let direct = dir.join("patches");
        if direct.is_dir() {
            return Some(direct);
        }

        let nested = dir.join("patcher").join("patches");
        if nested.is_dir() {
            return Some(nested);
        }

        if !dir.pop() {
            break;
        }
    }

    None
}

pub fn get_all_files_runtime() -> Result<Vec<(String, String)>, String> {
    if cfg!(debug_assertions) {
        let patches_dir = find_patches_dir()
            .ok_or_else(|| "未找到 patches 目录，请从项目根目录或 patcher 目录启动".to_string())?;
        let mut files = Vec::new();
        for (relative_path, _) in get_all_files() {
            let full_path = patches_dir.join(relative_path);
            let content = fs::read_to_string(&full_path)
                .map_err(|e| format!("读取补丁文件失败 {:?}: {}", full_path, e))?;
            files.push((relative_path.to_string(), content));
        }
        return Ok(files);
    }

    Ok(get_all_files()
        .into_iter()
        .map(|(path, content)| (path.to_string(), content.to_string()))
        .collect())
}
