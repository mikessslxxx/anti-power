import { CONTENT_SELECTOR } from './constants.js';
import { addFeedbackCopyButtons, ensureContentCopyButton } from './copy.js';
import { renderMath } from './math.js';
import { renderMermaid } from './mermaid.js';

/**
 * 功能配置（由入口传入）
 */
let config = {
    mermaid: true,
    math: true,
    copyButton: true,
    tableColor: true
};

/**
 * 扫描根节点并处理需要增强的内容区域
 * @param {Element} root
 * @returns {void}
 */
const scan = (root) => {
    if (!root || !root.isConnected) return;

    // 检查根节点本身及子节点是否匹配内容选择器
    const contentNodes = [];
    if (root.matches && root.matches(CONTENT_SELECTOR)) {
        contentNodes.push(root);
    }
    contentNodes.push(...root.querySelectorAll(CONTENT_SELECTOR));

    contentNodes.forEach((node) => {
        if (config.copyButton) {
            ensureContentCopyButton(node);
        }
        if (config.math) {
            void renderMath(node);
        }
    });

    if (config.mermaid) {
        const mermaidNodes = [];
        if (root.matches && root.matches('[class*="language-mermaid"]')) {
            mermaidNodes.push(root);
        }
        mermaidNodes.push(...root.querySelectorAll('[class*="language-mermaid"]'));

        mermaidNodes.forEach((node) => {
            void renderMermaid(node);
        });
    }
};

/**
 * 获取渲染根节点，按优先级依次寻找
 * @returns {Element}
 */
const getRoot = () =>
    document.getElementById('chat') ||
    document.getElementById('react-app') ||
    document.body;

let pendingNodes = new Set();
let scheduled = false;

/**
 * 批量处理待扫描节点
 */
const flushScan = () => {
    scheduled = false;
    const nodes = [...pendingNodes];
    pendingNodes.clear();

    nodes.forEach(node => {
        if (node.isConnected) scan(node);
    });

    if (config.copyButton) {
        addFeedbackCopyButtons();
    }
};

/**
 * 调度扫描任务
 * @param {NodeList|Array} nodes 
 */
const scheduleScan = (nodes) => {
    let hasElements = false;
    const enqueue = (target) => {
        if (!target) return;
        const contentRoot = target.closest ? target.closest(CONTENT_SELECTOR) : null;
        pendingNodes.add(contentRoot || target);
        hasElements = true;
    };

    nodes.forEach((node) => {
        if (node.nodeType === Node.ELEMENT_NODE) {
            enqueue(node);
            return;
        }
        if (node.parentElement) {
            enqueue(node.parentElement);
        }
    });

    if (hasElements && !scheduled) {
        scheduled = true;
        requestAnimationFrame(flushScan);
    }
};

/**
 * 初始化扫描与 MutationObserver
 * @returns {void}
 */
const init = () => {
    const root = getRoot();
    scan(root);
    if (config.copyButton) {
        addFeedbackCopyButtons();
    }

    const observer = new MutationObserver((mutations) => {
        const nodesToScan = [];
        mutations.forEach((mutation) => {
            if (mutation.type === 'characterData') {
                if (mutation.target.parentElement) {
                    nodesToScan.push(mutation.target.parentElement);
                }
                return;
            }
            if (mutation.addedNodes.length > 0) {
                mutation.addedNodes.forEach((node) => nodesToScan.push(node));
            }
        });
        if (nodesToScan.length > 0) {
            scheduleScan(nodesToScan);
        }
    });

    observer.observe(root, { childList: true, subtree: true, characterData: true });
};

/**
 * 模块入口：接收配置并启动
 * @param {Object} userConfig - 用户配置
 * @returns {void}
 */
export const start = (userConfig = {}) => {
    // 合并用户配置
    config = { ...config, ...userConfig };

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
};
