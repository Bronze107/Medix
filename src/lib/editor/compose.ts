/**
 * 导出合成的**计划**（纯函数）与执行（薄壳）。
 *
 * 计划里所有坐标都已换算到目标空间，所以 `planComposite` / `planOverlay`
 * 可以脱离 canvas 单测；`drawStrokes` / `renderComposite` 只是把计划翻译成
 * canvas 调用，不含判断逻辑。
 *
 * 蒙版的 alpha 处理不在这里 —— 那是 Phase 3（蒙版刷）的事。
 */

import { fullRect, type Rect, type Size } from "./geometry";
import { stampCenters, type Point, type Stroke } from "./strokes";

/** 已映射到某个目标空间的笔迹，可直接交给 canvas 绘制。 */
export interface DrawableStroke {
  points: Point[];
  size: number;
  hardness: number;
  opacity: number;
  color: string;
  erase: boolean;
}

export interface CompositePlan {
  /** 从原图的这个区域取像素（原图空间） */
  sourceRect: Rect;
  /** 输出画布尺寸 */
  outputSize: Size;
  strokes: DrawableStroke[];
  /**
   * true = 蒙版模式：笔迹表示「要重绘的区域」，导出时写进 **alpha 通道**
   * （alpha=0 处即 ComfyUI `LoadImage` 的 MASK=1）。
   * false = 画笔模式：笔迹是可见像素，用 source-over 画上去。
   */
  mask: boolean;
}

/**
 * 蒙版模式下垫在底图下面的不透明底色。
 *
 * 必须有这一步：原图自己带透明区时，`destination-out` 分不清「用户涂的」和
 * 「原本就透明的」，会把真实透明区一并当成重绘区。铺底之后 alpha 就只是蒙版的
 * 纯函数。代价是蒙版模式的合成结果不再保留原图透明度 —— 可以接受，重绘工作流
 * 本来就按 RGB 处理底图。
 */
const MASK_BASE_COLOR = "#ffffff";

/** 原图 → 目标空间的等比缩放系数（工作分辨率是原图的等比缩小，两轴同系数）。 */
function scaleOf(orig: Size, target: Size): number {
  return orig.w > 0 ? target.w / orig.w : 1;
}

/** 原图空间 → 工作空间的实时预览笔迹（不裁剪，偏移为 0）。 */
export function planOverlay(strokes: Stroke[], orig: Size, work: Size): DrawableStroke[] {
  const scale = scaleOf(orig, work);
  return strokes.map((s) => ({
    points: s.points.map((p) => ({ x: p.x * scale, y: p.y * scale })),
    // 笔尖直径是长度，只乘比例、不偏移
    size: Math.max(s.size * scale, 1),
    hardness: s.hardness,
    opacity: s.opacity,
    color: s.color,
    erase: s.erase,
  }));
}

/**
 * 把「裁剪矩形 + 原图空间笔迹」映射成输出空间的绘制计划。
 *
 * 笔迹超出裁剪区的部分由 canvas `clip()` 几何裁掉，**不能靠过滤落点** ——
 * 过滤会把落在框外的中间点丢掉，导致线段被拉直或断开。
 */
export function planComposite(
  strokes: Stroke[],
  crop: Rect | null,
  orig: Size,
  work: Size,
  mask: boolean,
): CompositePlan {
  const sourceRect = crop ?? fullRect(orig);
  const scale = scaleOf(orig, work);
  return {
    sourceRect,
    mask,
    outputSize: {
      w: Math.max(1, Math.round(sourceRect.w * scale)),
      h: Math.max(1, Math.round(sourceRect.h * scale)),
    },
    strokes: strokes.map((s) => ({
      points: s.points.map((p) => ({
        x: (p.x - sourceRect.x) * scale,
        y: (p.y - sourceRect.y) * scale,
      })),
      size: Math.max(s.size * scale, 1),
      hardness: s.hardness,
      opacity: s.opacity,
      color: s.color,
      erase: s.erase,
    })),
  };
}

