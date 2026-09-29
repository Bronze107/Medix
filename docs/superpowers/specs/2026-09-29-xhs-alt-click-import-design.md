# 设计：小红书 Alt+点击 导入图片

> 日期: 2026-09-29
> 状态: 已批准

## 背景

Medix 浏览器插件通过图片右键菜单导入，但小红书在笔记页拦截了 `contextmenu`
（含 Shift+右键，capture 阶段监听），扩展的原生菜单项无法出现。

## 已确认的设计决策

1. 触发方式：**Alt + 点击图片**（用户已确认，备选的悬浮按钮/弹窗批量收集被否决）
2. 范围：仅 `*.xiaohongshu.com` 注入 content script，其他网站零影响
3. 导入内容：页面 `<img>` 的 `currentSrc`（所见即所得，含站点水印；不去水印、不解析笔记 API）

## 方案

### 1. 新增 `extension/content.js`

- capture 阶段监听 `click`：命中 `event.altKey === true` 且点击目标为/包含 `<img>` 时触发
- 取图：`target.closest('img')` → `currentSrc`（小红书懒加载，`currentSrc` 比 `src` 可靠）；
  找不到 `<img>` 时不拦截，不触发任何行为
- 命中后 `preventDefault()` + `stopPropagation()`（阻止小红书自定义菜单/跳转），
  `chrome.runtime.sendMessage({ type: "medix-import", url, page_url })` 交给 background
- 不直接 fetch `localhost`：本地服务（`src-tauri/src/server/mod.rs`）无 CORS 头，
  页面源的跨域请求会被浏览器拦截，必须经 service worker 中转（其 fetch 受
  `host_permissions: http://localhost:*/*` 保护，不受 CORS 约束）

### 2. 修改 `extension/background.js`

- 抽取共享函数 `importImage(url, pageUrl)`：现有「POST /api/import → badge/通知反馈」逻辑
- 右键菜单 handler 与新增的 `chrome.runtime.onMessage`（处理 `medix-import`）共用该函数
- 右键菜单行为不变

### 3. 修改 `extension/manifest.json`

```json
"content_scripts": [{
  "matches": ["https://*.xiaohongshu.com/*"],
  "js": ["content.js"],
  "run_at": "document_idle"
}]
```

- `host_permissions` 保持 `http://localhost:*/*` 不变
- 版本 0.1.0 → 0.1.1

### 4. 已知限制

- CSS `background-image` 形式的图拿不到（笔记正文图是 `<img>`，不受影响）
- 导入图片带站点水印

## 验证

手工验证（插件无自动化测试基建）：

1. `chrome://extensions` 重新加载插件
2. 打开小红书笔记页 → Alt+点击图片 → Medix 出现新条目，来源显示「网页 · 小红书」
3. Alt+点击页面空白处/非图片元素 → 无任何行为
4. 其他网站（如 Twitter）右键菜单导入仍正常
