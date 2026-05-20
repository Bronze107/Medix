import { getCurrentWindow } from "@tauri-apps/api/window";
import { WindowControls } from "./WindowControls";

export function TitleBar() {
  const handleDoubleClick = () => {
    getCurrentWindow().toggleMaximize();
  };

  return (
    <div
      data-tauri-drag-region
      onDoubleClick={handleDoubleClick}
      className="flex h-10 items-center bg-[var(--color-bg-secondary)] border-b border-[var(--color-border)] select-none"
      style={{ WebkitAppRegion: "drag" } as React.CSSProperties}
    >
      <div className="flex h-full flex-1 items-center gap-2 pl-4 cursor-default">
        <div className="h-5 w-5 rounded bg-blue-600" />
        <span className="text-sm font-semibold text-[var(--color-text-primary)]">
          Medix
        </span>
      </div>
      <div style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}>
        <WindowControls />
      </div>
    </div>
  );
}
