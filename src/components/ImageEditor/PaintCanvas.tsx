import { useEffect, useRef } from "react";
import { stampCenters, type Point, type Stroke } from "@/lib/editor/strokes";
import { createTip, drawStrokes, planOverlay, type DrawableStroke } from "@/lib/editor/compose";
import { originalFromWorking, workingFromClient, type Size } from "@/lib/editor/geometry";

export interface BrushSettings {
  /** 笔尖直径，原图像素 */
  size: number;
  /** 0..1，1 = 实心 */
  hardness: number;
  /** 0..1 */
  opacity: number;
  color: string;
  erase: boolean;
}

interface PaintCanvasProps {
  origSize: Size;
  work: Size;
  displaySize: Size;
  strokes: Stroke[];
  brush: BrushSettings;
  onCommitStroke: (stroke: Stroke) => void;
}

let seq = 0;
const nextId = () => `stroke-${seq++}`;

/** 小于这个原图距离的移动视为抖动，忽略（否则会产生海量重复落点）。 */
const MIN_MOVE_PX = 1;

/**
 * 画笔图层：一个工作分辨率的 canvas，按顺序重放笔迹。
 *
 * 预览与导出共用同一套绘制原语（`stampCenters` + `createTip`），所以所见即所存。
 * 涂抹过程中只补画**新增的**笔尖落点（`drawnRef`），避免每次 pointermove 都
 * 重放整幅；提交后由 useEffect 整体重放一次，结果与增量绘制一致。
 */
export function PaintCanvas({
  origSize,
  work,
  displaySize,
  strokes,
  brush,
  onCommitStroke,
}: PaintCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const draftRef = useRef<Stroke | null>(null);
  const drawnRef = useRef(0);
  const tipRef = useRef<{ key: string; tip: HTMLCanvasElement } | null>(null);

  // 重放已提交的笔迹（撤销/重做/清空也走这里）
  useEffect(() => {
    const ctx = canvasRef.current?.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, work.w, work.h);
    drawStrokes(ctx, planOverlay(strokes, origSize, work), document);
  }, [strokes, origSize.w, origSize.h, work.w, work.h]);

  const tipFor = (s: DrawableStroke) => {
    const key = `${s.size}|${s.hardness}|${s.color}`;
    if (tipRef.current?.key !== key) {
      tipRef.current = { key, tip: createTip(s.size, s.hardness, s.color, document) };
    }
    return tipRef.current.tip;
  };

  /** 指针客户端坐标 → 原图像素坐标（与原图空间笔迹模型对应）。 */
  const toOriginal = (e: React.PointerEvent): Point => {
    const el = canvasRef.current;
    if (!el) return { x: 0, y: 0 };
    const w = workingFromClient(e.clientX, e.clientY, el.getBoundingClientRect(), work);
    return originalFromWorking(w.x, w.y, work, origSize);
  };

  const asDrawable = (s: Stroke): DrawableStroke => {
    const scale = origSize.w > 0 ? work.w / origSize.w : 1;
    return {
      points: s.points.map((p) => ({ x: p.x * scale, y: p.y * scale })),
      size: Math.max(s.size * scale, 1),
      hardness: s.hardness,
      opacity: s.opacity,
      color: s.color,
      erase: s.erase,
    };
  };

  /** 只补画尚未绘制的落点。 */
  const paintNewStamps = () => {
    const ctx = canvasRef.current?.getContext("2d");
    const draft = draftRef.current;
    if (!ctx || !draft) return;
    const d = asDrawable(draft);
    const stamps = stampCenters(d.points, d.size);
    const tip = tipFor(d);
    const half = tip.width / 2;
    ctx.globalCompositeOperation = d.erase ? "destination-out" : "source-over";
    ctx.globalAlpha = d.opacity;
    for (let i = drawnRef.current; i < stamps.length; i++) {
      ctx.drawImage(tip, stamps[i].x - half, stamps[i].y - half);
    }
    drawnRef.current = stamps.length;
    ctx.globalAlpha = 1;
    ctx.globalCompositeOperation = "source-over";
  };

  const onPointerDown = (e: React.PointerEvent) => {
    e.preventDefault();
    e.currentTarget.setPointerCapture?.(e.pointerId);
    draftRef.current = { id: nextId(), points: [toOriginal(e)], ...brush };
    drawnRef.current = 0;
    paintNewStamps();
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const draft = draftRef.current;
    if (!draft) return;
    const p = toOriginal(e);
    const last = draft.points[draft.points.length - 1];
    if (Math.hypot(p.x - last.x, p.y - last.y) < MIN_MOVE_PX) return;
    draft.points.push(p);
    paintNewStamps();
  };

  const onPointerUp = (e: React.PointerEvent) => {
    e.currentTarget.releasePointerCapture?.(e.pointerId);
    const draft = draftRef.current;
    draftRef.current = null;
    // 提交原始指针点；重放时会用同样的 stampCenters 还原成一样的落点
    if (draft && draft.points.length > 0) onCommitStroke(draft);
  };

  return (
    <canvas
      ref={canvasRef}
      width={work.w}
      height={work.h}
      data-testid="paint-canvas"
      className="absolute inset-0 cursor-crosshair touch-none"
      style={{ width: displaySize.w, height: displaySize.h }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
    />
  );
}
