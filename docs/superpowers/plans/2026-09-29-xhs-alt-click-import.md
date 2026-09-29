# 小红书 Alt+点击 导入 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在小红书页面 Alt+点击图片即可导入 Medix，绕过站点对右键菜单的拦截。

**Architecture:** 新增 `content.js`（capture 阶段监听 Alt+click，取 `<img>` 的 `currentSrc`，经 `chrome.runtime.sendMessage` 中转）；`background.js` 抽取共享 `importImage()` 并新增 `onMessage` 处理；`manifest.json` 注册 content script。本地服务无 CORS 头，故 content script 不直接 fetch，必须由 service worker 代发。

**Tech Stack:** Chrome Extension Manifest V3（原生 JS，无构建步骤）。

**Spec:** `docs/superpowers/specs/2026-09-29-xhs-alt-click-import-design.md`

---

## 背景知识（实现者必读）

- 插件位于 `extension/`，无打包流程——改完代码在 `chrome://extensions` 点「重新加载」即生效
- 现有导入链路：`background.js` 右键菜单 → `fetch http://localhost:{port}/api/import` → badge/通知反馈；port 存在 `chrome.storage.local`（默认 8765）
- 本地服务端（`src-tauri/src/server/mod.rs`）返回头中**没有** `Access-Control-Allow-Origin`，页面源（xiaohongshu.com）发出的 fetch 会被 CORS 拦截——这是必须经 background 中转的原因
- 小红书在 capture 阶段拦截右键（含 Shift+右键），`content.js` 的 click 监听必须也用 capture 阶段（`addEventListener` 第三参 `true`）才能先于站点逻辑执行
- 小红书图片懒加载：`<img>` 的 `src` 可能是占位图，实际显示地址在 `currentSrc`
- 插件无自动化测试基建（项目 Vitest 仅覆盖前端 React 组件），本功能以手工验证为准

---

### Task 1: 重构 `background.js` — 抽取共享导入函数 + 消息中转

**Files:**
- Modify: `extension/background.js`

- [ ] **Step 1: 用以下完整内容替换 `background.js` 中从 `chrome.contextMenus.onClicked` 到 `importImage` 相关逻辑的部分**

保留文件末尾的 `showSuccess` / `showError` / `getPort` 三个函数不动。改动点：
① 右键菜单 handler 简化为调用 `importImage`；② 新增 `chrome.runtime.onMessage` 监听器；
③ 把「POST + 反馈」逻辑抽成 `importImage(imageUrl, pageUrl)`。

替换后的顶部到 `showSuccess` 之前应为：

```js
const DEFAULT_PORT = 8765;

// Create context menu on install
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "add-to-medix",
    title: "添加到 Medix",
    contexts: ["image"],
  });
});

// Handle context menu click
chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId !== "add-to-medix") return;

  const imageUrl = info.srcUrl;
  const pageUrl = tab?.url || info.pageUrl || "";

  await importImage(imageUrl, pageUrl);
});

// Relay imports from content scripts (e.g. Xiaohongshu alt+click).
// The local Medix server has no CORS headers, so content scripts cannot
// fetch localhost directly — the service worker's fetch is not origin-bound.
chrome.runtime.onMessage.addListener((msg) => {
  if (msg?.type === "medix-import" && msg.url) {
    importImage(msg.url, msg.page_url || "");
  }
});

async function importImage(imageUrl, pageUrl) {
  const port = await getPort();

  // Brief badge feedback before the async request
  chrome.action.setBadgeText({ text: "..." });
  chrome.action.setBadgeBackgroundColor({ color: "#3b82f6" });

  try {
    const resp = await fetch(`http://localhost:${port}/api/import`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        url: imageUrl,
        page_url: pageUrl,
      }),
    });

    if (resp.ok) {
      const data = await resp.json();
      if (data.ok) {
        showSuccess();
      } else {
        showError(data.error || "未知错误");
      }
    } else {
      let msg = `HTTP ${resp.status}`;
      try { const body = await resp.json(); msg = body.error || msg; } catch {}
      showError(msg);
    }
  } catch (e) {
    showError(e.message);
  }
}
```

- [ ] **Step 2: 语法检查**

```bash
node --check extension/background.js
```

预期：无输出（语法正确）。

- [ ] **Step 3: 提交**

```bash
git add extension/background.js
git commit -m "refactor(extension): extract shared importImage and add message relay

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 2: 新增 `content.js` + 注册 content script

**Files:**
- Create: `extension/content.js`
- Modify: `extension/manifest.json`

- [ ] **Step 1: 创建 `extension/content.js`**

```js
// Alt+click any image on Xiaohongshu to send it to Medix.
// Xiaohongshu blocks the native context menu (even Shift+right-click),
// so the extension's menu item never appears there — this is the workaround.
// Capture phase so we run before the site's own handlers.

document.addEventListener(
  "click",
  (event) => {
    if (!event.altKey) return;

    const img = event.target.closest?.("img");
    if (!img) return; // not on an image — let the site handle it

    // Xiaohongshu lazy-loads: currentSrc holds the actually-displayed URL
    const url = img.currentSrc || img.src;
    if (!url || url.startsWith("data:")) return;

    event.preventDefault();
    event.stopPropagation();

    chrome.runtime.sendMessage({
      type: "medix-import",
      url,
      page_url: location.href,
    });
  },
  true
);
```

- [ ] **Step 2: 修改 `extension/manifest.json` — 注册 content script 并升级版本号**

```json
{
  "manifest_version": 3,
  "name": "Medix",
  "version": "0.1.1",
  "description": "一键添加图片到 Medix 媒体库",
  "permissions": ["contextMenus", "storage", "notifications"],
  "host_permissions": ["http://localhost:*/*"],
  "background": {
    "service_worker": "background.js"
  },
  "content_scripts": [
    {
      "matches": ["https://*.xiaohongshu.com/*"],
      "js": ["content.js"],
      "run_at": "document_idle"
    }
  ],
  "action": {
    "default_popup": "popup.html"
  }
}
```

- [ ] **Step 3: 语法检查**

```bash
node --check extension/content.js
node -e "JSON.parse(require('fs').readFileSync('extension/manifest.json', 'utf8')); console.log('manifest ok')"
```

预期：第一个无输出；第二个输出 `manifest ok`。

- [ ] **Step 4: 提交**

```bash
git add extension/content.js extension/manifest.json
git commit -m "feat(extension): alt+click image import on Xiaohongshu via content script

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 3: 手工端到端验证

- [ ] **Step 1: 确保 Medix 桌面端在运行（本地服务监听 8765 端口）**
- [ ] **Step 2: `chrome://extensions` → Medix → 点「重新加载」**
- [ ] **Step 3: 验证清单**

1. 打开小红书笔记详情页 → 按住 Alt 点击正文图片 → 扩展图标出现 ✓ badge，系统通知「图片已添加到 Medix」，Medix 出现新条目且来源显示「网页 · 小红书」
2. Alt+点击页面空白处 / 头像外的非图片元素 → 无任何行为，页面正常响应
3. Alt+点击小红书信息流缩略图 → 同样可导入（若缩略图是懒加载占位，`currentSrc` 应给出真实地址）
4. 其他网站（如 Twitter/X）→ 右键图片 →「添加到 Medix」仍正常工作
5. 关闭 Medix 桌面端后 Alt+点击 → 出现红色 ！ badge 和失败通知（报错路径正常）

- [ ] **Step 4: 若验证失败，对照 spec 排查后修正并重新验证（修正需另提交）**

无提交（验证任务）。