/** 生成一个径向渐变笔尖的离屏画布（硬度 >= 1 时为实心圆）。 */
export function createTip(
  size: number,
  hardness: number,
  color: string,
  doc: Document,
): HTMLCanvasElement {
  const d = Math.max(1, Math.ceil(size));
  const c = doc.createElement("canvas");
  c.width = d;
  c.height = d;
  const ctx = c.getContext("2d");
  if (!ctx) return c;
  const r = d / 2;
  if (hardness >= 1) {
    ctx.fillStyle = color;
  } else {
    // 硬度 1 → 到边缘才渐隐；硬度 0 → 从圆心开始就渐隐
    const inner = r * Math.max(0, Math.min(hardness, 1));
    const g = ctx.createRadialGradient(r, r, inner, r, r, r);
    g.addColorStop(0, color);
    g.addColorStop(1, "transparent");
    ctx.fillStyle = g;
  }
  ctx.beginPath();
  ctx.arc(r, r, r, 0, Math.PI * 2);
  ctx.fill();
  return c;
}

/**
 * 顺序重放笔迹。实时预览层与导出合成都走这里，保证「所见即所存」。
 * 橡皮擦用 `destination-out`，重放顺序天然还原了「先画后擦 / 先擦后画」。
 */
export function drawStrokes(
  ctx: CanvasRenderingContext2D,
  strokes: DrawableStroke[],
  doc: Document,
): void {
  for (const s of strokes) {
    if (s.points.length === 0) continue;
    ctx.globalCompositeOperation = s.erase ? "destination-out" : "source-over";
    ctx.globalAlpha = Math.max(0, Math.min(1, s.opacity));

    const tip = createTip(s.size, s.hardness, s.color, doc);
    const half = tip.width / 2;
    // 记录的是原始指针点，落点在这里插值 —— 快速拖动才不会画成一串断点
    for (const p of stampCenters(s.points, s.size)) {
      ctx.drawImage(tip, p.x - half, p.y - half);
    }
  }
  ctx.globalAlpha = 1;
  ctx.globalCompositeOperation = "source-over";
}

/** 执行导出计划。`base` 为原图（HTMLImageElement）。 */
export function renderComposite(
  ctx: CanvasRenderingContext2D,
  plan: CompositePlan,
  base: CanvasImageSource,
  doc: Document,
): void {
  const { outputSize, sourceRect, mask } = plan;
  ctx.clearRect(0, 0, outputSize.w, outputSize.h);

  if (mask) {
    // 见 MASK_BASE_COLOR 注释：必须先铺不透明底，否则原图自带的透明区会被
    // 当成「要重绘」
    ctx.fillStyle = MASK_BASE_COLOR;
    ctx.fillRect(0, 0, outputSize.w, outputSize.h);
  }

  ctx.drawImage(
    base,
    sourceRect.x,
    sourceRect.y,
    sourceRect.w,
    sourceRect.h,
    0,
    0,
    outputSize.w,
    outputSize.h,
  );

  // 超出裁剪区的笔迹由 clip 几何裁掉（见 planComposite 注释）
  ctx.save();
  ctx.beginPath();
  ctx.rect(0, 0, outputSize.w, outputSize.h);
  ctx.clip();

  if (mask) {
    // 笔迹先画到独立图层，再整体 destination-out 到底图上。
    // 这样「橡皮擦」在图层内部用 destination-out 表达「取消涂抹」即可，
    // 不需要知道底图任何一个像素的颜色。
    const layer = doc.createElement("canvas");
    layer.width = outputSize.w;
    layer.height = outputSize.h;
    const lctx = layer.getContext("2d");
    if (lctx) {
      drawStrokes(lctx, plan.strokes, doc);
      ctx.globalCompositeOperation = "destination-out";
      ctx.drawImage(layer, 0, 0);
      ctx.globalCompositeOperation = "source-over";
    }
  } else {
    drawStrokes(ctx, plan.strokes, doc);
  }

  ctx.restore();
}

/** 把画布导出成 PNG data URL（无损，不做二次 JPEG 压缩）。 */
export function canvasToDataUrl(canvas: HTMLCanvasElement): string {
  return canvas.toDataURL("image/png");
}
