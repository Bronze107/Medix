import { useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { Media } from "@/types/media";
import { mediaThumbnail } from "@/lib/tauri";

interface TableViewProps {
  media: Media[];
  selectedId: string | null;
  onSelect: (media: Media) => void;
  onDoubleClick?: (media: Media) => void;
  selectedIds: string[];
  selectionMode: boolean;
  onToggleSelect: (media: Media) => void;
}

function formatFileSize(bytes: number | null): string {
  if (bytes === null) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

const thumbCache = new Map<string, string>();

function useThumbnail(id: string | null) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!id) return;
    if (thumbCache.has(id)) {
      setUrl(thumbCache.get(id)!);
      return;
    }
    let cancelled = false;
    mediaThumbnail(id).then((path) => {
      if (!cancelled) {
        const src = convertFileSrc(path);
        thumbCache.set(id, src);
        setUrl(src);
      }
    });
    return () => { cancelled = true; };
  }, [id]);
  return url;
}

function TableView({
  media,
  selectedId,
  onSelect,
  onDoubleClick,
  selectedIds,
  selectionMode,
  onToggleSelect,
}: TableViewProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const rowHeight = 48;

  const virtualizer = useVirtualizer({
    count: media.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => rowHeight,
    overscan: 10,
  });

  const virtualItems = virtualizer.getVirtualItems();

  return (
    <div ref={parentRef} className="h-full overflow-auto">
      {/* Header */}
      <div
        className="sticky top-0 z-10 flex items-center gap-2 border-b border-[var(--color-border)] bg-[var(--color-bg-secondary)] px-3 py-2 text-[10px] font-medium text-[var(--color-text-muted)] uppercase"
      >
        <div className="w-8 shrink-0" />
        <div className="flex-1">文件</div>
        <div className="w-20 text-right">尺寸</div>
        <div className="w-20 text-right">文件大小</div>
        <div className="w-28 text-right">日期</div>
      </div>

      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: "100%",
          position: "relative",
        }}
      >
        {virtualItems.map((virtualRow) => {
          const item = media[virtualRow.index];
          if (!item) return null;

          return (
            <TableRow
              key={item.id}
              item={item}
              isSelected={item.id === selectedId}
              isMultiSelected={selectedIds.includes(item.id)}
              selectionMode={selectionMode}
              onClick={() => {
                if (selectionMode) {
                  onToggleSelect(item);
                } else {
                  onSelect(item);
                }
              }}
              onDoubleClick={
                onDoubleClick ? () => onDoubleClick(item) : undefined
              }
              onToggleSelect={() => onToggleSelect(item)}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                height: `${virtualRow.size}px`,
                transform: `translateY(${virtualRow.start}px)`,
              }}
            />
          );
        })}
      </div>
    </div>
  );
}

function TableRow({
  item,
  isSelected,
  isMultiSelected,
  selectionMode,
  onClick,
  onDoubleClick,
  onToggleSelect,
  style,
}: {
  item: Media;
  isSelected: boolean;
  isMultiSelected: boolean;
  selectionMode: boolean;
  onClick: () => void;
  onDoubleClick?: () => void;
  onToggleSelect: () => void;
  style: React.CSSProperties;
}) {
  const thumbUrl = useThumbnail(item.id);

  return (
    <div
      onClick={onClick}
      onDoubleClick={(e) => {
        if (!selectionMode && onDoubleClick) {
          e.stopPropagation();
          onDoubleClick();
        }
      }}
      style={style}
      className={`flex items-center gap-2 border-b border-[var(--color-border-light)] px-3 transition-colors ${
        isSelected
          ? "bg-blue-900/20"
          : isMultiSelected
          ? "bg-green-900/20"
          : "hover:bg-[var(--color-bg-hover)]"
      }`}
    >
      {/* Checkbox + thumbnail */}
      <div className="relative w-8 shrink-0">
        <div className="flex h-8 w-8 items-center justify-center overflow-hidden rounded bg-[var(--color-bg-tertiary)]">
          {thumbUrl ? (
            <img src={thumbUrl} alt="" className="h-full w-full object-cover" draggable={false} />
          ) : (
            <div className="h-4 w-4 text-[var(--color-text-muted)]" />
          )}
        </div>
        <div
          className={`absolute -left-1 -top-1 flex h-4 w-4 items-center justify-center rounded border transition-all ${
            isMultiSelected
              ? "border-green-500 bg-green-500 text-white opacity-100"
              : "border-[var(--color-border-light)] bg-[var(--color-bg-secondary)]/80 text-transparent opacity-0 hover:opacity-100"
          } ${selectionMode ? "opacity-100" : ""}`}
          onClick={(e) => { e.stopPropagation(); onToggleSelect(); }}
        >
          {isMultiSelected && (
            <svg className="h-2.5 w-2.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={3}>
              <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
            </svg>
          )}
        </div>
      </div>

      {/* File info */}
      <div className="flex-1 min-w-0">
        <p className="truncate text-xs text-[var(--color-text-secondary)]">
          {item.id.slice(0, 8)}
        </p>
      </div>

      {/* Dimensions */}
      <div className="w-20 text-right text-[11px] text-[var(--color-text-muted)] tabular-nums">
        {item.width ?? "?"}×{item.height ?? "?"}
      </div>

      {/* File size */}
      <div className="w-20 text-right text-[11px] text-[var(--color-text-muted)] tabular-nums">
        {formatFileSize(item.file_size)}
      </div>

      {/* Date */}
      <div className="w-28 text-right text-[11px] text-[var(--color-text-muted)]">
        {item.imported_at?.slice(0, 10) ?? "—"}
      </div>
    </div>
  );
}

export default TableView;
