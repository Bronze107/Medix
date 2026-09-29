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
