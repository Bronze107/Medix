import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { mediaGetPaths } from "@/lib/tauri";
import { ConfirmDialog } from "@/components/ConfirmDialog/ConfirmDialog";
import { CropOverlay } from "./CropOverlay";
import {
  clampFreeRect,
  fitAspectRect,
  fullRect,
  serializeCropRect,
  type Rect,
  type Size,
} from "@/lib/editor/geometry";

/** 常用宽高比。`null` 为自由裁剪，`"orig"` 取原图比例。 */
const ASPECT_PRESETS: { label: string; value: number | "orig" | null }[] = [
  { label: "自由", value: null },
  { label: "1:1", value: 1 },
  { label: "4:3", value: 4 / 3 },
  { label: "16:9", value: 16 / 9 },
  { label: "原图", value: "orig" },
];

export interface ImageEditorProps {
  mediaId: string;
  title?: string;
  /** 确认裁剪：参数为原图像素空间的整数矩形 */
  onConfirm: (rect: Rect) => void | Promise<void>;
  onCancel: () => void;
}

/**
 * 图像编辑器（Phase 1：仅裁剪）。
 *
 * 底图必须用**原图**（`mediaGetPaths().original`）而不是缩略图 —— 缩略图是
 * 256px 且 `object-cover` 裁过的，拿它当覆盖层底图坐标全错。
 *
 * 尺寸取自 `<img>` 的 `naturalWidth/Height`：浏览器会按 EXIF 方向旋转，所以这与
 * 用户看到的一致，也与 Rust 侧 `media::edit::open_oriented` 解码出的尺寸一致。
 * 注意**不能**用 `media.width/height`，那是导入时未应用方向的值。
 */
