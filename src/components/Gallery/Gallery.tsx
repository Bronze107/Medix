import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { Media } from "@/types/media";

interface GalleryProps {
  media: Media[];
  selectedId: string | null;
  onSelect: (media: Media) => void;
  columnCount?: number;
  gap?: number;
}

function formatFileSize(bytes: number | null): string {
  if (bytes === null) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function Gallery({
  media,
  selectedId,
  onSelect,
  columnCount = 4,
  gap = 12,
}: GalleryProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const rowCount = Math.ceil(media.length / columnCount);

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 180,
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
              {rowMedia.map((item) => {
                const isSelected = item.id === selectedId;
                return (
                  <button
                    key={item.id}
                    onClick={() => onSelect(item)}
                    className={`group relative flex flex-col overflow-hidden rounded-lg border transition-all ${
                      isSelected
                        ? "border-blue-500 bg-neutral-800"
                        : "border-neutral-700 bg-neutral-800/50 hover:border-neutral-500"
                    }`}
                  >
                    <div className="flex flex-1 items-center justify-center bg-neutral-900/50 p-2">
                      <div className="text-xs text-neutral-500">
                        {item.width ?? "?"} × {item.height ?? "?"}
                      </div>
                    </div>
                    <div className="border-t border-neutral-700 p-2 text-left">
                      <p className="truncate text-xs font-medium text-neutral-300">
                        {item.id.slice(0, 8)}…
                      </p>
                      <p className="mt-0.5 text-[10px] text-neutral-500">
                        {formatFileSize(item.file_size)}
                      </p>
                    </div>
                    {isSelected && (
                      <div className="absolute inset-y-0 left-0 w-0.5 bg-blue-500" />
                    )}
                  </button>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}

export default Gallery;
