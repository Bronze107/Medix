import { useState, useEffect, useCallback, useRef } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { mediaGetPaths, variantList } from "@/lib/tauri";
import { useThumbnail } from "@/hooks/useThumbnail";
import type { Media } from "@/types/media";
import type { Variant } from "@/types/variant";

interface LightboxProps {
  media: Media[];
  currentIndex: number;
  onClose: () => void;
  onNavigate: (index: number) => void;
}

type CompareMode = "side-by-side" | "slider" | null;

function FilmstripThumb({
  item,
  isActive,
  onClick,
}: {
  item: Media;
  isActive: boolean;
  onClick: () => void;
}) {
  const url = useThumbnail(item.id, item.display_variant_id);
  return (
    <button
      onClick={(e) => { e.stopPropagation(); onClick(); }}
      className={`flex-shrink-0 overflow-hidden rounded transition-all duration-200 ${
        isActive
          ? "ring-2 ring-[var(--color-accent)] scale-100"
          : "opacity-50 hover:opacity-100"
      }`}
    >
      {url ? (
        <img src={url} alt="" className="h-16 w-16 object-cover" draggable={false} />
      ) : (
        <div className="h-16 w-16 bg-white/10" />
      )}
    </button>
  );
}

function Filmstrip({
  media,
  currentIndex,
  onNavigate,
}: {
  media: Media[];
  currentIndex: number;
  onNavigate: (index: number) => void;
}) {
  const stripRef = useRef<HTMLDivElement>(null);

  const start = Math.max(0, currentIndex - 3);
  const end = Math.min(media.length, currentIndex + 4);
  const visible = media.slice(start, end);

  return (
    <div ref={stripRef} className="flex items-center justify-center gap-1 px-3 py-2">
      {visible.map((m, i) => (
        <FilmstripThumb
          key={m.id}
          item={m}
          isActive={start + i === currentIndex}
          onClick={() => onNavigate(start + i)}
        />
      ))}
    </div>
  );
}