export function ImageEditor({ mediaId, title = "裁剪", onConfirm, onCancel }: ImageEditorProps) {
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [origSize, setOrigSize] = useState<Size | null>(null);
  const [stage, setStage] = useState<Size>({ w: 0, h: 0 });
  const [rect, setRect] = useState<Rect | null>(null);
  const [aspect, setAspect] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const stageRef = useRef<HTMLDivElement>(null);

  // 取原图路径
  useEffect(() => {
    let alive = true;
    mediaGetPaths(mediaId)
      .then((paths) => {
        if (!alive) return;
        if (!paths.original) {
          setError("找不到原图文件");
          return;
        }
        setUrl(convertFileSrc(paths.original));
      })
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [mediaId]);

  // 量取舞台尺寸（ResizeObserver 在 jsdom 里不存在，可选调用）
  useEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const measure = () => {
      const r = el.getBoundingClientRect();
      setStage({ w: r.width, h: r.height });
    };
    measure();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", measure);
      return () => window.removeEventListener("resize", measure);
    }
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [url]);

  // 图像显示框：原图等比放进舞台
  const displaySize: Size | null =
    origSize && stage.w > 0 && stage.h > 0
      ? fitInside(origSize, stage)
      : null;

  const onImageLoad = (e: React.SyntheticEvent<HTMLImageElement>) => {
    const img = e.currentTarget;
    const w = img.naturalWidth;
    const h = img.naturalHeight;
    if (!(w > 0) || !(h > 0)) {
      setError("无法读取图像尺寸");
      return;
    }
    const size = { w, h };
    setOrigSize(size);
    setRect((prev) => prev ?? fullRect(size));
  };

  const applyPreset = useCallback(
    (value: number | "orig" | null) => {
      if (!origSize || !rect) return;
      if (value === null) {
        setAspect(null);
        const c = clampFreeRect(rect, origSize);
        if (c) setRect(c);
        return;
      }
      const ratio = value === "orig" ? origSize.w / origSize.h : value;
      // 以当前框中心为中心，取框内能放下的最大等比矩形
      const w = Math.min(rect.w, rect.h * ratio);
      const h = w / ratio;
      const candidate: Rect = {
        x: rect.x + rect.w / 2 - w / 2,
        y: rect.y + rect.h / 2 - h / 2,
        w,
        h,
      };
      const fitted = fitAspectRect(candidate, origSize, ratio);
      if (fitted) {
        setAspect(ratio);
        setRect(fitted);
      }
    },
    [origSize, rect],
  );

  const handleChange = useCallback((next: Rect) => setRect(next), []);

  const serialized = origSize && rect ? serializeCropRect(rect, origSize) : null;
  // 与「整图」不同才算改过 —— 这样改比例、拖手柄都会算，而单纯点「自由」不会
  const untouched =
    !origSize ||
    !rect ||
    (rect.x === 0 && rect.y === 0 && rect.w === origSize.w && rect.h === origSize.h);

  const handleConfirm = async () => {
    if (!serialized || busy) return;
    setBusy(true);
    try {
      await onConfirm(serialized);
    } finally {
      setBusy(false);
    }
  };

  const requestCancel = () => {
    if (untouched) onCancel();
    else setConfirmDiscard(true);
  };

  return (
    <div
      className="fixed inset-0 z-[60] flex flex-col bg-[var(--color-bg-overlay)] animate-fade-in"
      data-testid="image-editor"
    >
      {/* Header */}
      <div className="flex shrink-0 items-center justify-between px-4 py-3">
        <span className="text-sm font-semibold text-[var(--color-text-primary)]">{title}</span>
        <button
          onClick={requestCancel}
          title="关闭 (Esc)"
          className="rounded-lg p-2 text-[var(--color-text-muted)] transition-colors hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)]"
        >
          <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      {/* Stage */}
      <div ref={stageRef} className="relative min-h-0 flex-1 px-4">
        {error && (
          <div className="flex h-full items-center justify-center text-xs text-[var(--color-danger)]">
            {error}
          </div>
        )}
        {!error && !url && (
          <div className="flex h-full items-center justify-center text-xs text-[var(--color-text-muted)]">
            加载图像中...
          </div>
        )}
        {url && (
          // 单个 <img> 常驻：尺寸未就绪时先以 1x1 隐藏挂载，靠 onLoad 读出
          // naturalWidth/Height 后再撑开。避免挂两个 img 触发两次加载。
          <div
            className="absolute"
            style={
              displaySize
                ? {
                    left: (stage.w - displaySize.w) / 2,
                    top: (stage.h - displaySize.h) / 2,
                    width: displaySize.w,
                    height: displaySize.h,
                  }
                : { left: 0, top: 0, width: 1, height: 1, opacity: 0 }
            }
          >
            <img
              src={url}
              alt=""
              onLoad={onImageLoad}
              onError={() => setError("图像加载失败")}
              className="h-full w-full select-none"
              draggable={false}
            />
            {origSize && rect && displaySize && (
              <CropOverlay
                origSize={origSize}
                displaySize={displaySize}
                rect={rect}
                aspect={aspect}
                onChange={handleChange}
              />
            )}
          </div>
        )}
      </div>

      {/* Toolbar */}
      <div className="flex shrink-0 flex-wrap items-center justify-center gap-2 px-4 py-3">
        <span className="text-[11px] text-[var(--color-text-muted)]">比例</span>
        {ASPECT_PRESETS.map((p) => {
          const active =
            p.value === null ? aspect === null : typeof p.value === "number" && aspect === p.value;
          return (
            <button
              key={p.label}
              onClick={() => applyPreset(p.value)}
              disabled={!origSize || !rect}
              className={`rounded px-2 py-1 text-[11px] transition-colors active:scale-[0.97] disabled:opacity-40 ${
                active
                  ? "bg-[var(--color-accent)] text-white"
                  : "border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              }`}
            >
              {p.label}
            </button>
          );
        })}
        {serialized && (
          <span className="ml-2 text-[11px] text-[var(--color-text-muted)]">
            {serialized.w} × {serialized.h}
          </span>
        )}
      </div>

      {/* Footer */}
      <div className="flex shrink-0 items-center justify-end gap-2 px-4 pb-4">
        <button
          onClick={requestCancel}
          className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] active:scale-[0.97]"
        >
          取消
        </button>
        <button
          onClick={handleConfirm}
          disabled={!serialized || busy}
          className="rounded bg-[var(--color-accent)] px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-[var(--color-accent-hover)] disabled:opacity-50 active:scale-[0.97]"
        >
          {busy ? "保存中..." : "确认"}
        </button>
      </div>

      <ConfirmDialog
        open={confirmDiscard}
        title="放弃编辑？"
        message="当前裁剪尚未保存，放弃后不会生成新版本。"
        confirmLabel="放弃"
        cancelLabel="继续编辑"
        variant="danger"
        onConfirm={() => {
          setConfirmDiscard(false);
          onCancel();
        }}
        onCancel={() => setConfirmDiscard(false)}
      />
    </div>
  );
}

/** 把 size 等比缩放到能放进 box 内。 */
function fitInside(size: Size, box: Size): Size {
  const scale = Math.min(box.w / size.w, box.h / size.h);
  return { w: Math.max(1, Math.round(size.w * scale)), h: Math.max(1, Math.round(size.h * scale)) };
}
