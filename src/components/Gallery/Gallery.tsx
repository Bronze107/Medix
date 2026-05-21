import { useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { Media } from "@/types/media";
import { useThumbnail } from "@/hooks/useThumbnail";

interface GroupInfo {
  label: string;
  startIndex: number;
  count: number;
}

interface GalleryProps {
  media: Media[];
  selectedId: string | null;
  onSelect: (media: Media) => void;
  onDoubleClick?: (media: Media) => void;
  onContextMenu?: (e: React.MouseEvent, media: Media) => void;
  groups?: GroupInfo[];
  selectedIds: string[];
  selectionMode: boolean;
  onToggleSelect: (media: Media, index: number, shiftKey: boolean) => void;
  columnCount?: number;
  gap?: number;
}

function Gallery({
  media,
  selectedId,
  onSelect,
  onDoubleClick,
  onContextMenu,
  groups,
  selectedIds,
  selectionMode,
  onToggleSelect,
  columnCount = 5,
  gap = 12,
}: GalleryProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const rowCount = Math.ceil(media.length / columnCount);

  // Build group row positions for virtual scroll
  const groupRows: { groupIndex: number; virtualRow: number }[] = [];
  if (groups && groups.length > 0) {
    let offset = 0;
    for (let gi = 0; gi < groups.length; gi++) {
      const g = groups[gi];
      const mediaRow = Math.floor(g.startIndex / columnCount);
      groupRows.push({ groupIndex: gi, virtualRow: mediaRow + offset });
      offset++;
    }
  }
  const totalRowCount = rowCount + groupRows.length;
  const groupRowSet = new Set(groupRows.map((g) => g.virtualRow));

  const virtualizer = useVirtualizer({
    count: totalRowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => groupRowSet.has(index) ? 48 : 240,
    overscan: 3,
  });

  const virtualItems = virtualizer.getVirtualItems();

  return (
    <div ref={parentRef} className="h-full overflow-auto">
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: "100%",
          position: "relative",
        }}
      >
        {virtualItems.map((virtualRow) => {
          // Check if this is a group header row
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
                className="flex items-end border-b border-[var(--color-border)] bg-[var(--color-bg-secondary)]/90 backdrop-blur px-4 pb-3"
              >
                <span className="text-[11px] font-medium text-[var(--color-text-secondary)]">{g.label}</span>
                <span className="ml-2 text-[10px] text-[var(--color-text-muted)]">{g.count} 张</span>
              </div>
            );
          }

          // Regular media row — adjust rowIndex to skip group headers
          let rowIndex = virtualRow.index;
          for (const gr of groupRows) { if (gr.virtualRow < virtualRow.index) rowIndex--; }

          const startIndex = rowIndex * columnCount;
          const rowMedia = media.slice(startIndex, startIndex + columnCount);

          return (
            <div
              key={virtualRow.key}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                height: `${virtualRow.size}px`,
                transform: `translateY(${virtualRow.start}px)`,
                display: "grid",
                gridTemplateColumns: `repeat(${columnCount}, 1fr)`,
                gap: `${gap}px`,
                padding: `0 ${gap / 2}px`,
              }}
            >
              {rowMedia.map((item, colIdx) => {
                const absIndex = startIndex + colIdx;
                return (
                <ThumbnailCard
                  key={item.id}
                  item={item}
                  isSelected={item.id === selectedId}
                  isMultiSelected={selectedIds.includes(item.id)}
                  selectionMode={selectionMode}
                  onClick={() => onSelect(item)}
                  onDoubleClick={onDoubleClick ? () => onDoubleClick(item) : undefined}
                  onContextMenu={onContextMenu ? (e: React.MouseEvent) => onContextMenu(e, item) : undefined}
                  onToggleSelect={(shiftKey: boolean) => onToggleSelect(item, absIndex, shiftKey)}
                />
              )})}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function ThumbnailCard({
  item,
  isSelected,
  isMultiSelected,
  selectionMode,
  onClick,
  onDoubleClick,
  onContextMenu,
  onToggleSelect,
}: {
  item: Media;
  isSelected: boolean;
  isMultiSelected: boolean;
  selectionMode: boolean;
  onClick: () => void;
  onDoubleClick?: () => void;
  onContextMenu?: (e: React.MouseEvent) => void;
  onToggleSelect: (shiftKey: boolean) => void;
}) {
  const thumbUrl = useThumbnail(item.id);
  const [loaded, setLoaded] = useState(false);

  const handleCardClick = (e: React.MouseEvent) => {
    if (selectionMode) {
      onToggleSelect(e.shiftKey);
    } else {
      onClick();
    }
  };

  return (
    <div
      data-media-card
      onClick={handleCardClick}
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
      className={`group relative flex cursor-pointer flex-col overflow-hidden rounded-xl transition-all duration-200 ease-out ${
        isSelected
          ? "ring-2 ring-[var(--color-accent)] ring-offset-2 ring-offset-[var(--color-bg-primary)] bg-[var(--color-bg-elevated)]"
          : isMultiSelected
          ? "ring-2 ring-[var(--color-success)] ring-offset-2 ring-offset-[var(--color-bg-primary)] bg-[var(--color-bg-elevated)]"
          : "bg-[var(--color-bg-elevated)] shadow-sm hover:shadow-lg hover:shadow-black/20 hover:-translate-y-0.5"
      }`}
    >
      {/* Checkbox – top-right, blurred frosted glass */}
      <div
        className={`absolute right-2 top-2 z-10 transition-all duration-150 ${
          selectionMode ? "opacity-100" : "opacity-0 group-hover:opacity-100"
        }`}
        onClick={(e: React.MouseEvent) => {
          e.stopPropagation();
          onToggleSelect(e.shiftKey);
        }}
      >
        <div
          className={`flex h-6 w-6 items-center justify-center rounded-md border-2 backdrop-blur-sm transition-all ${
            isMultiSelected
              ? "border-[var(--color-accent)] bg-[var(--color-accent)] text-white"
              : "border-white/70 bg-white/15 text-transparent hover:border-white"
          }`}
        >
          <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={3}>
            <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
          </svg>
        </div>
      </div>

      {/* Image area */}
      <div className="relative flex flex-1 items-center justify-center overflow-hidden bg-[var(--color-bg-tertiary)]/30">
        {thumbUrl ? (
          <img
            src={thumbUrl}
            alt=""
            loading="lazy"
            className={`h-full w-full object-cover transition-all duration-500 ease-out group-hover:scale-105 ${
              loaded ? "opacity-100 scale-100 blur-0" : "opacity-0 scale-95 blur-sm"
            }`}
            draggable={false}
            onLoad={() => setLoaded(true)}
          />
        ) : (
          <div className="flex flex-col items-center gap-1 p-2 text-[var(--color-text-muted)]">
            <svg className="h-8 w-8" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1}>
              <path strokeLinecap="round" strokeLinejoin="round" d="m2.25 15.75 5.159-5.159a2.25 2.25 0 0 1 3.182 0l5.159 5.159m-1.5-1.5 1.409-1.409a2.25 2.25 0 0 1 3.182 0l2.909 2.909M3.75 21h16.5a2.25 2.25 0 0 0 2.25-2.25V5.25A2.25 2.25 0 0 0 20.25 3H3.75A2.25 2.25 0 0 0 1.5 5.25v13.5A2.25 2.25 0 0 0 3.75 21Z" />
            </svg>
            <span className="text-[11px]">{item.width ?? "?"} × {item.height ?? "?"}</span>
          </div>
        )}
        {/* Hover info overlay – file name at bottom */}
        <div className="pointer-events-none absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/50 to-transparent pb-2 pt-8 opacity-0 transition-opacity duration-200 group-hover:opacity-100">
          <p className="truncate px-3 text-[11px] font-medium text-white/90">
            {item.id.slice(0, 8)}…
          </p>
        </div>
      </div>
    </div>
  );
}

export default Gallery;
