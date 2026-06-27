import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { BrowseItem } from "@/types/browse";

function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  if (seconds >= 600) return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

type SortField = "imported_at" | "created_at" | "modified_at" | "file_size" | "width" | "height";

interface GroupInfo {
  label: string;
  startIndex: number;
  count: number;
}

interface TableViewProps {
  media: BrowseItem[];
  selectedId: string | null;
  onSelect: (media: BrowseItem) => void;
  onDoubleClick?: (media: BrowseItem) => void;
  onContextMenu?: (e: React.MouseEvent, media: BrowseItem) => void;
  groups?: GroupInfo[];
  sortBy: SortField;
  descending: boolean;
  onSortChange: (field: SortField) => void;
  selectedIds: string[];
  onToggleSelect: (media: BrowseItem, index: number, shiftKey: boolean) => void;
  onAddToCollection?: (media: BrowseItem) => void;
  onDelete?: (media: BrowseItem) => void;
}

function formatFileSize(bytes: number | null): string {
  if (bytes === null) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
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
    estimateSize: (index) => groupRowSet.has(index) ? 28 : rowHeight,
    overscan: 10,
  });

  const virtualItems = virtualizer.getVirtualItems();

  return (
    <div ref={parentRef} className="h-full overflow-y-auto overflow-x-hidden">
      {/* Header */}
      <div
        className="sticky top-0 z-10 flex items-center gap-2 border-b border-[var(--color-border)] bg-[var(--color-bg-secondary)] px-3 py-2 text-[10px] font-medium uppercase"
      >
        <div className="w-8 shrink-0" />
        <div className="flex-1 text-[var(--color-text-muted)]">文件</div>
        <button onClick={() => onSortChange("width")} className={`w-24 text-right hover:text-[var(--color-text-primary)] ${sortBy === "width" || sortBy === "height" ? "text-[var(--color-text-primary)]" : "text-[var(--color-text-muted)]"}`}>类型/时长{sortBy === "width" ? sortArrow("width") : sortBy === "height" ? sortArrow("height") : ""}</button>
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
                className="flex items-center gap-3 px-3"
              >
                <span className="text-xs font-semibold text-[var(--color-text-secondary)]">{g.label}</span>
                <div className="flex-1 h-px bg-[var(--color-border)]" />
                <span className="text-[11px] text-[var(--color-text-muted)] tabular-nums">{g.count}</span>
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
              key={item.item_id}
              item={item}
              isSelected={item.item_id === selectedId}
              isMultiSelected={selectedIds.includes(item.item_id)}
              onClick={() => onSelect(item)}
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
  onClick,
  onDoubleClick,
  onContextMenu,
  onToggleSelect,
  onAddToCollection,
  onDelete,
  style,
}: {
  item: BrowseItem;
  isSelected: boolean;
  isMultiSelected: boolean;
  onClick: () => void;
  onDoubleClick?: () => void;
  onContextMenu?: (e: React.MouseEvent) => void;
  onToggleSelect: (shiftKey: boolean) => void;
  onAddToCollection?: () => void;
  onDelete?: () => void;
  style: React.CSSProperties;
}) {
  const thumbUrl = item.thumb_256
    ? convertFileSrc(item.thumb_256)
    : item.source_path && !item.source_path.startsWith("http")
      ? convertFileSrc(item.source_path)
      : null;

  return (
    <div
      data-media-card
      onClick={(e) => {
        if (e.ctrlKey || e.metaKey || e.shiftKey) {
          onToggleSelect(e.shiftKey);
        } else {
          onClick();
        }
      }}
      onDoubleClick={(e) => {
        if (onDoubleClick) {
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
        <div className="relative flex h-8 w-8 items-center justify-center overflow-hidden rounded bg-[var(--color-bg-tertiary)]">
          {/* LQIP placeholder */}
          {item.lqip && (
            <img src={item.lqip} alt="" className="absolute inset-0 h-full w-full object-cover blur-sm scale-110" draggable={false} />
          )}
          {thumbUrl ? (
            <img src={thumbUrl} alt="" className="relative h-full w-full object-cover" draggable={false} decoding="async" />
          ) : (
            <div className="h-4 w-4 text-[var(--color-text-muted)]" />
          )}
        </div>
        <div
          className={`absolute -left-1 -top-1 flex h-4 w-4 items-center justify-center rounded border transition-all opacity-0 group-hover:opacity-100 ${
            isMultiSelected
              ? "border-[var(--color-accent)] bg-[var(--color-accent)] text-white opacity-100"
              : "border-[var(--color-text-muted)]/40 bg-[var(--color-bg-secondary)]/80 text-transparent hover:border-[var(--color-text-secondary)]"
          }`}
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
          {item.item_id.slice(0, 8)}
        </p>
      </div>

      {/* Kind */}
      <div className="w-14 text-center text-[11px]">
        {item.item_kind === "variant" ? (
          <span className={item.is_display_variant ? "text-[var(--color-accent)]" : "text-[var(--color-text-muted)]"}>
            {item.is_display_variant ? "展示" : "版本"}
          </span>
        ) : (
          <span className="text-[var(--color-text-muted)]/40">原图</span>
        )}
      </div>

      {/* Dimensions */}
      <div className="w-24 text-right text-[11px] text-[var(--color-text-muted)] tabular-nums">
        {item.media_type === "video" ? (
          <span>
            {item.duration != null ? `${formatDuration(item.duration)} · ` : ""}
            {item.height != null ? `${item.height}p` : "?"}
          </span>
        ) : (
          <span>
            {item.width ?? "?"}×{item.height ?? "?"}
          </span>
        )}
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
