import { useState, useEffect, useCallback, useRef } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { mediaGetPaths } from "@/lib/tauri";
import { useThumbnail } from "@/hooks/useThumbnail";
import type { Media } from "@/types/media";

interface LightboxProps {
  media: Media[];
  currentIndex: number;
  onClose: () => void;
  onNavigate: (index: number) => void;
  /** 请求裁剪某张图。由父组件渲染编辑器（本组件在关闭时会卸载）。 */
  onCrop: (mediaId: string) => void;
  /** 以对比模式打开（网格多选「对比」入口）。仅在挂载时生效。 */
  initialCompare?: { leftId: string; rightId: string } | null;
}

type CompareMode = "side-by-side" | "slider";

// --- View state machine ---
type ViewState =
  | { type: "single"; activeId: string | null }
  | { type: "compare"; leftId: string; rightId: string; mode: CompareMode };

function FilmstripThumb({
  item,
  isActive,
  slotBadge,
  onClick,
}: {
  item: Media;
  isActive: boolean;
  slotBadge?: "A" | "B" | null;
  onClick: (shiftKey: boolean) => void;
}) {
  const url = useThumbnail(item.id);
  return (
    <button
      onClick={(e) => { e.stopPropagation(); onClick(e.shiftKey); }}
      className={`relative flex-shrink-0 overflow-hidden rounded transition-all duration-200 ${
        slotBadge === "A"
          ? "ring-2 ring-[var(--color-accent)]"
          : slotBadge === "B"
          ? "ring-2 ring-[var(--color-success)]"
          : isActive
          ? "ring-2 ring-[var(--color-accent)] scale-100"
          : "opacity-50 hover:opacity-100"
      }`}
    >
      {url ? (
        <img src={url} alt="" className="h-16 w-16 object-cover" draggable={false} decoding="async" />
      ) : (
        <div className="h-16 w-16 bg-white/10" />
      )}
      {slotBadge && (
        <span
          className={`absolute left-0.5 top-0.5 rounded px-1 text-[11px] font-semibold text-white ${
            slotBadge === "A" ? "bg-[var(--color-accent)]" : "bg-[var(--color-success)]"
          }`}
        >
          {slotBadge}
        </span>
      )}
    </button>
  );
}

function Filmstrip({
  media,
  currentIndex,
  comparePinned,
  onNavigate,
  onEnterCompare,
  compare = false,
  leftId,
  rightId,
  onSetSlot,
}: {
  media: Media[];
  currentIndex: number;
  comparePinned: string | null;
  onNavigate: (index: number) => void;
  onEnterCompare: (rightId: string) => void;
  /** 对比模式：B 槽为锚点加大窗口，点击换 B 槽、Shift 点击换 A 槽 */
  compare?: boolean;
  leftId?: string;
  rightId?: string;
  onSetSlot?: (slot: "A" | "B", id: string) => void;
}) {
  const anchorIndex = compare
    ? Math.max(0, media.findIndex((m) => m.id === rightId))
    : currentIndex;
  const windowSize = compare ? 10 : 3;
  const start = Math.max(0, anchorIndex - windowSize);
  const end = Math.min(media.length, anchorIndex + windowSize + 1);
  const visible = media.slice(start, end);

  return (
    <div
      onClick={(e) => e.stopPropagation()}
      className={`flex items-center gap-1 px-3 py-2 ${compare ? "justify-start overflow-x-auto" : "justify-center"}`}
    >
      {visible.map((m, i) => {
        const index = start + i;
        const slot = compare ? (m.id === leftId ? "A" as const : m.id === rightId ? "B" as const : null) : null;
        return (
          <FilmstripThumb
            key={m.id}
            item={m}
            isActive={slot === null && index === currentIndex}
            slotBadge={slot}
            onClick={(shiftKey) => {
              if (compare) {
                if (!onSetSlot) return;
                if (shiftKey) {
                  if (m.id !== rightId) onSetSlot("A", m.id);
                } else if (m.id !== leftId) {
                  onSetSlot("B", m.id);
                }
              } else if (comparePinned && m.id !== comparePinned) {
                onEnterCompare(m.id);
              } else {
                onNavigate(index);
              }
            }}
          />
        );
      })}
    </div>
  );
}

