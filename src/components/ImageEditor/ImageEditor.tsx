import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { mediaGetPaths } from "@/lib/tauri";
import { ConfirmDialog } from "@/components/ConfirmDialog/ConfirmDialog";
import { CropOverlay } from "./CropOverlay";
import { PaintCanvas, type BrushSettings } from "./PaintCanvas";
import {
  clampFreeRect,
  computeWorkingSize,
  fitAspectRect,
  fullRect,
  serializeCropRect,
  type Rect,
  type Size,
} from "@/lib/editor/geometry";
import {
  canRedo,
  canUndo,
  historyReducer,
  initialHistory,
} from "@/lib/editor/document";
import { canvasToDataUrl, planComposite, renderComposite } from "@/lib/editor/compose";
import type { Stroke } from "@/lib/editor/strokes";

/** 常用宽高比。`null` 为自由裁剪，`"orig"` 取原图比例。 */
const ASPECT_PRESETS: { label: string; value: number | "orig" | null }[] = [
  { label: "自由", value: null },
  { label: "1:1", value: 1 },
  { label: "4:3", value: 4 / 3 },
  { label: "16:9", value: 16 / 9 },
  { label: "原图", value: "orig" },
];

/** 画笔大小的 UI 取值范围——按**工作空间**像素计，与实际图像分辨率无关。 */
const BRUSH_SIZE_MIN = 1;
const BRUSH_SIZE_MAX = 200;

export type EditorTool = "crop" | "paint" | "mask";

/**
 * library：裁剪 + 可见画笔，产物落库为新版本。
 * comfy-edit：裁剪 + 蒙版刷，产物喂给 ComfyUI 图生图（不落库）。
 */
export type EditorMode = "library" | "comfy-edit";

/**
 * 编辑结果。`crop` 交给 Rust 无损裁剪；`canvas` 是前端合成好的 PNG。
 * 在 library 模式下由「有哪些改动」决定（否则先画笔再切回裁剪，保存会把笔迹
 * 丢掉）；comfy-edit 模式恒为 `canvas`，因为蒙版必须随图一起送出。
 *
 * `hasMask` 供调用方判断「涂了蒙版但工作流并不消费 MASK」并给出警告。
 */
export type EditorResult =
  | { kind: "crop"; rect: Rect }
  | { kind: "canvas"; dataUrl: string; hasMask: boolean };

export interface ImageEditorProps {
  mediaId: string;
  /** 默认 library：编辑结果作为新版本落库 */
  mode?: EditorMode;
  title?: string;
  onConfirm: (result: EditorResult) => void | Promise<void>;
  onCancel: () => void;
}

/** 各模式下可用的工具。两种模式共用同一套画布与笔迹逻辑。 */
const TOOLS_BY_MODE: Record<EditorMode, { id: EditorTool; label: string }[]> = {
  library: [
    { id: "crop", label: "裁剪" },
    { id: "paint", label: "画笔" },
  ],
  "comfy-edit": [
    { id: "crop", label: "裁剪" },
    { id: "mask", label: "蒙版" },
  ],
};

/**
 * 图像编辑器：裁剪 + 画笔。
 *
 * 底图必须用**原图**（`mediaGetPaths().original`）而不是缩略图 —— 缩略图是
 * 256px 且 `object-cover` 裁过的，拿它当覆盖层底图坐标全错。
 *
 * 尺寸取自 `<img>` 的 `naturalWidth/Height`：浏览器会按 EXIF 方向旋转，所以这与
 * 用户看到的一致，也与 Rust 侧 `media::edit::open_oriented` 解码出的尺寸一致。
 * 注意**不能**用 `media.width/height`，那是导入时未应用方向的值。
 */
