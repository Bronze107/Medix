import { useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { Media } from "@/types/media";
import { mediaThumbnail } from "@/lib/tauri";

type SortField = "imported_at" | "created_at" | "modified_at" | "file_size" | "width" | "height";

interface GroupInfo {
  label: string;
  startIndex: number;
  count: number;
}

interface TableViewProps {
  media: Media[];
  selectedId: string | null;
  onSelect: (media: Media) => void;
  onDoubleClick?: (media: Media) => void;
  onContextMenu?: (e: React.MouseEvent, media: Media) => void;
  groups?: GroupInfo[];
  sortBy: SortField;
  descending: boolean;
  onSortChange: (field: SortField) => void;
  selectedIds: string[];
  selectionMode: boolean;
  onToggleSelect: (media: Media, index: number, shiftKey: boolean) => void;
  onAddToCollection?: (media: Media) => void;
  onDelete?: (media: Media) => void;
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
  onContextMenu,
  groups,
  sortBy,
  descending,
  onSortChange,
  selectedIds,
  selectionMode,
  onToggleSelect,
  onAddToCollection,
  onDelete,
}: TableViewProps) {

  const sortArrow = (field: SortField) => {
    if (sortBy !== field) return null;
    return descending ? " ↓" : " ↑";
  };

  // Build group row positions
  const groupRows: { groupIndex: number; virtualRow: number }[] = [];
  if (groups && groups.length > 0) {
    let offset = 0;
    for (let gi = 0; gi < groups.length; gi++) {
      groupRows.push({ groupIndex: gi, virtualRow: groups[gi].startIndex + offset });
      offset++;
    }
  }
  const groupRowSet = new Set(groupRows.map((g) => g.virtualRow));
  const totalCount = media.length + groupRows.length;

  const rowHeight = 48;

  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: totalCount,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => groupRowSet.has(index) ? 36 : rowHeight,
    overscan: 10,
  });

  const virtualItems = virtualizer.getVirtualItems();

  return (
    <div ref={parentRef} className="h-full overflow-auto">
      {/* Header */}
      <div
        className="sticky top-0 z-10 flex items-center gap-2 border-b border-[var(--color-border)] bg-[var(--color-bg-secondary)] px-3 py-2 text-[10px] font-medium uppercase"
      >
        <div className="w-8 shrink-0" />
        <div className="flex-1 text-[var(--color-text-muted)]">文件</div>
        <button onClick={() => onSortChange("width")} className={`w-20 text-right hover:text-[var(--color-text-primary)] ${sortBy === "width" || sortBy === "height" ? "text-[var(--color-text-primary)]" : "text-[var(--color-text-muted)]"}`}>尺寸{sortBy === "width" ? sortArrow("width") : sortBy === "height" ? sortArrow("height") : ""}</button>
        <button onClick={() => onSortChange("file_size")} className={`w-20 text-right hover:text-[var(--color-text-primary)] ${sortBy === "file_size" ? "text-[var(--color-text-primary)]" : "text-[var(--color-text-muted)]"}`}>大小{sortArrow("file_size")}</button>
        <button onClick={() => onSortChange("imported_at")} className={`w-28 text-right hover:text-[var(--color-text-primary)] ${sortBy === "imported_at" ? "text-[var(--color-text-primary)]" : "text-[var(--color-text-muted)]"}`}>日期{sortArrow("imported_at")}</button>
        {(onAddToCollection || onDelete) && <div className="w-16 shrink-0" />}
      </div>

      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: "100%",
          position: "relative",
        }}
      >
        {virtualItems.map((virtualRow) => {
          // Group header row
          const groupEntry = groupRows.find((gr) => gr.virtualRow === virtualRow.index);
          if (groupEntry && groups) {
            const g = groups[groupEntry.groupIndex];
            return (
              <div
                key={`group-${g.label}`}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  height: `${virtualRow.size}px`,
                  transform: `translateY(${virtualRow.start}px)`,
                }}
                className="flex items-end border-b border-[var(--color-border)] bg-[var(--color-bg-secondary)]/90 backdrop-blur px-3 pb-2"
              >
                <span className="text-[11px] font-medium text-[var(--color-text-secondary)]">{g.label}</span>
                <span className="ml-2 text-[10px] text-[var(--color-text-muted)]">{g.count} 张</span>
              </div>
            );
          }

          // Adjust index for media rows (skip group rows)
          let mediaIndex = virtualRow.index;
          for (const gr of groupRows) { if (gr.virtualRow < virtualRow.index) mediaIndex--; }
          const item = media[mediaIndex];
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
                  onToggleSelect(item, mediaIndex, false);
                } else {
                  onSelect(item);
                }
              }}
              onDoubleClick={
                onDoubleClick ? () => onDoubleClick(item) : undefined
              }
              onContextMenu={onContextMenu ? (e: React.MouseEvent) => onContextMenu(e, item) : undefined}
              onToggleSelect={(shiftKey: boolean) => onToggleSelect(item, mediaIndex, shiftKey)}
              onAddToCollection={onAddToCollection ? () => onAddToCollection(item) : undefined}
              onDelete={onDelete ? () => onDelete(item) : undefined}
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
  onContextMenu,
  onToggleSelect,
  onAddToCollection,
  onDelete,
  style,
}: {
  item: Media;
  isSelected: boolean;
  isMultiSelected: boolean;
  selectionMode: boolean;
  onClick: () => void;
  onDoubleClick?: () => void;
  onContextMenu?: (e: React.MouseEvent) => void;
  onToggleSelect: (shiftKey: boolean) => void;
  onAddToCollection?: () => void;
  onDelete?: () => void;
  style: React.CSSProperties;
}) {
  const thumbUrl = useThumbnail(item.id);

  return (
    <div
      data-media-card
      onClick={onClick}
      onDoubleClick={(e) => {
        if (!selectionMode && onDoubleClick) {
          e.stopPropagation();
          onDoubleClick();
        }
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu?.(e);
      }}
      style={style}
      className={`group flex items-center gap-2 border-b border-[var(--color-border-light)] px-3 transition-colors ${
        isSelected
          ? "bg-[var(--color-accent-soft)]"
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
          onClick={(e: React.MouseEvent) => { e.stopPropagation(); onToggleSelect(e.shiftKey); }}
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

      {/* Hover actions */}
      {(onAddToCollection || onDelete) && (
        <div className="flex w-16 shrink-0 items-center justify-end gap-0.5 opacity-0 transition-all translate-x-2 group-hover:opacity-100 group-hover:translate-x-0">
          {onAddToCollection && (
            <button
              onClick={(e) => { e.stopPropagation(); onAddToCollection(); }}
              className="rounded p-1 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
              title="添加到集合"
            >
              <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
              </svg>
            </button>
          )}
          {onDelete && (
            <button
              onClick={(e) => { e.stopPropagation(); onDelete(); }}
              className="rounded p-1 text-[var(--color-text-muted)] hover:bg-[var(--color-danger-soft)] hover:text-[var(--color-danger)] transition-colors"
              title="删除"
            >
              <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
              </svg>
            </button>
          )}
        </div>
      )}
    </div>
  );
}

export default TableView;
