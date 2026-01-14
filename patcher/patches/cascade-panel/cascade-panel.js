/**
 * Anti-Power 补丁入口
 * 根据配置文件动态加载功能模块
 */

import { loadStyle } from './utils.js';

const DEFAULT_CONFIG = { mermaid: true, math: true, copyButton: true, tableColor: true };

// 加载配置
const loadConfig = async () => {
    try {
        const res = await fetch('./cascade-panel/config.json', { cache: 'no-store' });
        if (!res.ok) {
            throw new Error(`Config load failed: ${res.status}`);
        }
        const data = await res.json();
        if (!data || typeof data !== 'object' || Array.isArray(data)) {
            return DEFAULT_CONFIG;
        }
        return data;
    } catch {
        // 默认全部启用
        return DEFAULT_CONFIG;
    }
};

// 动态加载表格修复样式
const loadTableFix = () => {
    void loadStyle('./cascade-panel/table-fix.css').catch((error) => {
        console.warn('[Cascade] 表格样式加载失败:', error);
    });
};

// 入口
(async () => {
    const config = await loadConfig();

    // 表格颜色修复（CSS 动态加载）
    if (config.tableColor) {
        loadTableFix();
    }

    // 启动扫描模块，传入配置
    const { start } = await import('./scan.js');
    start(config);
})();