export function ImageEditor({
  mediaId,
  mode = "library",
  title = "编辑图片",
  onConfirm,
  onCancel,
}: ImageEditorProps) {
  const tools = TOOLS_BY_MODE[mode];
  const brushTool = mode === "comfy-edit" ? "mask" : "paint";
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [origSize, setOrigSize] = useState<Size | null>(null);
  const [stage, setStage] = useState<Size>({ w: 0, h: 0 });
  const [rect, setRect] = useState<Rect | null>(null);
  const [aspect, setAspect] = useState<number | null>(null);
  const [tool, setTool] = useState<EditorTool>("crop");
  const [busy, setBusy] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [brush, setBrush] = useState<BrushSettings>({
    size: 24,
    hardness: 0.8,
    opacity: 1,
    // 蒙版模式默认用显眼的红色：蒙版在导出结果里是**不可见**的（写进 alpha），
    // 屏幕上的颜色只是「我涂了哪里」的指示，选黑色会看不清
    color: mode === "comfy-edit" ? "#ff3b30" : "#000000",
    erase: false,
  });
  const [history, dispatch] = useReducer(historyReducer, undefined, initialHistory);
  const stageRef = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLImageElement>(null);

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

  const work = useMemo(
    () => (origSize ? computeWorkingSize(origSize.w, origSize.h) : null),
    [origSize],
  );

  // 图像显示框：原图等比放进舞台
  const displaySize: Size | null =
    origSize && stage.w > 0 && stage.h > 0 ? fitInside(origSize, stage) : null;

  const serialized = origSize && rect ? serializeCropRect(rect, origSize) : null;
  const cropTouched =
    !!origSize &&
    !!rect &&
    !(rect.x === 0 && rect.y === 0 && rect.w === origSize.w && rect.h === origSize.h);
  const hasStrokes = history.strokes.length > 0;
  const dirty = cropTouched || hasStrokes;

  const onImageLoad = (e: React.SyntheticEvent<HTMLImageElement>) => {
    const img = e.currentTarget;
    const w = img.naturalWidth;
    const h = img.naturalHeight;
    if (!(w > 0) || !(h > 0)) {
      setError("无法读取图像尺寸");
      return;
    }
    setOrigSize((prev) => prev ?? { w, h });
    setRect((prev) => prev ?? fullRect({ w, h }));
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
      const fitted = fitAspectRect(
        { x: rect.x + rect.w / 2 - w / 2, y: rect.y + rect.h / 2 - h / 2, w, h },
        origSize,
        ratio,
      );
      if (fitted) {
        setAspect(ratio);
        setRect(fitted);
      }
    },
    [origSize, rect],
  );

  const commitStroke = useCallback((stroke: Stroke) => {
    dispatch({ type: "addStroke", stroke });
  }, []);

  /** 画笔大小按工作空间给，落点存原图空间——这里换算一次。 */
  const recordBrush: BrushSettings =
    origSize && work ? { ...brush, size: brush.size * (origSize.w / work.w) } : brush;

  const buildCanvasResult = (): string | null => {
    const img = imgRef.current;
    if (!img || !origSize || !work) return null;
    // 笔迹不记录自己是画笔还是蒙版 —— 一种模式只有一种笔刷，所以由模式决定即可
    const mask = mode === "comfy-edit";
    const plan = planComposite(history.strokes, rect, origSize, work, mask);
    const canvas = document.createElement("canvas");
    canvas.width = plan.outputSize.w;
    canvas.height = plan.outputSize.h;
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    renderComposite(ctx, plan, img, document);
    return canvasToDataUrl(canvas);
  };

  const handleConfirm = async () => {
    if (busy || !serialized || !dirty) return;
    setBusy(true);
    try {
      // comfy-edit 模式恒走画布合成：蒙版必须随图一起送出去，哪怕只改了裁剪。
      // library 模式下有笔迹才走画布（否则笔迹会被丢掉），只有裁剪时走无损路径。
      if (mode === "comfy-edit" || hasStrokes) {
        let dataUrl: string | null = null;
        try {
          dataUrl = buildCanvasResult();
        } catch (e) {
          // 必须显式暴露：否则只是一句 console 报错 + 按钮毫无反应。
          // 只有 SecurityError 才说明是跨源污染，其它异常照实报，别乱归因。
          const security = (e as { name?: string })?.name === "SecurityError";
          setError(
            security
              ? "画布导出失败：图片被跨源污染，无法读取像素"
              : `画布导出失败：${String(e)}`,
          );
          return;
        }
        if (!dataUrl) {
          setError("合成失败");
          return;
        }
        // hasMask 指的是「涂了蒙版」，不是「有笔迹」—— library 模式的可见笔迹
        // 不是蒙版，误报会让调用方弹出无意义的「工作流不消费 MASK」警告
        const hasMask = mode === "comfy-edit" && hasStrokes;
        await onConfirm({ kind: "canvas", dataUrl, hasMask });
      } else {
        await onConfirm({ kind: "crop", rect: serialized });
      }
    } finally {
      setBusy(false);
    }
  };

  const requestCancel = () => {
    if (dirty) setConfirmDiscard(true);
    else onCancel();
  };

  // 键盘：Esc 取消，Ctrl+Z 撤销，Ctrl+Shift+Z / Ctrl+Y 重做
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        requestCancel();
        return;
      }
      if (!(e.ctrlKey || e.metaKey)) return;
      const k = e.key.toLowerCase();
      if (k === "z") {
        e.preventDefault();
        dispatch({ type: e.shiftKey ? "redo" : "undo" });
      } else if (k === "y") {
        e.preventDefault();
        dispatch({ type: "redo" });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const ready = !!url && !!origSize && !!displaySize && !!work && !!rect;

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
        {url && !error && (
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
              ref={imgRef}
              src={url}
              alt=""
              crossOrigin="anonymous"
              onLoad={onImageLoad}
              onError={() => setError("图像加载失败")}
              className="h-full w-full select-none"
              draggable={false}
            />
            {ready && tool === brushTool && (
              <PaintCanvas
                origSize={origSize}
                work={work}
                displaySize={displaySize}
                strokes={history.strokes}
                brush={recordBrush}
                onCommitStroke={commitStroke}
              />
            )}
            {ready && tool === "crop" && (
              <CropOverlay
                origSize={origSize}
                displaySize={displaySize}
                rect={rect}
                aspect={aspect}
                onChange={setRect}
              />
            )}
          </div>
        )}
      </div>

      {/* Toolbar */}
      <div className="flex shrink-0 flex-wrap items-center justify-center gap-2 px-4 py-2">
        {tools.map((t) => (
          <button
            key={t.id}
            onClick={() => setTool(t.id)}
            className={`rounded px-2.5 py-1 text-xs transition-colors active:scale-[0.97] ${
              tool === t.id
                ? "bg-[var(--color-accent)] text-white"
                : "border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
            }`}
          >
            {t.label}
          </button>
        ))}

        <span className="mx-1 h-4 w-px bg-[var(--color-border)]" />

        {tool === "crop" ? (
          <>
            <span className="text-[11px] text-[var(--color-text-muted)]">比例</span>
            {ASPECT_PRESETS.map((p) => {
              const active =
                p.value === null
                  ? aspect === null
                  : typeof p.value === "number" && aspect === p.value;
              return (
                <button
                  key={p.label}
                  onClick={() => applyPreset(p.value)}
                  disabled={!ready}
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
          </>
        ) : (
          <>
            <label className="flex items-center gap-1 text-[11px] text-[var(--color-text-muted)]">
              大小
              <input
                type="range"
                min={BRUSH_SIZE_MIN}
                max={BRUSH_SIZE_MAX}
                value={brush.size}
                aria-label="画笔大小"
                onChange={(e) => setBrush((b) => ({ ...b, size: Number(e.target.value) }))}
                className="w-24"
              />
              <span className="w-8 text-right text-[11px] text-[var(--color-text-secondary)]">
                {brush.size}
              </span>
            </label>
            <label className="flex items-center gap-1 text-[11px] text-[var(--color-text-muted)]">
              硬度
              <input
                type="range"
                min={0}
                max={100}
                value={Math.round(brush.hardness * 100)}
                aria-label="画笔硬度"
                onChange={(e) => setBrush((b) => ({ ...b, hardness: Number(e.target.value) / 100 }))}
                className="w-20"
              />
            </label>
            <label className="flex items-center gap-1 text-[11px] text-[var(--color-text-muted)]">
              不透明度
              <input
                type="range"
                min={0}
                max={100}
                value={Math.round(brush.opacity * 100)}
                aria-label="画笔不透明度"
                onChange={(e) => setBrush((b) => ({ ...b, opacity: Number(e.target.value) / 100 }))}
                className="w-20"
              />
            </label>
            <input
              type="color"
              value={brush.color}
              aria-label="画笔颜色"
              title={
                mode === "comfy-edit" ? "仅用于显示涂抹区域，不进入生成结果" : "画笔颜色"
              }
              disabled={brush.erase}
              onChange={(e) => setBrush((b) => ({ ...b, color: e.target.value }))}
              className="h-6 w-8 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] disabled:opacity-40"
            />
            <button
              onClick={() => setBrush((b) => ({ ...b, erase: !b.erase }))}
              aria-pressed={brush.erase}
              className={`rounded px-2 py-1 text-[11px] transition-colors active:scale-[0.97] ${
                brush.erase
                  ? "bg-[var(--color-accent)] text-white"
                  : "border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]"
              }`}
            >
              橡皮擦
            </button>
            <span className="mx-1 h-4 w-px bg-[var(--color-border)]" />
            <button
              onClick={() => dispatch({ type: "undo" })}
              disabled={!canUndo(history)}
              className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-[11px] text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] disabled:opacity-40"
            >
              撤销
            </button>
            <button
              onClick={() => dispatch({ type: "redo" })}
              disabled={!canRedo(history)}
              className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-[11px] text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] disabled:opacity-40"
            >
              重做
            </button>
            <button
              onClick={() => dispatch({ type: "clear" })}
              disabled={!hasStrokes}
              className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-[11px] text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] disabled:opacity-40"
            >
              清空
            </button>
          </>
        )}

        {serialized && (
          <span className="ml-1 text-[11px] text-[var(--color-text-muted)]">
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
          disabled={!ready || !dirty || busy}
          className="rounded bg-[var(--color-accent)] px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-[var(--color-accent-hover)] disabled:opacity-50 active:scale-[0.97]"
        >
          {busy ? "保存中..." : "保存新版本"}
        </button>
      </div>

      <ConfirmDialog
        open={confirmDiscard}
        title="放弃编辑？"
        message="当前编辑尚未保存，放弃后不会生成新版本。"
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