function Lightbox({ media, currentIndex, onClose, onNavigate }: LightboxProps) {
  const item = media[currentIndex];
  const [originalUrl, setOriginalUrl] = useState<string | null>(null);
  const [variants, setVariants] = useState<Variant[]>([]);
  const [compareMode, setCompareMode] = useState<CompareMode>(null);
  const [compareVariant, setCompareVariant] = useState<Variant | null>(null);
  const [sliderPos, setSliderPos] = useState(50);
  const [scale, setScale] = useState(1);
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const [dragging, setDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0, ox: 0, oy: 0 });
  const containerRef = useRef<HTMLDivElement>(null);

  // Load original image path
  useEffect(() => {
    setOriginalUrl(null);
    setVariants([]);
    setCompareMode(null);
    setCompareVariant(null);
    setScale(1);
    setOffset({ x: 0, y: 0 });

    if (!item) return;
    mediaGetPaths(item.id).then((paths) => {
      if (paths.original) {
        setOriginalUrl(convertFileSrc(paths.original));
      }
    });
    variantList(item.id).then(setVariants);
  }, [item]);

  // Keyboard
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      switch (e.key) {
        case "Escape":
          if (compareMode) {
            setCompareMode(null);
            setCompareVariant(null);
          } else {
            onClose();
          }
          break;
        case "ArrowLeft":
          if (currentIndex > 0) onNavigate(currentIndex - 1);
          break;
        case "ArrowRight":
          if (currentIndex < media.length - 1) onNavigate(currentIndex + 1);
          break;
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [currentIndex, media.length, onClose, onNavigate, compareMode]);

  // Mouse wheel zoom — cursor-relative, accounting for flexbox centering
  const handleWheel = useCallback((e: React.WheelEvent) => {
    if (compareMode) return; // comparison mode has its own view, don't affect single-image zoom
    e.preventDefault();
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;

    // Cursor position relative to container CENTER (flexbox centers the image)
    const cx = e.clientX - rect.left - rect.width / 2;
    const cy = e.clientY - rect.top - rect.height / 2;

    const factor = e.deltaY < 0 ? 1.2 : 1 / 1.2;
    const prevScale = scale;
    const nextScale = Math.min(5, Math.max(0.1, prevScale * factor));
    const ratio = nextScale / prevScale;

    const nextOffset = {
      x: cx - ratio * (cx - offset.x),
      y: cy - ratio * (cy - offset.y),
    };

    setScale(nextScale);
    setOffset(nextOffset);
  }, [scale, offset, compareMode]);

  // Pan handlers
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    setDragging(true);
    setDragStart({ x: e.clientX, y: e.clientY, ox: offset.x, oy: offset.y });
  }, [offset]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!dragging) return;
    setOffset({
      x: dragStart.ox + (e.clientX - dragStart.x),
      y: dragStart.oy + (e.clientY - dragStart.y),
    });
  }, [dragging, dragStart]);

  const handleMouseUp = useCallback(() => {
    setDragging(false);
  }, []);

  // Reset zoom on double click
  const handleDoubleClick = useCallback(() => {
    setScale(1);
    setOffset({ x: 0, y: 0 });
  }, []);

  // Slider drag
  const sliderDragRef = useRef(false);
  const handleSliderStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    sliderDragRef.current = true;

    const onMove = (ev: MouseEvent) => {
      if (!sliderDragRef.current || !containerRef.current) return;
      const rect = containerRef.current.getBoundingClientRect();
      const x = ev.clientX - rect.left;
      setSliderPos(Math.max(5, Math.min(95, (x / rect.width) * 100)));
    };
    const onUp = () => {
      sliderDragRef.current = false;
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }, []);

  if (!item) return null;

  const imgUrl = originalUrl;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/95"
      onClick={() => { if (!compareMode) onClose(); }}
    >
      {/* Toolbar */}
      <div className="absolute left-0 right-0 top-0 z-20 flex items-center justify-between px-4 py-3">
        <div className="flex items-center gap-3">
          <button
            onClick={onClose}
            className="rounded p-1.5 text-white/70 hover:bg-white/10 hover:text-white"
            title="关闭 (Esc)"
          >
            <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
            </svg>
          </button>

          <span className="text-sm text-white/60">
            {currentIndex + 1} / {media.length}
          </span>

          {/* Zoom controls */}
          <div className="flex items-center gap-1 border-l border-white/20 pl-3">
            <button
              onClick={(e) => { e.stopPropagation(); setScale((s) => Math.min(5, s * 1.5)); }}
              className="rounded p-1 text-white/70 hover:bg-white/10 hover:text-white text-sm"
              title="放大"
            >
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
              </svg>
            </button>
            <span className="text-xs text-white/50 w-10 text-center">{Math.round(scale * 100)}%</span>
            <button
              onClick={(e) => { e.stopPropagation(); setScale((s) => Math.max(0.1, s / 1.5)); }}
              className="rounded p-1 text-white/70 hover:bg-white/10 hover:text-white text-sm"
              title="缩小"
            >
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M5 12h14" />
              </svg>
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); setScale(1); setOffset({ x: 0, y: 0 }); }}
              className="rounded p-1 text-white/70 hover:bg-white/10 hover:text-white text-xs"
              title="重置 (双击图片)"
            >
              1:1
            </button>
          </div>

          {/* Compare controls */}
          {variants.length > 0 && (
            <div className="flex items-center gap-1 border-l border-white/20 pl-3">
              <select
                value={compareVariant?.id ?? ""}
                onChange={(e) => {
                  const v = variants.find((v) => v.id === e.target.value) ?? null;
                  setCompareVariant(v);
                  if (v && !compareMode) setCompareMode("side-by-side");
                  if (!v) setCompareMode(null);
                }}
                onClick={(e) => e.stopPropagation()}
                className="rounded bg-white/10 px-2 py-1 text-xs text-white/80 outline-none"
                style={{ colorScheme: "dark" }}
              >
                <option value="" className="bg-gray-900 text-white">
                  查看原图
                </option>
                {variants.map((v) => (
                  <option key={v.id} value={v.id} className="bg-gray-900 text-white">
                    {v.label || v.preset_name || "未命名"} ({v.format} {v.width}x{v.height})
                  </option>
                ))}
              </select>
              {compareVariant && (
                <>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setCompareMode("side-by-side");
                    }}
                    className={`rounded px-2 py-1 text-xs ${
                      compareMode === "side-by-side"
                        ? "bg-white/20 text-white"
                        : "text-white/50 hover:text-white/80"
                    }`}
                  >
                    并排
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setCompareMode("slider");
                    }}
                    className={`rounded px-2 py-1 text-xs ${
                      compareMode === "slider"
                        ? "bg-white/20 text-white"
                        : "text-white/50 hover:text-white/80"
                    }`}
                  >
                    叠加
                  </button>
                </>
              )}
            </div>
          )}
        </div>

        <div className="flex items-center gap-2 text-xs text-white/50">
          {item.width && item.height ? `${item.width} x ${item.height}` : ""}
        </div>
      </div>

      {/* Main content area */}
      <div
        ref={containerRef}
        className={`absolute inset-0 ${dragging ? "cursor-grabbing" : scale > 1 ? "cursor-grab" : "cursor-default"}`}
        onClick={(e) => e.stopPropagation()}
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
        onDoubleClick={handleDoubleClick}
      >
        {!imgUrl ? (
          <div className="flex h-full items-center justify-center text-white/30 text-sm">
            加载中...
          </div>
        ) : compareMode && compareVariant ? (
          compareMode === "side-by-side" ? (
            <div className="flex h-full w-full">
              {/* Left: Original */}
              <div className="flex-1 relative overflow-hidden border-r border-white/20">
                <div className="text-[10px] text-white/40 absolute top-2 left-0 right-0 text-center pointer-events-none z-10">
                  原图
                </div>
                <img
                  src={imgUrl}
                  alt="原图"
                  className="absolute inset-0 w-full h-full object-contain"
                  draggable={false}
                />
              </div>
              {/* Right: Variant */}
              <div className="flex-1 relative overflow-hidden">
                <div className="text-[10px] text-white/40 absolute top-2 left-0 right-0 text-center pointer-events-none z-10">
                  {(compareVariant.label || compareVariant.preset_name || "版本")}
                </div>
                <img
                  src={convertFileSrc(compareVariant.file_path)}
                  alt={(compareVariant.label || compareVariant.preset_name || "版本")}
                  className="absolute inset-0 w-full h-full object-contain"
                  draggable={false}
                />
              </div>
            </div>
          ) : (
            /* Slider overlay */
            <div className="relative h-full w-full">
              {/* Original - clipped right side */}
              <img
                src={imgUrl}
                alt="原图"
                className="absolute inset-0 w-full h-full object-contain"
                style={{
                  clipPath: `inset(0 ${100 - sliderPos}% 0 0)`,
                }}
                draggable={false}
              />
              {/* Variant - clipped left side */}
              <img
                src={convertFileSrc(compareVariant.file_path)}
                alt={(compareVariant.label || compareVariant.preset_name || "版本")}
                className="absolute inset-0 w-full h-full object-contain"
                style={{
                  clipPath: `inset(0 0 0 ${sliderPos}%)`,
                }}
                draggable={false}
              />
              {/* Slider line */}
              <div
                className="absolute inset-y-0 z-10 flex items-center justify-center"
                style={{ left: `${sliderPos}%` }}
                onMouseDown={handleSliderStart}
              >
                <div className="h-full w-0.5 bg-white shadow-lg cursor-ew-resize" />
                <div className="absolute flex h-8 w-8 items-center justify-center rounded-full bg-white/90 shadow-lg cursor-ew-resize">
                  <svg className="h-4 w-4 text-gray-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M8 4l-7 8 7 8M16 4l7 8-7 8" />
                  </svg>
                </div>
              </div>
              <div className="absolute bottom-4 left-0 right-0 text-center pointer-events-none">
                <span className="rounded bg-black/50 px-2 py-1 text-[10px] text-white/50">
                  原图 ← → {(compareVariant.label || compareVariant.preset_name || "版本")}
                </span>
              </div>
            </div>
          )
        ) : (
          /* Single image with zoom/pan */
          <div
            className="flex h-full w-full items-center justify-center"
            style={{
              transform: `translate(${offset.x}px, ${offset.y}px) scale(${scale})`,
            }}
          >
            <img
              src={imgUrl}
              alt=""
              className="max-h-full max-w-full object-contain select-none"
              draggable={false}
            />
          </div>
        )}
      </div>

      {/* Filmstrip */}
      {!compareMode && media.length > 1 && (
        <div className="absolute bottom-0 left-0 right-0 z-20 border-t border-white/10 bg-black/60 backdrop-blur-sm">
          <Filmstrip
            media={media}
            currentIndex={currentIndex}
            onNavigate={onNavigate}
          />
        </div>
      )}

      {/* Prev/Next buttons */}
      {!compareMode && (
        <>
          <button
            onClick={(e) => {
              e.stopPropagation();
              if (currentIndex > 0) {
                onNavigate(currentIndex - 1);
              }
            }}
            disabled={currentIndex === 0}
            className="absolute left-3 top-1/2 z-10 -translate-y-1/2 rounded-full bg-white/10 p-2 text-white/70 hover:bg-white/20 hover:text-white disabled:opacity-20"
          >
            <svg className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 19.5 8.25 12l7.5-7.5" />
            </svg>
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              if (currentIndex < media.length - 1) {
                onNavigate(currentIndex + 1);
              }
            }}
            disabled={currentIndex === media.length - 1}
            className="absolute right-3 top-1/2 z-10 -translate-y-1/2 rounded-full bg-white/10 p-2 text-white/70 hover:bg-white/20 hover:text-white disabled:opacity-20"
          >
            <svg className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="m8.25 4.5 7.5 7.5-7.5 7.5" />
            </svg>
          </button>
        </>
      )}
    </div>
  );
}

export default Lightbox;
