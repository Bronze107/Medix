import { useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { Media } from "@/types/media";
import { mediaThumbnail } from "@/lib/tauri";

interface GalleryProps {
  media: Media[];
  selectedId: string | null;
  onSelect: (media: Media) => void;
  selectedIds: string[];
  selectionMode: boolean;
  onToggleSelect: (media: Media) => void;
  columnCount?: number;
  gap?: number;
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
    let retryCount = 0;
    const maxRetries = 15;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    const load = () => {
      mediaThumbnail(id)
        .then((b64) => {
          if (!cancelled) {
            thumbCache.set(id, b64);
            setUrl(b64);
          }
        })
        .catch(() => {
          if (!cancelled) {
            setUrl(null);
            retryCount++;
            if (retryCount <= maxRetries) {
              retryTimer = setTimeout(load, 2000);
            }
          }
        });
    };

    load();

    return () => {
      cancelled = true;
      if (retryTimer) clearTimeout(retryTimer);
    };
  }, [id]);

  return url;
}

function Gallery({
  media,
  selectedId,
  onSelect,
  selectedIds,
  selectionMode,
  onToggleSelect,
  columnCount = 4,
  gap = 12,
}: GalleryProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const rowCount = Math.ceil(media.length / columnCount);

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 220,
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
          const rowIndex = virtualRow.index;
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
              {rowMedia.map((item) => (
                <ThumbnailCard
                  key={item.id}
                  item={item}
                  isSelected={item.id === selectedId}
                  isMultiSelected={selectedIds.includes(item.id)}
                  selectionMode={selectionMode}
                  onClick={() => onSelect(item)}
                  onToggleSelect={() => onToggleSelect(item)}
                />
              ))}
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
  onToggleSelect,
}: {
  item: Media;
  isSelected: boolean;
  isMultiSelected: boolean;
  selectionMode: boolean;
  onClick: () => void;
  onToggleSelect: () => void;
}) {
  const thumbUrl = useThumbnail(item.id);

  const handleCardClick = () => {
    if (selectionMode) {
      onToggleSelect();
    } else {
      onClick();
    }
  };

  return (
    <div
      onClick={handleCardClick}
      className={`group relative flex cursor-pointer flex-col overflow-hidden rounded-lg border transition-all ${
        isSelected
          ? "border-blue-500 bg-neutral-800"
          : isMultiSelected
          ? "border-green-500 bg-neutral-800/70"
          : "border-neutral-700 bg-neutral-800/50 hover:border-neutral-500"
      }`}
    >
      {/* Checkbox overlay */}
      <div
        className={`absolute left-2 top-2 z-10 flex h-5 w-5 items-center justify-center rounded border transition-all ${
          isMultiSelected
            ? "border-green-500 bg-green-500 text-white"
            : "border-neutral-600 bg-neutral-900/80 text-transparent group-hover:border-neutral-400"
        } ${selectionMode ? "opacity-100" : "opacity-0 group-hover:opacity-100"}`}
        onClick={(e) => {
          e.stopPropagation();
          onToggleSelect();
        }}
      >
        <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={3}>
          <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
        </svg>
      </div>

      <div className="relative flex flex-1 items-center justify-center overflow-hidden bg-neutral-900/50">
        {thumbUrl ? (
          <img
            src={thumbUrl}
            alt=""
            loading="lazy"
            className="h-full w-full object-cover"
            draggable={false}
          />
        ) : (
          <div className="flex flex-col items-center gap-1 p-2 text-neutral-500">
            <svg
              className="h-8 w-8"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={1}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="m2.25 15.75 5.159-5.159a2.25 2.25 0 0 1 3.182 0l5.159 5.159m-1.5-1.5 1.409-1.409a2.25 2.25 0 0 1 3.182 0l2.909 2.909M3.75 21h16.5a2.25 2.25 0 0 0 2.25-2.25V5.25A2.25 2.25 0 0 0 20.25 3H3.75A2.25 2.25 0 0 0 1.5 5.25v13.5A2.25 2.25 0 0 0 3.75 21Z"
              />
            </svg>
            <span className="text-[10px]">
              {item.width ?? "?"} × {item.height ?? "?"}
            </span>
          </div>
        )}
      </div>
      <div className="border-t border-neutral-700 p-2 text-left">
        <p className="truncate text-xs font-medium text-neutral-300">
          {item.id.slice(0, 8)}…
        </p>
        <p className="mt-0.5 text-[10px] text-neutral-500">
          {formatFileSize(item.file_size)}
        </p>
      </div>
      {isSelected && !selectionMode && (
        <div className="absolute inset-y-0 left-0 w-0.5 bg-blue-500" />
      )}
    </div>
  );
}

export default Gallery;
