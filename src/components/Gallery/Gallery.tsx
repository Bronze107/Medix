import { useEffect, useMemo, useRef, useState } from "react";
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
  onToggleSelect: (media: Media, index: number, shiftKey: boolean) => void;
  gap?: number;
  scale?: number;
}

type MediaRow = { type: "media"; items: Media[]; height: number; startIndex: number };
type GroupRow = { type: "group"; label: string; count: number; height: number };
type Row = MediaRow | GroupRow;

function computeRows(
  media: Media[],
  containerWidth: number,
  gap: number,
  scale: number,
  groups?: GroupInfo[],
): Row[] {
  if (containerWidth <= 0) return [];
  const targetHeight = Math.round(220 * scale);
  const rows: Row[] = [];
  let i = 0;

  while (i < media.length) {
    // Check if a group header should be inserted before this row
    if (groups) {
      const g = groups.find((g) => g.startIndex === i);
      if (g) {
        rows.push({ type: "group", label: g.label, count: g.count, height: 32 });
      }
    }

    const rowItems: Media[] = [];
    let totalRatio = 0;

    for (; i < media.length; i++) {
      // Break before starting a new group
      if (rowItems.length > 0 && groups?.some((g) => g.startIndex === i)) break;

      const w = media[i].width ?? 300;
      const h = media[i].height ?? 300;
      const ratio = w / h;
      const trialWidth = (totalRatio + ratio) * targetHeight;
      const approxGaps = gap * rowItems.length;
      if (rowItems.length > 0 && trialWidth + approxGaps > containerWidth) break;
      rowItems.push(media[i]);
      totalRatio += ratio;
    }

    if (rowItems.length === 0) { i++; continue; }

    const gapTotal = gap * (rowItems.length - 1);
    const availableWidth = containerWidth - gapTotal;
    let rowHeight = availableWidth / totalRatio;
    if (rowItems.length === 1) {
      rowHeight = Math.min(targetHeight * 1.5, Math.max(targetHeight * 0.6, rowHeight));
    }

    rows.push({
      type: "media",
      items: rowItems,
      height: Math.round(rowHeight),
      startIndex: i - rowItems.length,
    });
  }

  return rows;
}

function Gallery({
  media,
  selectedId,
  onSelect,
  onDoubleClick,
  onContextMenu,
  groups,
  selectedIds,
  onToggleSelect,
  gap = 12,
  scale = 1,
}: GalleryProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [containerWidth, setContainerWidth] = useState(800);

  useEffect(() => {
    const el = parentRef.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => {
      setContainerWidth(entry.contentRect.width);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const rows = useMemo(
    () => computeRows(media, containerWidth, gap, scale, groups),
    [media, containerWidth, gap, scale, groups],
  );

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => {
      const r = rows[index];
      if (!r) return 240;
      return r.height + gap;
    },
    overscan: 3,
  });

  const virtualItems = virtualizer.getVirtualItems();

  return (
    <div ref={parentRef} className="h-full overflow-y-auto overflow-x-hidden">
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: "100%",
          position: "relative",
        }}
      >
        {virtualItems.map((virtualRow) => {
          const row = rows[virtualRow.index];
          if (!row) return null;

          if (row.type === "group") {
            return (
              <div
                key={`group-${row.label}`}
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
                <span className="text-xs font-semibold text-[var(--color-text-secondary)] select-none">{row.label}</span>
                <div className="flex-1 h-px bg-[var(--color-border)]" />
                <span className="text-[11px] text-[var(--color-text-muted)] tabular-nums">{row.count}</span>
              </div>
            );
          }

          // Media row – flex layout with per-item aspect-ratio widths
          const rowHeight = row.height;

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
              }}
            >
              <div
                style={{
                  display: "flex",
                  gap: `${gap}px`,
                  height: `${rowHeight}px`,
                }}
              >
                {row.items.map((item) => {
                  const ratio = (item.width ?? 300) / (item.height ?? 300);
                  const itemWidth = rowHeight * ratio;
                  const absIndex = row.startIndex + row.items.indexOf(item);
                  return (
                    <div key={item.id} style={{ width: `${itemWidth}px`, flexShrink: 0 }}>
                      <ThumbnailCard
                        item={item}
                        isSelected={item.id === selectedId}
                        isMultiSelected={selectedIds.includes(item.id)}
                        onClick={() => onSelect(item)}
                        onDoubleClick={onDoubleClick ? () => onDoubleClick(item) : undefined}
                        onContextMenu={onContextMenu ? (e: React.MouseEvent) => onContextMenu(e, item) : undefined}
                        onToggleSelect={(shiftKey: boolean) => onToggleSelect(item, absIndex, shiftKey)}
                      />
                    </div>
                  );
                })}
                {/* Spacer to fill remaining width when a single image is constrained */}
                <div style={{ flex: 1, minWidth: 0 }} />
              </div>
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
  onClick,
  onDoubleClick,
  onContextMenu,
  onToggleSelect,
}: {
  item: Media;
  isSelected: boolean;
  isMultiSelected: boolean;
  onClick: () => void;
  onDoubleClick?: () => void;
  onContextMenu?: (e: React.MouseEvent) => void;
  onToggleSelect: (shiftKey: boolean) => void;
}) {
  const thumbUrl = useThumbnail(item.id);
  const [loaded, setLoaded] = useState(false);

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
      className={`group relative flex h-full cursor-pointer flex-col overflow-hidden rounded-xl transition-all duration-200 ease-out ${
        isSelected
          ? "ring-2 ring-[var(--color-accent)] ring-offset-2 ring-offset-[var(--color-bg-primary)] bg-[var(--color-bg-elevated)]"
          : isMultiSelected
          ? "ring-2 ring-[var(--color-success)] ring-offset-2 ring-offset-[var(--color-bg-primary)] bg-[var(--color-bg-elevated)]"
          : "bg-[var(--color-bg-elevated)] shadow-sm hover:shadow-lg hover:shadow-black/20 hover:-translate-y-0.5"
      }`}
    >
      {/* Checkbox – top-right, hover to show */}
      <div
        className="absolute right-2 top-2 z-10 opacity-0 group-hover:opacity-100 transition-all duration-150"
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
            className={`h-full w-full object-contain transition-all duration-500 ease-out group-hover:scale-105 ${
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
        {/* Hover info overlay */}
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
