// Alt+click any image to send it to Medix.
//
// This started as a Xiaohongshu-only workaround (that site swallows the native
// context menu, even Shift+right-click, so the extension's menu item never
// appears there). The manifest now injects it on every site, so keep the logic
// site-agnostic — it works with whatever <img> the page is actually showing.
//
// Capture phase so we run before the site's own handlers.

// Wrapped in an IIFE so a second injection cannot throw a redeclaration error
// — that would fail exactly as silently as the bugs this file is trying to
// make visible.
(() => {
  const TAG = "[Medix]";

  // Flip to true to bring back the verbose tracing — it is the quickest way to
  // tell "content script never injected" apart from "injected but missed the
  // image" when a new site misbehaves. Warnings and errors stay on either way.
  const DEBUG = false;
  const log = DEBUG ? (...args) => console.log(...args) : () => {};

  log(`${TAG} content script ready on`, location.href);

  /**
   * Find the <img> under the cursor.
   *
   * event.target alone is not enough: many sites — Instagram included — cover
   * the photo with a transparent <div> that owns the click, so the target is
   * that div and target.closest("img") finds nothing (closest only walks up,
   * and the overlay is a sibling of the image, not its descendant).
   */
  function findImageAt(event) {
    const direct = event.target?.closest?.("img");
    if (direct) return direct;

    const x = event.clientX;
    const y = event.clientY;

    // Everything under the pointer, topmost first — this still reaches the
    // image that the overlay is painted on top of.
    for (const el of document.elementsFromPoint(x, y)) {
      if (el instanceof HTMLImageElement) return el;
    }

    // Last resort: the smallest <img> whose box contains the point. Works even
    // when the image is not hit-testable at all (pointer-events: none).
    let best = null;
    let bestArea = Infinity;
    for (const img of document.images) {
      const r = img.getBoundingClientRect();
      if (r.width <= 0 || r.height <= 0) continue;
      if (x < r.left || x > r.right || y < r.top || y > r.bottom) continue;
      const area = r.width * r.height;
      if (area < bestArea) {
        bestArea = area;
        best = img;
      }
    }
    return best;
  }

  document.addEventListener(
    "click",
    (event) => {
      if (!event.altKey) return;

      const img = findImageAt(event);
      if (!img) {
        // Nothing image-like under the cursor. Dump the hit-test stack so a
        // genuinely unsupported page can be diagnosed instead of guessed at.
        const stack = document
          .elementsFromPoint(event.clientX, event.clientY)
          .map((el) => el.tagName.toLowerCase() + (el.className ? "." + String(el.className).split(" ")[0] : ""));
        log(`${TAG} alt+click, but no <img> under the cursor. Stack:`, stack);
        return;
      }

      if (img !== event.target?.closest?.("img")) {
        log(`${TAG} recovered <img> from behind an overlay`, img);
      }

      // Lazy-loading sites (Xiaohongshu, Instagram, ...) keep the URL that is
      // actually on screen in currentSrc; src can hold a placeholder.
      const url = img.currentSrc || img.src;
      if (!url || url.startsWith("data:")) {
        console.warn(`${TAG} alt+click on an <img> with no usable URL`, img);
        return;
      }

      event.preventDefault();
      event.stopPropagation();

      log(`${TAG} importing`, url);

      // Two failure modes used to be completely silent here:
      //  - sendMessage can throw when this page's extension context has been
      //    invalidated (the extension was reloaded/updated since page load);
      //  - it can call back with chrome.runtime.lastError when the background
      //    service worker is unreachable.
      try {
        chrome.runtime.sendMessage(
          { type: "medix-import", url, page_url: location.href },
          (response) => {
            const err = chrome.runtime.lastError;
            if (err) {
              console.error(`${TAG} sendMessage failed: ${err.message}`);
              return;
            }
            log(`${TAG} delivered to background`, response);
          }
        );
      } catch (e) {
        console.error(`${TAG} sendMessage threw (reload this page):`, e);
      }
    },
    true
  );
})();
