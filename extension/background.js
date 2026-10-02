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

// Relay imports from content scripts (Alt+click, any site).
// The local Medix server has no CORS headers, so content scripts cannot
// fetch localhost directly — the service worker's fetch is not origin-bound.
chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg?.type !== "medix-import" || !msg.url) return;
  console.log("[Medix] background received import request", msg.url);
  importImage(msg.url, msg.page_url || "")
    .then(sendResponse)
    .catch((e) => sendResponse({ ok: false, error: String(e) }));
  // Return true to answer asynchronously. Besides giving the content script
  // the real result, holding the channel open keeps the MV3 service worker
  // alive for the duration of the download.
  return true;
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
        return { ok: true };
      }
      const err = data.error || "未知错误";
      showError(err);
      return { ok: false, error: err };
    }

    let msg = `HTTP ${resp.status}`;
    try { const body = await resp.json(); msg = body.error || msg; } catch {}
    showError(msg);
    return { ok: false, error: msg };
  } catch (e) {
    showError(e.message);
    return { ok: false, error: e.message };
  }
}

function showSuccess() {
  // Badge on extension icon
  chrome.action.setBadgeText({ text: "✓" });
  chrome.action.setBadgeBackgroundColor({ color: "#22c55e" });
  setTimeout(() => chrome.action.setBadgeText({ text: "" }), 2500);

  console.log("[Medix] import queued successfully");
  chrome.notifications.create(`medix-import-${Date.now()}`, {
    type: "basic",
    title: "Medix",
    message: "图片已添加到 Medix",
    priority: 0,
    eventTime: Date.now(),
  });
}

function showError(msg) {
  chrome.action.setBadgeText({ text: "!" });
  chrome.action.setBadgeBackgroundColor({ color: "#ef4444" });
  setTimeout(() => chrome.action.setBadgeText({ text: "" }), 3500);

  console.error("[Medix] import failed:", msg);
  chrome.notifications.create(`medix-error-${Date.now()}`, {
    type: "basic",
    title: "Medix — 添加失败",
    message: msg,
    priority: 0,
    eventTime: Date.now(),
  });
}

async function getPort() {
  const result = await chrome.storage.local.get("port");
  return result.port || DEFAULT_PORT;
}
