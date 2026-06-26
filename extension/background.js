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
});

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
