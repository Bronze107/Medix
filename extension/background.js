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
  const altText = info.selectionText || "";

  const port = await getPort();

  try {
    const resp = await fetch(`http://localhost:${port}/api/import`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        url: imageUrl,
        page_url: pageUrl,
        alt_text: altText,
      }),
    });

    if (resp.ok) {
      console.log("[Medix] image queued:", imageUrl);
    } else {
      console.error("[Medix] import failed:", await resp.text());
    }
  } catch (e) {
    console.error("[Medix] connection failed:", e.message);
  }
});

async function getPort() {
  const result = await chrome.storage.local.get("port");
  return result.port || DEFAULT_PORT;
}
