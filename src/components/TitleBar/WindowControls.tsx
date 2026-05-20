import { useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

const win = getCurrentWindow();

function MinimizeIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
      <rect x="2" y="9.5" width="8" height="1" fill="currentColor" rx="0.5" />
    </svg>
  );
}

function MaximizeIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
      <rect x="2" y="2" width="8" height="8" rx="1" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}

function RestoreIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
      <rect x="2" y="4" width="6.5" height="6.5" rx="0.75" stroke="currentColor" strokeWidth="1" />
      <rect x="4" y="2" width="6.5" height="6.5" rx="0.75" fill="var(--color-bg-secondary)" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}

function CloseIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
      <path d="M3 3L9 9M9 3L3 9" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
    </svg>
  );
}

const btnBase =
  "flex h-full w-[46px] items-center justify-center text-[var(--color-text-secondary)] transition-colors";

export function WindowControls() {
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    (async () => {
      const max = await win.isMaximized();
      if (!cancelled) setIsMaximized(max);

      const fn = await win.onResized(() => {
        win.isMaximized().then((m) => {
          if (!cancelled) setIsMaximized(m);
        });
      });
      if (!cancelled) unlisten = fn;
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return (
    <div className="flex h-full">
      <button
        onClick={() => win.minimize()}
        className={`${btnBase} hover:bg-[var(--color-bg-tertiary)]`}
        aria-label="最小化"
      >
        <MinimizeIcon />
      </button>
      <button
        onClick={() => win.toggleMaximize()}
        className={`${btnBase} hover:bg-[var(--color-bg-tertiary)]`}
        aria-label={isMaximized ? "还原" : "最大化"}
      >
        {isMaximized ? <RestoreIcon /> : <MaximizeIcon />}
      </button>
      <button
        onClick={() => win.close()}
        className={`${btnBase} hover:bg-[#c42b1c] hover:text-white`}
        aria-label="关闭"
      >
        <CloseIcon />
      </button>
    </div>
  );
}
