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

type CompareMode = "side-by-side" | "slider";

// --- View state machine ---
type ViewState =
  | { type: "single"; activeId: string | null }              // null = original
  | { type: "compare"; leftId: string | null; rightId: string | null; mode: CompareMode };

function formatSize(bytes: number | null | undefined): string {
  if (bytes == null) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1048576).toFixed(1)} MB`;
}

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
        <img src={url} alt="" className="h-16 w-16 object-cover" draggable={false} decoding="async" />
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
  const start = Math.max(0, currentIndex - 3);
  const end = Math.min(media.length, currentIndex + 4);
  const visible = media.slice(start, end);

  return (
    <div className="flex items-center justify-center gap-1 px-3 py-2">
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

// --- Variant thumbnail in right panel ---
function VariantThumb({
  label,
  detail,
  source,
  isActive,
  isCompareSelected,
  filePath,
  onClick,
  onCtrlClick,
}: {
  label: string;
  detail: string;
  source: string | null;
  isActive: boolean;
  isCompareSelected: boolean;
  filePath: string;
  onClick: () => void;
  onCtrlClick: () => void;
}) {
  const [imgLoaded, setImgLoaded] = useState(false);
  const src = convertFileSrc(filePath);

  return (
    <button
      onClick={(e) => {
        if (e.ctrlKey || e.metaKey) {
          onCtrlClick();
        } else {
          onClick();
        }
      }}
      className={`group relative flex w-full items-start gap-2 rounded-lg p-2 text-left transition-colors ${
        isActive
          ? "bg-white/15 ring-1 ring-white/30"
          : isCompareSelected
          ? "bg-[var(--color-accent)]/15 ring-1 ring-[var(--color-accent)]/40"
          : "hover:bg-white/8"
      }`}
    >
      <div className="relative h-12 w-12 flex-shrink-0 overflow-hidden rounded-md bg-white/5">
        <img
          src={src}
          alt=""
          decoding="async"
          className={`h-full w-full object-cover transition-all duration-300 ${
            imgLoaded ? "opacity-100" : "opacity-0"
          }`}
          onLoad={() => setImgLoaded(true)}
          draggable={false}
        />
      </div>
      <div className="min-w-0 flex-1">
        <p className={`truncate text-[11px] font-medium ${
          isActive ? "text-white" : "text-white/80"
        }`}>
          {label}
        </p>
        <p className="truncate text-[10px] text-white/45">{detail}</p>
        {source && (
          <span className={`mt-0.5 inline-block rounded px-1 py-px text-[9px] ${
            source === "generated"
              ? "bg-blue-400/20 text-blue-300/80"
              : "bg-green-400/20 text-green-300/80"
          }`}>
            {source === "generated" ? "生成" : "导入"}
          </span>
        )}
      </div>
      {isCompareSelected && (
        <span className="absolute right-2 top-1 text-[9px] text-[var(--color-accent)]">
          {isActive ? "L" : "R"}
        </span>
      )}
    </button>
  );
}

function Lightbox({ media, currentIndex, onClose, onNavigate }: LightboxProps) {
  const item = media[currentIndex];
  const [originalUrl, setOriginalUrl] = useState<string | null>(null);
  const [rawOriginalPath, setRawOriginalPath] = useState<string | null>(null);
  const [variants, setVariants] = useState<Variant[]>([]);
  const [viewState, setViewState] = useState<ViewState>({ type: "single", activeId: null });
  const [scale, setScale] = useState(1);
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const [dragging, setDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0, ox: 0, oy: 0 });
  const containerRef = useRef<HTMLDivElement>(null);

  // Load original + variants when item changes
  useEffect(() => {
    setOriginalUrl(null);
    setVariants([]);
    setScale(1);
    setOffset({ x: 0, y: 0 });

    if (!item) return;
    mediaGetPaths(item.id).then((paths) => {
      if (paths.original) {
        setRawOriginalPath(paths.original);
        setOriginalUrl(convertFileSrc(paths.original));
      }
    });
    variantList(item.id).then((list) => {
      setVariants(list);
      // If display variant is set, show it as the active image
      if (item.display_variant_id) {
        const dv = list.find((v) => v.id === item.display_variant_id);
        if (dv) {
          setViewState({ type: "single", activeId: dv.id });
        } else {
          setViewState({ type: "single", activeId: null });
        }
      } else {
        setViewState({ type: "single", activeId: null });
      }
    });
  }, [item]);

  // Helper: get file path for an id (null = original)
  const getFilePath = useCallback(
    (id: string | null): string | null => {
      if (id === null) return originalUrl;
      const v = variants.find((v) => v.id === id);
      return v ? convertFileSrc(v.file_path) : null;
    },
    [originalUrl, variants],
  );

  // Helper: get variant display info for an id
  const getVariantInfo = useCallback(
    (id: string | null) => {
      if (id === null) return { label: "原图", detail: item ? `${item.width ?? "?"}×${item.height ?? "?"}` : "", source: null };
      const v = variants.find((v) => v.id === id);
      if (!v) return { label: "未知", detail: "", source: null };
      const fmt = v.format.toUpperCase();
      const dim = `${v.width ?? "?"}×${v.height ?? "?"}`;
      const detail = `${fmt}${v.quality && v.format === "jpeg" ? `·Q${v.quality}` : ""} · ${dim} · ${formatSize(v.file_size)}`;
      return { label: v.label || v.preset_name || "未命名版本", detail, source: v.source };
    },
    [item, variants],
  );

  // Determine if we're in compare mode
  const compareMode =
    viewState.type === "compare" ? viewState.mode : null;

  // Determine which ids are selected for comparison
  const compareLeft = viewState.type === "compare" ? viewState.leftId : undefined;
  const compareRight = viewState.type === "compare" ? viewState.rightId : undefined;

  // Active id for single view
  const activeId = viewState.type === "single" ? viewState.activeId : null;

  // Keyboard
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      switch (e.key) {
        case "Escape":
          e.stopPropagation();
          onClose();
          break;
        case "ArrowLeft":
          if (viewState.type !== "compare" && currentIndex > 0) onNavigate(currentIndex - 1);
          break;
        case "ArrowRight":
          if (viewState.type !== "compare" && currentIndex < media.length - 1) onNavigate(currentIndex + 1);
          break;
        case "ArrowUp":
        case "ArrowDown": {
          e.preventDefault();
          if (media.length === 0) break;
          // Build ordered list: [null (original), ...variant ids]
          const ids = [null as string | null, ...variants.map((v) => v.id)];
          if (viewState.type === "compare") {
            const curIdx = ids.indexOf(viewState.rightId);
            const nextIdx = e.key === "ArrowUp"
              ? Math.max(0, curIdx - 1)
              : Math.min(ids.length - 1, curIdx + 1);
            setViewState({ ...viewState, rightId: ids[nextIdx] });
          } else {
            const curIdx = ids.indexOf(viewState.activeId);
            const nextIdx = e.key === "ArrowUp"
              ? Math.max(0, curIdx - 1)
              : Math.min(ids.length - 1, curIdx + 1);
            setViewState({ type: "single", activeId: ids[nextIdx] });
            setScale(1);
            setOffset({ x: 0, y: 0 });
          }
          break;
        }
        case "Tab":
          if (viewState.type === "compare") {
            e.preventDefault();
            setViewState({
              ...viewState,
              mode: viewState.mode === "side-by-side" ? "slider" : "side-by-side",
            });
          }
          break;
      }
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [currentIndex, media.length, onClose, onNavigate, viewState, variants]);

  // Mouse wheel zoom — cursor-relative (single view only)
  const handleWheel = useCallback(
    (e: React.WheelEvent) => {
      if (viewState.type !== "single") return;
      e.preventDefault();
      const rect = containerRef.current?.getBoundingClientRect();
      if (!rect) return;

      const cx = e.clientX - rect.left - rect.width / 2;
      const cy = e.clientY - rect.top - rect.height / 2;

      const factor = e.deltaY < 0 ? 1.2 : 1 / 1.2;
      const prevScale = scale;
      const nextScale = Math.min(5, Math.max(0.1, prevScale * factor));
      const ratio = nextScale / prevScale;

      setScale(nextScale);
      setOffset({ x: cx - ratio * (cx - offset.x), y: cy - ratio * (cy - offset.y) });
    },
    [scale, offset, viewState.type],
  );

  // Pan handlers
  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (e.button !== 0) return;
      setDragging(true);
      setDragStart({ x: e.clientX, y: e.clientY, ox: offset.x, oy: offset.y });
    },
    [offset],
  );

  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
      if (!dragging) return;
      setOffset({ x: dragStart.ox + (e.clientX - dragStart.x), y: dragStart.oy + (e.clientY - dragStart.y) });
    },
    [dragging, dragStart],
  );

  const handleMouseUp = useCallback(() => setDragging(false), []);
  const handleDoubleClick = useCallback(() => { setScale(1); setOffset({ x: 0, y: 0 }); }, []);

  // Slider drag
  const sliderDragRef = useRef(false);
  const [sliderPos, setSliderPos] = useState(50);
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

  // --- Variant click handlers ---
  const handleVariantClick = useCallback(
    (id: string | null) => {
      if (viewState.type === "compare") {
        // In compare mode, clicking replaces the right side
        setViewState({ ...viewState, rightId: id });
      } else {
        // Single mode → preview this variant
        setViewState({ type: "single", activeId: id });
        setScale(1);
        setOffset({ x: 0, y: 0 });
      }
    },
    [viewState],
  );

  const handleVariantCtrlClick = useCallback(
    (id: string | null) => {
      // Ctrl+click: add to comparison
      if (viewState.type === "compare") {
        // Already comparing — replace right side
        setViewState({ ...viewState, rightId: id });
        return;
      }
      // Enter compare mode with current active + clicked variant
      const left = viewState.activeId;
      setViewState({ type: "compare", leftId: left, rightId: id, mode: "side-by-side" });
    },
    [viewState],
  );

  if (!item) return null;

  const mainUrl = viewState.type === "single" ? getFilePath(viewState.activeId) : null;
  const hasVariants = variants.length > 0;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/95"
      onClick={() => {
        if (viewState.type === "compare") {
          const lastActive = viewState.leftId !== null ? viewState.leftId : viewState.rightId;
          setViewState({ type: "single", activeId: lastActive });
        } else {
          onClose();
        }
      }}
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

          {/* Zoom controls — single view only */}
          {viewState.type === "single" && (
            <div className="flex items-center gap-1 border-l border-white/20 pl-3">
              <button
                onClick={(e) => { e.stopPropagation(); setScale((s) => Math.min(5, s * 1.5)); }}
                className="rounded p-1 text-white/70 hover:bg-white/10 hover:text-white"
                title="放大"
              >
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
                </svg>
              </button>
              <span className="w-10 text-center text-xs text-white/50">{Math.round(scale * 100)}%</span>
              <button
                onClick={(e) => { e.stopPropagation(); setScale((s) => Math.max(0.1, s / 1.5)); }}
                className="rounded p-1 text-white/70 hover:bg-white/10 hover:text-white"
                title="缩小"
              >
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M5 12h14" />
                </svg>
              </button>
              <button
                onClick={(e) => { e.stopPropagation(); setScale(1); setOffset({ x: 0, y: 0 }); }}
                className="rounded p-1 text-white/70 hover:bg-white/10 hover:text-white text-xs"
                title="重置"
              >
                1:1
              </button>
            </div>
          )}

          {/* Compare mode indicator + toggle */}
          {viewState.type === "compare" && (
            <div className="flex items-center gap-1 border-l border-white/20 pl-3">
              <span className="text-xs text-white/50">对比</span>
              <button
                onClick={(e) => { e.stopPropagation(); setViewState({ ...viewState, mode: "side-by-side" }); }}
                className={`rounded px-2 py-0.5 text-xs ${
                  viewState.mode === "side-by-side" ? "bg-white/20 text-white" : "text-white/50 hover:text-white/80"
                }`}
              >
                并排
              </button>
              <button
                onClick={(e) => { e.stopPropagation(); setViewState({ ...viewState, mode: "slider" }); }}
                className={`rounded px-2 py-0.5 text-xs ${
                  viewState.mode === "slider" ? "bg-white/20 text-white" : "text-white/50 hover:text-white/80"
                }`}
              >
                叠加
              </button>
              <span className="ml-1 text-[10px] text-white/30">点击空白退出对比</span>
            </div>
          )}
        </div>

        {/* Current item info */}
        <div className="flex items-center gap-2 text-xs text-white/50">
          {viewState.type === "single" && (
            <span>{getVariantInfo(activeId).label}</span>
          )}
          <span>
            {item.width && item.height ? `${item.width} x ${item.height}` : ""}
          </span>
        </div>
      </div>

      {/* Main content area */}
      <div
        ref={containerRef}
        className={`absolute inset-0 ${
          hasVariants ? "right-[172px]" : ""
        } ${
          dragging ? "cursor-grabbing" : viewState.type === "single" && scale > 1 ? "cursor-grab" : "cursor-default"
        }`}
        onClick={(e) => e.stopPropagation()}
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
        onDoubleClick={handleDoubleClick}
      >
        {!mainUrl && viewState.type === "single" ? (
          <div className="flex h-full items-center justify-center text-sm text-white/30">加载中...</div>
        ) : viewState.type === "compare" ? (
          compareMode === "side-by-side" ? (
            /* ──── Side-by-side ──── */
            <div className="flex h-full w-full">
              <div className="flex-1 relative overflow-hidden border-r border-white/20">
                <div className="pointer-events-none absolute left-0 right-0 top-2 z-10 text-center text-[10px] text-white/40">
                  {getVariantInfo(compareLeft ?? null).label}
                </div>
                <img
                  src={getFilePath(compareLeft ?? null) ?? ""}
                  alt=""
                  className="absolute inset-0 w-full h-full object-contain"
                  draggable={false}
                />
              </div>
              <div className="flex-1 relative overflow-hidden">
                <div className="pointer-events-none absolute left-0 right-0 top-2 z-10 text-center text-[10px] text-white/40">
                  {getVariantInfo(compareRight ?? null).label}
                </div>
                <img
                  src={getFilePath(compareRight ?? null) ?? ""}
                  alt=""
                  className="absolute inset-0 w-full h-full object-contain"
                  draggable={false}
                />
              </div>
            </div>
          ) : (
            /* ──── Slider overlay ──── */
            <div className="relative h-full w-full">
              <img
                src={getFilePath(compareLeft ?? null) ?? ""}
                alt=""
                className="absolute inset-0 w-full h-full object-contain"
                style={{ clipPath: `inset(0 ${100 - sliderPos}% 0 0)` }}
                draggable={false}
                decoding="async"
              />
              <img
                src={getFilePath(compareRight ?? null) ?? ""}
                alt=""
                className="absolute inset-0 w-full h-full object-contain"
                style={{ clipPath: `inset(0 0 0 ${sliderPos}%)` }}
                draggable={false}
                decoding="async"
              />
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
              <div className="pointer-events-none absolute bottom-4 left-0 right-0 text-center">
                <span className="rounded bg-black/50 px-2 py-1 text-[10px] text-white/50">
                  {getVariantInfo(compareLeft ?? null).label} ← → {getVariantInfo(compareRight ?? null).label}
                </span>
              </div>
            </div>
          )
        ) : (
          /* ──── Single image with zoom/pan ──── */
          <div
            className="flex h-full w-full items-center justify-center"
            style={{ transform: `translate(${offset.x}px, ${offset.y}px) scale(${scale})` }}
          >
            {mainUrl && (
              <img src={mainUrl} alt="" className="max-h-full max-w-full object-contain select-none" draggable={false} decoding="async" />
            )}
          </div>
        )}
      </div>

      {/* ──── Right variant panel ──── */}
      {hasVariants && (
        <div
          className="absolute bottom-0 right-0 top-0 z-20 w-[172px] border-l border-white/10 bg-black/60 backdrop-blur-sm"
          onClick={(e) => e.stopPropagation()}
        >
          <div className="flex h-full flex-col">
            <div className="border-b border-white/10 px-3 py-2.5">
              <p className="text-[11px] font-medium text-white/60">版本</p>
            </div>
            <div className="flex-1 overflow-y-auto px-2 py-1.5 space-y-1">
              {/* Original */}
              <VariantThumb
                label="原图"
                detail={item ? `${item.width ?? "?"}×${item.height ?? "?"}` : ""}
                source={null}
                isActive={viewState.type === "single" && viewState.activeId === null}
                isCompareSelected={
                  viewState.type === "compare" &&
                  (viewState.leftId === null || viewState.rightId === null)
                }
                filePath={rawOriginalPath ?? ""}
                onClick={() => handleVariantClick(null)}
                onCtrlClick={() => handleVariantCtrlClick(null)}
              />

              {/* Variants */}
              {variants.map((v) => {
                const info = getVariantInfo(v.id);
                return (
                  <VariantThumb
                    key={v.id}
                    label={info.label}
                    detail={info.detail}
                    source={info.source}
                    isActive={viewState.type === "single" && viewState.activeId === v.id}
                    isCompareSelected={
                      viewState.type === "compare" &&
                      (viewState.leftId === v.id || viewState.rightId === v.id)
                    }
                    filePath={v.file_path}
                    onClick={() => handleVariantClick(v.id)}
                    onCtrlClick={() => handleVariantCtrlClick(v.id)}
                  />
                );
              })}
            </div>
            <div className="border-t border-white/10 px-3 py-2">
              <p className="text-[10px] text-white/30">Ctrl+点击 对比 · Tab 切换</p>
            </div>
          </div>
        </div>
      )}

      {/* Filmstrip — only in single view, when not comparing */}
      {viewState.type !== "compare" && media.length > 1 && (
        <div className="absolute bottom-0 left-0 right-0 z-20 border-t border-white/10 bg-black/60 backdrop-blur-sm">
          <Filmstrip media={media} currentIndex={currentIndex} onNavigate={onNavigate} />
        </div>
      )}

      {/* Prev/Next — only in single view */}
      {viewState.type !== "compare" && (
        <>
          <button
            onClick={(e) => { e.stopPropagation(); if (currentIndex > 0) onNavigate(currentIndex - 1); }}
            disabled={currentIndex === 0}
            className="absolute left-3 top-1/2 z-10 -translate-y-1/2 rounded-full bg-white/10 p-2 text-white/70 hover:bg-white/20 hover:text-white disabled:opacity-20"
          >
            <svg className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 19.5 8.25 12l7.5-7.5" />
            </svg>
          </button>
          <button
            onClick={(e) => { e.stopPropagation(); if (currentIndex < media.length - 1) onNavigate(currentIndex + 1); }}
            disabled={currentIndex === media.length - 1}
            className="absolute right-[188px] top-1/2 z-10 -translate-y-1/2 rounded-full bg-white/10 p-2 text-white/70 hover:bg-white/20 hover:text-white disabled:opacity-20"
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