/** 已固定图片浮卡（③）：翻页持续显示，Enter 与当前图发起对比 */
function PinnedCard({
  pinnedId,
  onCancel,
}: {
  pinnedId: string;
  onCancel: () => void;
}) {
  const url = useThumbnail(pinnedId);
  return (
    <div
      className="absolute bottom-24 right-4 z-30 flex items-center gap-2 rounded-xl border border-[var(--color-accent)]/50 bg-[var(--color-bg-elevated)]/95 p-2 shadow-2xl backdrop-blur-xl animate-fade-in"
      onClick={(e) => e.stopPropagation()}
    >
      {url ? (
        <img src={url} alt="" className="h-12 w-12 rounded-lg object-cover" draggable={false} />
      ) : (
        <div className="h-12 w-12 rounded-lg bg-white/10" />
      )}
      <div className="pr-1">
        <p className="text-xs font-medium text-[var(--color-text-primary)]">已固定 A 槽</p>
        <p className="text-[11px] text-[var(--color-text-muted)]">Enter 与当前图对比</p>
      </div>
      <button
        onClick={onCancel}
        className="rounded p-1 text-[var(--color-text-muted)] transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
        title="取消固定"
      >
        <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
        </svg>
      </button>
    </div>
  );
}

function Lightbox({ media, currentIndex, onClose, onNavigate, onCrop, initialCompare }: LightboxProps) {
  const item = media[currentIndex];
  const [fileUrlCache, setFileUrlCache] = useState<Record<string, string>>({});
  const [viewState, setViewState] = useState<ViewState>(() =>
    initialCompare
      ? { type: "compare", leftId: initialCompare.leftId, rightId: initialCompare.rightId, mode: "side-by-side" }
      : { type: "single", activeId: null },
  );
  const [comparePinned, setComparePinned] = useState<string | null>(null);
  const [scale, setScale] = useState(1);
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const [dragging, setDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0, ox: 0, oy: 0 });
  const containerRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);

  // Pre-load current item's URL into cache
  useEffect(() => {
    if (!item) return;
    const id = item.id;
    if (fileUrlCache[id]) return;
    mediaGetPaths(id).then((paths) => {
      const url = paths.original;
      if (url) {
        setFileUrlCache(prev => ({ ...prev, [id]: convertFileSrc(url) }));
      }
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item?.id]);

  // Reset zoom on item change. 导航不销毁对比状态和 pin：对比模式下 ←/→ 滑动
  // B 槽依赖 onNavigate 更新 currentIndex，pin 则跨越翻页持续有效。
  useEffect(() => {
    setScale(1);
    setOffset({ x: 0, y: 0 });
    if (!item) return;
    setViewState((prev) =>
      prev.type === "single" && prev.activeId !== item.id
        ? { type: "single", activeId: item.id }
        : prev,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item?.id]);

  // Resolve any media ID to a file URL via the cache
  function getFilePath(mediaId: string | null): string | null {
    if (!mediaId) return null;
    return fileUrlCache[mediaId] ?? null;
  }

  // Resolve media type — only the current item's type is known
  function getActiveMediaType(mediaId: string | null): string {
    if (!mediaId || mediaId === item?.id) return item?.media_type ?? "image";
    return "image";
  }

  // Derived values
  const compareMode = viewState.type === "compare" ? viewState.mode : null;
  const compareLeft = viewState.type === "compare" ? viewState.leftId : undefined;
  const compareRight = viewState.type === "compare" ? viewState.rightId : undefined;
  const activeId = viewState.type === "single" ? viewState.activeId : null;

  // ---- compare pin flow ----
  const handleEnterCompare = useCallback((rightId: string) => {
    if (!comparePinned) return;
    setComparePinned(null);
    setViewState({
      type: "compare",
      leftId: comparePinned,
      rightId,
      mode: "side-by-side",
    });
  }, [comparePinned]);

  // 对比模式底条换槽（②）
  const handleSetSlot = useCallback((slot: "A" | "B", id: string) => {
    setViewState((prev) =>
      prev.type === "compare"
        ? slot === "A"
          ? { ...prev, leftId: id }
          : { ...prev, rightId: id }
        : prev,
    );
  }, []);

  // Pre-load compare image URLs when entering compare mode
  const compareIds = viewState.type === "compare"
    ? `${viewState.leftId}|${viewState.rightId}`
    : null;

  useEffect(() => {
    if (!compareIds) return;
    const [leftId, rightId] = compareIds.split("|");
    [leftId, rightId].forEach(cacheId => {
      if (cacheId && !fileUrlCache[cacheId]) {
        mediaGetPaths(cacheId).then(paths => {
          const url = paths.original;
          if (url) {
            setFileUrlCache(prev => ({ ...prev, [cacheId]: convertFileSrc(url) }));
          }
        });
      }
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [compareIds]);

  // ---- compare B-slot stepping (②)：A 槽钉住，←/→ 让 B 槽沿整个列表滑动 ----
  const stepCompareSlot = useCallback(
    (dir: 1 | -1) => {
      if (viewState.type !== "compare") return;
      const rightIdx = media.findIndex((m) => m.id === viewState.rightId);
      let idx = rightIdx;
      do {
        idx += dir;
      } while (idx >= 0 && idx < media.length && media[idx].id === viewState.leftId);
      if (idx < 0 || idx >= media.length) return;
      onNavigate(idx);
      setViewState({ ...viewState, rightId: media[idx].id });
    },
    [media, onNavigate, viewState],
  );

  // Keyboard
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      switch (e.key) {
        case " ":
          if (viewState.type !== "compare" && getActiveMediaType(activeId) === "video" && videoRef.current) {
            e.preventDefault();
            if (videoRef.current.paused) {
              videoRef.current.play();
            } else {
              videoRef.current.pause();
            }
          }
          break;
        case "Escape":
          e.stopPropagation();
          if (viewState.type === "compare") {
            setViewState({ type: "single", activeId: item?.id ?? null });
          } else {
            onClose();
          }
          break;
        case "Enter":
          // pin 存活时，Enter 与当前图发起对比（③）
          if (viewState.type === "single" && comparePinned && item?.id && comparePinned !== item.id) {
            e.preventDefault();
            handleEnterCompare(item.id);
          }
          break;
        case "ArrowLeft":
          if (viewState.type === "compare") {
            e.preventDefault();
            stepCompareSlot(-1);
          } else if (getActiveMediaType(activeId) === "video" && videoRef.current) {
            e.preventDefault();
            videoRef.current.currentTime = Math.max(0, videoRef.current.currentTime - 5);
          } else if (currentIndex > 0) {
            onNavigate(currentIndex - 1);
          }
          break;
        case "ArrowRight":
          if (viewState.type === "compare") {
            e.preventDefault();
            stepCompareSlot(1);
          } else if (getActiveMediaType(activeId) === "video" && videoRef.current) {
            e.preventDefault();
            videoRef.current.currentTime = Math.min(
              videoRef.current.duration || Infinity,
              videoRef.current.currentTime + 5,
            );
          } else if (currentIndex < media.length - 1) {
            onNavigate(currentIndex + 1);
          }
          break;
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
  }, [currentIndex, media.length, onClose, onNavigate, viewState, activeId, getActiveMediaType, item, comparePinned, handleEnterCompare, stepCompareSlot]);

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
  const handleDoubleClick = useCallback(() => {
    if (videoRef.current) {
      if (videoRef.current.paused) {
        videoRef.current.play();
      } else {
        videoRef.current.pause();
      }
    } else {
      setScale(1);
      setOffset({ x: 0, y: 0 });
    }
  }, []);

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

  if (!item) return null;

  const mainUrl = viewState.type === "single" && viewState.activeId
    ? getFilePath(viewState.activeId)
    : null;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/95"
      onClick={() => {
        if (viewState.type === "compare") {
          setViewState({ type: "single", activeId: item?.id ?? null });
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

          {/* 裁剪 — 单张图片时可用。编辑器是独立全屏层，必须先把 Lightbox 关掉：
              它的图像容器带 translate/scale 变换，嵌进去指针坐标会全部错位。 */}
          {viewState.type === "single" && item?.media_type !== "video" && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                if (item?.id) onCrop(item.id);
                onClose();
              }}
              className="rounded border-l border-white/20 px-2 py-1 text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
              title="裁剪"
            >
              裁剪
            </button>
          )}

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

          {/* Compare pin button — single view only */}
          {viewState.type === "single" && (
            <div className="flex items-center gap-1 border-l border-white/20 pl-3">
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  if (comparePinned) {
                    setComparePinned(null);
                  } else {
                    setComparePinned(activeId ?? item?.id ?? null);
                  }
                }}
                className={`rounded px-2 py-1 text-xs transition-colors ${
                  comparePinned
                    ? "bg-[var(--color-accent)] text-white"
                    : "text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
                }`}
                title={comparePinned ? "已固定 A 槽 — Enter 与当前图对比，或点击底条选另一张" : "固定当前图片进行对比"}
              >
                {comparePinned ? "已固定" : "对比"}
              </button>
            </div>
          )}

          {/* Compare mode toolbar */}
          {viewState.type === "compare" && (
            <div className="flex items-center gap-1 border-l border-white/20 pl-3">
              <span className="text-xs text-white/50">对比</span>
              <button
                onClick={(e) => { e.stopPropagation(); setViewState({ ...viewState, mode: "side-by-side" }); }}
                className={`rounded px-2 py-0.5 text-xs transition-colors ${
                  viewState.mode === "side-by-side" ? "bg-white/20 text-white" : "text-white/50 hover:text-white/80"
                }`}
              >
                并排
              </button>
              <button
                onClick={(e) => { e.stopPropagation(); setViewState({ ...viewState, mode: "slider" }); }}
                className={`rounded px-2 py-0.5 text-xs transition-colors ${
                  viewState.mode === "slider" ? "bg-white/20 text-white" : "text-white/50 hover:text-white/80"
                }`}
              >
                叠加
              </button>

              {/* Swap left/right */}
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  setViewState(prev => prev.type === "compare"
                    ? { ...prev, leftId: prev.rightId, rightId: prev.leftId }
                    : prev
                  );
                }}
                className="rounded px-2 py-0.5 text-xs text-white/50 transition-colors hover:bg-white/10 hover:text-white/80"
                title="交换左右"
              >
                ⇄ 交换
              </button>

              {/* Exit compare */}
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  setViewState({ type: "single", activeId: item?.id ?? null });
                }}
                className="rounded px-2 py-0.5 text-xs text-white/50 transition-colors hover:bg-white/10 hover:text-white/80"
              >
                退出对比
              </button>

              <span className="ml-1 text-[11px] text-white/30">←/→ 换 B 槽 · Tab 切模式 · 点击空白退出</span>
            </div>
          )}
        </div>

        {/* Current item info */}
        <div className="flex items-center gap-2 text-xs text-white/50">
          <span>
            {item.width && item.height ? `${item.width} x ${item.height}` : ""}
          </span>
        </div>
      </div>

      {/* Main content area */}
      <div
        ref={containerRef}
        className={`absolute inset-0 ${
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
            <>
              {/* ──── Side-by-side ──── */}
              <div className="flex h-full w-full">
                <div className="flex-1 relative overflow-hidden border-r border-white/20">
                  <div className="pointer-events-none absolute left-0 right-0 top-2 z-10 text-center text-[11px] text-white/40">
                    {compareLeft?.slice(0, 8)}
                  </div>
                  {getActiveMediaType(compareLeft ?? null) === "video" ? (
                    <video src={getFilePath(compareLeft ?? null) ?? ""} controls className="absolute inset-0 w-full h-full object-contain" />
                  ) : (
                    <img src={getFilePath(compareLeft ?? null) ?? ""} alt="" className="absolute inset-0 w-full h-full object-contain" draggable={false} decoding="async" />
                  )}
                </div>
                <div className="flex-1 relative overflow-hidden">
                  <div className="pointer-events-none absolute left-0 right-0 top-2 z-10 text-center text-[11px] text-white/40">
                    {compareRight?.slice(0, 8)}
                  </div>
                  {getActiveMediaType(compareRight ?? null) === "video" ? (
                    <video src={getFilePath(compareRight ?? null) ?? ""} controls className="absolute inset-0 w-full h-full object-contain" />
                  ) : (
                    <img src={getFilePath(compareRight ?? null) ?? ""} alt="" className="absolute inset-0 w-full h-full object-contain" draggable={false} decoding="async" />
                  )}
                </div>
              </div>
            </>
          ) : (
            /* ──── Slider overlay ──── */
            <div className="relative h-full w-full">
              {getActiveMediaType(compareLeft ?? null) === "video" ? (
                <video
                  src={getFilePath(compareLeft ?? null) ?? ""}
                  className="absolute inset-0 w-full h-full object-contain"
                  style={{ clipPath: `inset(0 ${100 - sliderPos}% 0 0)` }}
                  controls
                />
              ) : (
                <img
                  src={getFilePath(compareLeft ?? null) ?? ""}
                  alt=""
                  className="absolute inset-0 w-full h-full object-contain"
                  style={{ clipPath: `inset(0 ${100 - sliderPos}% 0 0)` }}
                  draggable={false}
                  decoding="async"
                />
              )}
              {getActiveMediaType(compareRight ?? null) === "video" ? (
                <video
                  src={getFilePath(compareRight ?? null) ?? ""}
                  className="absolute inset-0 w-full h-full object-contain"
                  style={{ clipPath: `inset(0 0 0 ${sliderPos}%)` }}
                  controls
                />
              ) : (
                <img
                  src={getFilePath(compareRight ?? null) ?? ""}
                  alt=""
                  className="absolute inset-0 w-full h-full object-contain"
                  style={{ clipPath: `inset(0 0 0 ${sliderPos}%)` }}
                  draggable={false}
                  decoding="async"
                />
              )}
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
              <div className="pointer-events-none absolute bottom-24 left-0 right-0 text-center">
                <span className="rounded bg-black/50 px-2 py-1 text-[11px] text-white/50">
                  {compareLeft?.slice(0, 8)} ← → {compareRight?.slice(0, 8)}
                </span>
              </div>
              {item?.media_type === "video" && (
                <p className="absolute bottom-36 left-0 right-0 pointer-events-none text-center text-[11px] text-[var(--color-text-muted)]">
                  视频对比 — 叠加预览
                </p>
              )}
            </div>
          )
        ) : (
          /* ──── Single image/video with zoom/pan ──── */
          <div
            className="flex h-full w-full items-center justify-center"
            style={{ transform: `translate(${offset.x}px, ${offset.y}px) scale(${scale})` }}
          >
            {mainUrl && (getActiveMediaType(activeId) === "video" ? (
              <video
                ref={videoRef}
                src={mainUrl}
                controls
                autoPlay
                className="max-h-[90vh] max-w-[90vw] rounded-lg"
                onError={() => {
                  console.error("Video playback failed: codec or container not supported");
                }}
              />
            ) : (
              <img src={mainUrl} alt="" className="max-h-full max-w-full object-contain select-none" draggable={false} decoding="async" />
            ))}
          </div>
        )}
      </div>

      {/* Filmstrip — 单图模式导航；对比模式换 A/B 槽（窗口更大） */}
      {media.length > 1 && (
        <div className="absolute bottom-0 left-0 right-0 z-20 border-t border-white/10 bg-black/60 backdrop-blur-sm">
          <Filmstrip
            media={media}
            currentIndex={currentIndex}
            comparePinned={comparePinned}
            onNavigate={onNavigate}
            onEnterCompare={handleEnterCompare}
            compare={viewState.type === "compare"}
            leftId={compareLeft}
            rightId={compareRight}
            onSetSlot={handleSetSlot}
          />
        </div>
      )}

      {/* Pinned float card — pin 跨翻页持续有效（③） */}
      {viewState.type === "single" && comparePinned && (
        <PinnedCard pinnedId={comparePinned} onCancel={() => setComparePinned(null)} />
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
            className="absolute right-4 top-1/2 z-10 -translate-y-1/2 rounded-full bg-white/10 p-2 text-white/70 hover:bg-white/20 hover:text-white disabled:opacity-20"
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
