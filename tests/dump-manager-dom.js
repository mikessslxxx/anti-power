/**
 * 导出 Manager 窗口 (Launchpad) 的 DOM 结构
 * 
 * 使用方法:
 * node dump-manager-dom.js "ws://127.0.0.1:9222/devtools/browser/xxx"
 */

const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');

async function main() {
    const wsUrl = process.argv[2];

    if (!wsUrl) {
        console.log('❌ 请提供 WebSocket URL 作为参数！');
        console.log('用法: node dump-manager-dom.js "ws://127.0.0.1:9222/devtools/browser/xxx"');
        process.exit(1);
    }

    console.log(`🔗 正在连接到: ${wsUrl}\n`);

    try {
        const browser = await chromium.connectOverCDP(wsUrl);
        console.log('✅ 成功连接!\n');

        const contexts = browser.contexts();
        let managerPage = null;

        // 查找 Manager 窗口（优先匹配标题为 "Manager" 的页面）
        let fallbackPage = null;
        for (const context of contexts) {
            for (const page of context.pages()) {
                const url = page.url();
                const title = await page.title();

                // 优先选择标题为 "Manager" 的页面
                if (title === 'Manager') {
                    managerPage = page;
                    console.log(`🎯 找到 Manager 窗口: ${title}`);
                    break;
                }
                // 作为备选，记录第一个 jetski-agent 页面
                if (url.includes('workbench-jetski-agent.html') && !fallbackPage) {
                    fallbackPage = page;
                }
            }
            if (managerPage) break;
        }

        // 如果没找到 Manager，使用备选页面
        if (!managerPage && fallbackPage) {
            managerPage = fallbackPage;
            console.log(`⚠️ 未找到标题为 "Manager" 的页面，使用备选: ${await fallbackPage.title()}`);
        }

        if (!managerPage) {
            console.log('❌ 未找到 Manager 窗口，请确保已打开 Manager/Launchpad');
            await browser.close();
            process.exit(1);
        }

        console.log('\n📦 正在导出 DOM 结构...\n');

        // 确保 temp 目录存在
        const tempDir = path.join(__dirname, 'temp');
        if (!fs.existsSync(tempDir)) {
            fs.mkdirSync(tempDir, { recursive: true });
        }

        // 导出完整 HTML
        const fullHtml = await managerPage.content();
        const htmlPath = path.join(tempDir, 'manager-dom-full.html');
        fs.writeFileSync(htmlPath, fullHtml, 'utf-8');
        console.log(`✅ 完整 HTML 已保存到: ${htmlPath}`);

        // 导出 DOM 树结构（简化版，方便阅读）
        const domTree = await managerPage.evaluate(() => {
            function getNodeInfo(element, depth = 0) {
                if (depth > 6) return null; // 限制深度

                const info = {
                    tag: element.tagName?.toLowerCase() || '#text',
                    id: element.id || undefined,
                    class: element.className || undefined,
                };

                // 只获取有意义的属性
                if (element.getAttribute) {
                    const role = element.getAttribute('role');
                    const dataTestid = element.getAttribute('data-testid');
                    if (role) info.role = role;
                    if (dataTestid) info.testId = dataTestid;
                }

                // 递归获取子元素
                if (element.children && element.children.length > 0) {
                    info.children = Array.from(element.children)
                        .map(child => getNodeInfo(child, depth + 1))
                        .filter(Boolean);
                }

                return info;
            }

            return getNodeInfo(document.body);
        });

        const treePath = path.join(tempDir, 'manager-dom-tree.json');
        fs.writeFileSync(treePath, JSON.stringify(domTree, null, 2), 'utf-8');
        console.log(`✅ DOM 树结构已保存到: ${treePath}`);

        // 提取所有有 ID 或特定 class 的重要元素
        const keyElements = await managerPage.evaluate(() => {
            const elements = document.querySelectorAll('[id], [class*="cascade"], [class*="agent"], [class*="panel"], [class*="chat"]');
            return Array.from(elements).slice(0, 100).map(el => ({
                tag: el.tagName.toLowerCase(),
                id: el.id || undefined,
                class: el.className?.split(' ').filter(c => c).slice(0, 5).join(' ') || undefined,
                text: el.textContent?.slice(0, 50).trim() || undefined
            }));
        });

        const elementsPath = path.join(tempDir, 'manager-key-elements.json');
        fs.writeFileSync(elementsPath, JSON.stringify(keyElements, null, 2), 'utf-8');
        console.log(`✅ 关键元素列表已保存到: ${elementsPath}`);

        console.log('\n🎉 导出完成! 你可以查看以下文件:');
        console.log(`   - manager-dom-full.html    (完整 HTML)`.padEnd(50));
        console.log(`   - manager-dom-tree.json    (DOM 树结构)`);
        console.log(`   - manager-key-elements.json (关键元素列表)`);

        await browser.close();

    } catch (error) {
        console.error('❌ 错误:', error.message);
    }
}

main();
